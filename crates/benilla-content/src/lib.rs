//! Content streaming: the game-data archives ("packs", MPQs) a server offers in the extension
//! handshake, downloaded, checked and kept in benilla's state folder, then mounted above the
//! install's patch chain at the next start (`benilla_formats::set_overlay`). Never written into
//! the install. Deviation: 1.12.1 reads only its own `Data` folder.
//!
//! The server offers [`KEY_PORT`] and [`KEY_MANIFEST`], the manifest's SHA-256. Each object, the
//! manifest and every pack, is fetched as `GET /packs/<sha256>` over plain HTTP from the login
//! server's host on that port, and kept only if its SHA-256 matches. The manifest's hash arrives
//! over the logged-in game connection, so a pack is trusted exactly as far as the server is.
//!
//! The manifest is text: [`MANIFEST_HEADER`], then one line per pack in mount order (later wins):
//! `<sha256 hex> <size> <file name>`. The server half lives in the server's `crates/streaming`.
//!
//! The store, `benilla-config/content/`: `packs/<sha256>.mpq` (shared by every server offering
//! that pack) and `servers/<server key>.manifest`, the last manifest taken from each server.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{anyhow, bail, ensure, Context, Result};
use sha2::{Digest, Sha256};

/// The handshake key naming the port the packs are served on.
pub const KEY_PORT: &str = "content.port";
/// The handshake key carrying the manifest's SHA-256.
pub const KEY_MANIFEST: &str = "content.manifest";
/// The manifest's first line: the format and its version.
pub const MANIFEST_HEADER: &str = "benilla-packs 1";
/// The largest manifest read.
const MANIFEST_MAX: u64 = 1 << 20;
/// How long a connection or a read may stall.
const TIMEOUT: Duration = Duration::from_secs(30);

/// One pack the manifest lists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pack {
    pub sha256: String,
    pub size: u64,
    pub name: String,
}

/// A server's list of packs, in mount order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Manifest {
    pub packs: Vec<Pack>,
}

impl Manifest {
    pub fn parse(text: &[u8]) -> Result<Self> {
        let text = std::str::from_utf8(text).context("the manifest is not text")?;
        let mut lines = text.lines();
        ensure!(
            lines.next() == Some(MANIFEST_HEADER),
            "not a version 1 pack manifest"
        );
        let mut packs = Vec::new();
        for line in lines.filter(|l| !l.is_empty()) {
            let mut parts = line.splitn(3, ' ');
            let (Some(sha256), Some(size), Some(name)) = (parts.next(), parts.next(), parts.next())
            else {
                bail!("a manifest line is not `<sha256> <size> <name>`: {line}");
            };
            ensure!(is_sha256(sha256), "not a SHA-256: {sha256}");
            packs.push(Pack {
                sha256: sha256.to_ascii_lowercase(),
                size: size.parse().with_context(|| format!("size {size}"))?,
                name: name.to_string(),
            });
        }
        Ok(Self { packs })
    }
}

/// What a sync did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Synced {
    /// The server offers what is already kept.
    Unchanged,
    /// The server's list changed: the next start mounts it. `bytes` were downloaded.
    Updated { packs: usize, bytes: u64 },
}

/// A download under way: the pack, and bytes of it held so far.
#[derive(Clone, Debug)]
pub struct Progress<'a> {
    pub pack: &'a Pack,
    pub done: u64,
}

