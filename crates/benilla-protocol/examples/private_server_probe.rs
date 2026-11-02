//! Live probe: logon, realm list, character list and world entry against any server, then a
//! tally of what the first seconds in the world send and which packets did not parse.
//!
//! Needs `WOW_HOST` (`host[:port]`), `WOW_USER` and `WOW_PASS`; `WOW_CHAR` picks the character
//! (default: the first). `PROBE_SECONDS` sets how long it listens in the world (default 15).

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use benilla_protocol::messages::{extensions, ServerPacket};
use benilla_protocol::{logon, WorldSession, WORLD_PORT};

fn main() -> anyhow::Result<()> {
    let var = |k: &str| std::env::var(k).map_err(|_| anyhow::anyhow!("set {k}"));
    let (host, user, pass) = (var("WOW_HOST")?, var("WOW_USER")?, var("WOW_PASS")?);
    let seconds: u64 = std::env::var("PROBE_SECONDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(15);

    let l = logon(
        &host,
        &user,
        &pass,
        benilla_protocol::auth::LOCALE_TAG_EN_US,
    )?;
    println!("PASS logon: {} realm(s)", l.realms.len());
    for r in &l.realms {
        println!("  realm '{}' at {}", r.name, r.address);
    }
    let addr = l
        .realms
        .first()
        .map(|r| r.address.clone())
        .unwrap_or_else(|| format!("localhost:{WORLD_PORT}"));
    let mut s = WorldSession::connect(&addr, &user, l.session_key)?;
    println!("PASS world auth at {addr}");
    s.request_extensions(&extensions::client_extensions("benilla probe"))?;

    let chars = s.char_enum()?;
    println!(
        "PASS character list: {:?}",
        chars.iter().map(|c| &c.name).collect::<Vec<_>>()
    );
    match s.server_extensions() {
        Some(ext) => println!("PASS server extensions: {:?}", ext.0),
        None => println!("NOTE no extension answer (a stock server)"),
    }
    let target = match std::env::var("WOW_CHAR") {
        Ok(name) => chars.iter().find(|c| c.name.eq_ignore_ascii_case(&name)),
        Err(_) => chars.first(),
    }
    .ok_or_else(|| anyhow::anyhow!("no character to log in with"))?;

    s.player_login(target.guid)?;
    s.set_active_mover(target.guid)?;
    s.set_read_timeout(Some(Duration::from_millis(500)))?;
    let end = Instant::now() + Duration::from_secs(seconds);
    let (mut seen, mut unparsed) = (BTreeMap::<String, u32>::new(), BTreeMap::new());
    let mut entered = false;
    while Instant::now() < end {
        match s.recv() {
            Ok(packet) => {
                if let ServerPacket::LoginVerifyWorld { map, .. } = &packet {
                    println!("PASS world entry: '{}' on map {map}", target.name);
                    entered = true;
                }
                *seen.entry(packet.name().to_string()).or_default() += 1;
            }
            Err(e) if is_timeout(&e) => {}
            Err(e) => {
                let key = format!("{e:#}");
                let key: String = key.chars().take(160).collect();
                *unparsed.entry(key).or_insert(0u32) += 1;
                if !e.chain().any(|c| c.to_string().contains("parse")) && unparsed.len() > 50 {
                    return Err(e);
                }
            }
        }
    }
    if !entered {
        println!("FAIL: no SMSG_LOGIN_VERIFY_WORLD within {seconds} s");
    }
    println!("received:");
    for (name, n) in &seen {
        println!("  {n:5} {name}");
    }
    println!("errors:");
    for (e, n) in &unparsed {
        println!("  {n:5} {e}");
    }
    Ok(())
}

/// A read timeout, which the reader reports by its text on Windows (WSAETIMEDOUT).
fn is_timeout(e: &anyhow::Error) -> bool {
    e.chain().any(|c| {
        c.downcast_ref::<std::io::Error>().is_some_and(|io| {
            matches!(
                io.kind(),
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
            )
        }) || c.to_string().contains("did not properly respond")
    })
}
