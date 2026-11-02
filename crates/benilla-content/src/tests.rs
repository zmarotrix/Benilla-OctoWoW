use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use super::*;

/// Each request a test server took: the object's hash and the range start.
type Requests = Arc<Mutex<Vec<(String, u64)>>>;

/// A pack server like the real one, on a thread: objects by hash, `Range: bytes=N-` honoured.
/// Records each request's range start.
fn serve(objects: HashMap<String, Vec<u8>>) -> (u16, Requests) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let mut stream = stream.unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let sha = line
                .split(' ')
                .nth(1)
                .unwrap()
                .trim_start_matches("/packs/")
                .to_string();
            let mut from = 0u64;
            loop {
                let mut h = String::new();
                reader.read_line(&mut h).unwrap();
                if h == "\r\n" {
                    break;
                }
                if let Some(v) = h.strip_prefix("Range: bytes=") {
                    from = v.trim().trim_end_matches('-').parse().unwrap();
                }
            }
            log.lock().unwrap().push((sha.clone(), from));
            match objects.get(&sha) {
                Some(body) => {
                    let part = &body[from as usize..];
                    let status = if from > 0 {
                        "206 Partial Content"
                    } else {
                        "200 OK"
                    };
                    write!(
                        stream,
                        "HTTP/1.1 {status}\r\nContent-Length: {}\r\n\r\n",
                        part.len()
                    )
                    .unwrap();
                    stream.write_all(part).unwrap();
                }
                None => write!(
                    stream,
                    "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n"
                )
                .unwrap(),
            }
        }
    });
    (port, seen)
}

fn temp_store(tag: &str) -> (Store, PathBuf) {
    let dir = std::env::temp_dir().join(format!("benilla-content-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    (Store::new(&dir), dir)
}

/// A manifest over `packs` and the objects a server holds for it.
fn offer(packs: &[(&str, &[u8])]) -> (String, HashMap<String, Vec<u8>>) {
    let mut text = format!("{MANIFEST_HEADER}\n");
    let mut objects = HashMap::new();
    for (name, body) in packs {
        let sha = sha256_hex(body);
        text.push_str(&format!("{sha} {} {name}\n", body.len()));
        objects.insert(sha, body.to_vec());
    }
    let manifest_sha = sha256_hex(text.as_bytes());
    objects.insert(manifest_sha.clone(), text.into_bytes());
    (manifest_sha, objects)
}

#[test]
fn a_sync_downloads_checks_and_mounts_the_packs_once() {
    let (manifest_sha, objects) =
        offer(&[("patch-a.mpq", b"first pack"), ("patch-b.mpq", b"second")]);
    let (port, seen) = serve(objects);
    let (store, dir) = temp_store("sync");
    let key = server_key("127.0.0.1");
    assert!(
        store.mounts(&key).is_empty(),
        "nothing before the first sync"
    );
    let mut reports = 0;
    let synced = store
        .sync(&key, "127.0.0.1", port, &manifest_sha, |_| reports += 1)
        .unwrap();
    assert_eq!(
        synced,
        Synced::Updated {
            packs: 2,
            bytes: 16
        }
    );
    assert!(reports > 0);
    let mounts = store.mounts(&key);
    assert_eq!(mounts.len(), 2);
    assert_eq!(std::fs::read(&mounts[0]).unwrap(), b"first pack");
    assert_eq!(std::fs::read(&mounts[1]).unwrap(), b"second");
    let requests = seen.lock().unwrap().len();
    assert_eq!(
        store
            .sync(&key, "127.0.0.1", port, &manifest_sha, |_| ())
            .unwrap(),
        Synced::Unchanged
    );
    assert_eq!(
        seen.lock().unwrap().len(),
        requests,
        "an unchanged offer downloads nothing"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_partial_pack_resumes_where_it_stopped() {
    let body: &[u8] = b"0123456789abcdef";
    let (manifest_sha, objects) = offer(&[("patch-a.mpq", body)]);
    let (port, seen) = serve(objects);
    let (store, dir) = temp_store("resume");
    let sha = sha256_hex(body);
    std::fs::create_dir_all(dir.join("packs")).unwrap();
    std::fs::write(
        dir.join("packs").join(format!("{sha}.mpq.part")),
        &body[..10],
    )
    .unwrap();
    let synced = store
        .sync("s", "127.0.0.1", port, &manifest_sha, |_| ())
        .unwrap();
    assert_eq!(synced, Synced::Updated { packs: 1, bytes: 6 });
    assert!(seen.lock().unwrap().contains(&(sha.clone(), 10)));
    assert_eq!(std::fs::read(&store.mounts("s")[0]).unwrap(), body);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_manifest_or_pack_that_does_not_match_its_hash_is_refused() {
    let (manifest_sha, mut objects) = offer(&[("patch-a.mpq", b"good")]);
    // The server sends other bytes under the pack's hash.
    let sha = sha256_hex(b"good");
    objects.insert(sha.clone(), b"evil".to_vec());
    let (port, _) = serve(objects);
    let (store, dir) = temp_store("refuse");
    assert!(store
        .sync("s", "127.0.0.1", port, &manifest_sha, |_| ())
        .is_err());
    assert!(store.mounts("s").is_empty());
    assert!(
        !dir.join("packs").join(format!("{sha}.mpq.part")).exists(),
        "the bad download is dropped"
    );
    // An offered hash the manifest does not have.
    assert!(store
        .sync("s", "127.0.0.1", port, &"0".repeat(64), |_| ())
        .is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_kept_manifest_with_a_missing_pack_mounts_nothing() {
    let (store, dir) = temp_store("missing");
    std::fs::create_dir_all(dir.join("servers")).unwrap();
    std::fs::write(
        dir.join("servers").join("s.manifest"),
        format!("{MANIFEST_HEADER}\n{} 4 patch-a.mpq\n", "a".repeat(64)),
    )
    .unwrap();
    assert_eq!(store.manifest("s").unwrap().packs.len(), 1);
    assert!(store.mounts("s").is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn server_keys_name_one_server_however_its_address_is_written() {
    assert_eq!(server_key("127.0.0.1"), "127.0.0.1_3724");
    assert_eq!(server_key("127.0.0.1:3724"), "127.0.0.1_3724");
    assert_eq!(server_key("LocalHost:3726;backup"), "localhost_3726");
    assert_eq!(server_key("[::1]:3724"), "__1_3724");
    assert_eq!(
        split_host("play.example.org:3725"),
        ("play.example.org".to_string(), 3725)
    );
}

#[test]
fn a_manifest_parses_or_says_why_not() {
    let sha = "B".repeat(64);
    let m =
        Manifest::parse(format!("{MANIFEST_HEADER}\n{sha} 12 patch-Z.mpq\n").as_bytes()).unwrap();
    assert_eq!(
        m.packs,
        [Pack {
            sha256: "b".repeat(64),
            size: 12,
            name: "patch-Z.mpq".into()
        }]
    );
    assert!(Manifest::parse(b"benilla-packs 2\n").is_err());
    assert!(Manifest::parse(format!("{MANIFEST_HEADER}\nnothex 1 x\n").as_bytes()).is_err());
}