/// The server key of a realmlist: its first address with the default port spelled out, folded to
/// lowercase and to characters any file system takes, so one server keeps one manifest however
/// its address is written.
pub fn server_key(realmlist: &str) -> String {
    let (host, port) = split_host(realmlist);
    format!("{host}_{port}")
        .to_ascii_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// A realmlist's first host and its login port (3724 when none is written).
pub fn split_host(realmlist: &str) -> (String, u16) {
    let first = realmlist.split(';').next().unwrap_or("").trim();
    let (host, port) = match first.strip_prefix('[') {
        Some(rest) => {
            let (h, after) = rest.split_once(']').unwrap_or((rest, ""));
            (h, after.strip_prefix(':'))
        }
        None if first.matches(':').count() > 1 => (first, None),
        None => match first.split_once(':') {
            Some((h, p)) => (h, Some(p)),
            None => (first, None),
        },
    };
    (
        host.to_string(),
        port.and_then(|p| p.parse().ok()).unwrap_or(3724),
    )
}

/// The packs kept in `benilla-config/content/`.
#[derive(Clone, Debug)]
pub struct Store {
    root: PathBuf,
}

impl Store {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn pack_path(&self, sha256: &str) -> PathBuf {
        self.root.join("packs").join(format!("{sha256}.mpq"))
    }

    fn manifest_path(&self, server: &str) -> PathBuf {
        self.root.join("servers").join(format!("{server}.manifest"))
    }

    /// The kept manifest of `server`, if one is.
    pub fn manifest(&self, server: &str) -> Option<Manifest> {
        Manifest::parse(&std::fs::read(self.manifest_path(server)).ok()?).ok()
    }

    /// The archives to mount for `server`, in order: every pack its kept manifest lists, or none
    /// when one is missing or the wrong size, so the server's content mounts whole or not at all.
    pub fn mounts(&self, server: &str) -> Vec<PathBuf> {
        let Some(manifest) = self.manifest(server) else {
            return Vec::new();
        };
        let paths: Vec<PathBuf> = manifest
            .packs
            .iter()
            .map(|p| self.pack_path(&p.sha256))
            .collect();
        let whole = manifest.packs.iter().zip(&paths).all(|(p, path)| {
            std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.len() == p.size)
        });
        if whole {
            paths
        } else {
            Vec::new()
        }
    }

    /// Bring `server`'s packs up to what it offers: its manifest `manifest_sha256` and every pack
    /// in it, from `host:port`. Blocking; run it off the main thread.
    pub fn sync(
        &self,
        server: &str,
        host: &str,
        port: u16,
        manifest_sha256: &str,
        mut progress: impl FnMut(Progress<'_>),
    ) -> Result<Synced> {
        ensure!(
            is_sha256(manifest_sha256),
            "the offered manifest hash is not a SHA-256"
        );
        let want = manifest_sha256.to_ascii_lowercase();
        let kept = std::fs::read(self.manifest_path(server)).ok();
        let changed = kept.as_deref().map(sha256_hex) != Some(want.clone());
        if !changed && !self.mounts(server).is_empty() {
            return Ok(Synced::Unchanged);
        }
        let text = if changed {
            let mut text = Vec::new();
            get(host, port, &want, 0, MANIFEST_MAX, &mut text)?;
            ensure!(
                sha256_hex(&text) == want,
                "the manifest does not match the hash the server offered"
            );
            text
        } else {
            kept.unwrap_or_default()
        };
        let manifest = Manifest::parse(&text)?;
        std::fs::create_dir_all(self.root.join("packs"))?;
        let mut bytes = 0;
        for pack in &manifest.packs {
            bytes += self.fetch_pack(host, port, pack, &mut progress)?;
        }
        let path = self.manifest_path(server);
        std::fs::create_dir_all(self.root.join("servers"))?;
        let part = path.with_extension("manifest.part");
        std::fs::write(&part, &text)?;
        std::fs::rename(&part, &path)?;
        Ok(Synced::Updated {
            packs: manifest.packs.len(),
            bytes,
        })
    }

    /// One pack, kept or downloaded (resuming a partial one) and checked; the bytes downloaded.
    fn fetch_pack(
        &self,
        host: &str,
        port: u16,
        pack: &Pack,
        progress: &mut impl FnMut(Progress<'_>),
    ) -> Result<u64> {
        let path = self.pack_path(&pack.sha256);
        if std::fs::metadata(&path).is_ok_and(|m| m.len() == pack.size) {
            return Ok(0);
        }
        let part = path.with_extension("mpq.part");
        let mut have = std::fs::metadata(&part).map_or(0, |m| m.len());
        if have > pack.size {
            std::fs::remove_file(&part)?;
            have = 0;
        }
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&part)
            .with_context(|| format!("opening {}", part.display()))?;
        let start = have;
        let mut sink = ProgressWriter {
            file: &mut file,
            done: have,
            pack,
            progress,
        };
        let restarted = get(host, port, &pack.sha256, have, pack.size, &mut sink)?;
        if restarted {
            // The server sent the whole pack: what was held before is stale.
            let whole = std::fs::read(&part)?;
            std::fs::write(&part, &whole[have as usize..])?;
        }
        file.sync_all()?;
        drop(file);
        let size = std::fs::metadata(&part)?.len();
        let sha = sha256_file(&part)?;
        if size != pack.size || sha != pack.sha256 {
            let _ = std::fs::remove_file(&part);
            bail!(
                "{} arrived as {size} bytes with SHA-256 {sha}, not {} bytes with {}",
                pack.name,
                pack.size,
                pack.sha256
            );
        }
        std::fs::rename(&part, &path)?;
        Ok(size - if restarted { 0 } else { start })
    }
}

/// Writes a download to its file and reports how far it is.
struct ProgressWriter<'a, F: FnMut(Progress<'_>)> {
    file: &'a mut std::fs::File,
    done: u64,
    pack: &'a Pack,
    progress: &'a mut F,
}

impl<F: FnMut(Progress<'_>)> Write for ProgressWriter<'_, F> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let n = self.file.write(buf)?;
        self.done += n as u64;
        (self.progress)(Progress {
            pack: self.pack,
            done: self.done,
        });
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}

/// `GET /packs/<sha256>` from `host:port`, from byte `from`, at most `max` bytes in all, into
/// `out`. Whether the server ignored the range and sent the object whole.
fn get(
    host: &str,
    port: u16,
    sha256: &str,
    from: u64,
    max: u64,
    out: &mut impl Write,
) -> Result<bool> {
    let addr = (host, port)
        .to_socket_addrs()
        .with_context(|| format!("resolving {host}"))?
        .next()
        .ok_or_else(|| anyhow!("{host} has no address"))?;
    let mut stream = TcpStream::connect_timeout(&addr, TIMEOUT)
        .with_context(|| format!("connecting to {addr}"))?;
    stream.set_read_timeout(Some(TIMEOUT))?;
    let range = if from > 0 {
        format!("Range: bytes={from}-\r\n")
    } else {
        String::new()
    };
    write!(
        stream,
        "GET /packs/{sha256} HTTP/1.1\r\nHost: {host}\r\n{range}Connection: close\r\n\r\n"
    )?;
    let mut reader = std::io::BufReader::new(stream);
    let (status, length) = read_head(&mut reader)?;
    let whole = match status {
        200 => true,
        206 if from > 0 => false,
        _ => bail!("the server answered {status} for {sha256}"),
    };
    let length = length.ok_or_else(|| anyhow!("no Content-Length for {sha256}"))?;
    let start = if whole { 0 } else { from };
    ensure!(
        start + length <= max,
        "{sha256} is larger than the {max} bytes expected"
    );
    let copied = std::io::copy(&mut reader.take(length), out)?;
    ensure!(
        copied == length,
        "the download of {sha256} stopped at {copied} of {length} bytes"
    );
    Ok(whole && from > 0)
}

/// The status code and `Content-Length` of a response head.
fn read_head(reader: &mut impl std::io::BufRead) -> Result<(u16, Option<u64>)> {
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let status = line
        .split(' ')
        .nth(1)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| anyhow!("not an HTTP response: {}", line.trim()))?;
    let mut length = None;
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 || line == "\r\n" || line == "\n" {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            if name.trim().eq_ignore_ascii_case("content-length") {
                length = value.trim().parse().ok();
            }
        }
    }
    Ok((status, length))
}

fn is_sha256(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut file =
        std::fs::File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex(&hasher.finalize()))
}

#[cfg(test)]
mod tests;
