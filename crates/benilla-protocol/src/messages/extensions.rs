//! The extension handshake between benilla and a server that offers more than 1.12: the client's
//! `CMSG_LOTTERY_QUERY_OBSOLETE` with a body names what it supports, and such a server answers
//! with `SMSG_LOTTERY_QUERY_RESULT_OBSOLETE` naming what it offers. Deviation: 1.12.1 sends the
//! query empty, from `GetLastLotteryNumbers` only (`0x4c4229`), and no server answers it.
//!
//! A stock server already tolerates the query, since a stock client can send it (vmangos logs it
//! as never allowed and goes on, `WorldSession.cpp:609`). Only a client that sent the tagged query
//! is answered, so a stock client never sees the reply.
//!
//! Both bodies are the tag `BNLX`, the format version (u16), a count (u16), then that many
//! `key`/`value` pairs as NUL-terminated strings.

use std::collections::BTreeMap;

use super::opcode;

/// The client's half of the handshake.
pub const CMSG_EXTENSIONS: u16 = opcode::CMSG_LOTTERY_QUERY_OBSOLETE;
/// The server's half of the handshake.
pub const SMSG_EXTENSIONS: u16 = 0x0335;

/// The tag that sets a handshake body apart from the stock, empty lottery query.
pub const TAG: [u8; 4] = *b"BNLX";
/// The body format this build writes and reads.
pub const VERSION: u16 = 1;

/// One side's list: key to value. Keys are lowercase, `.`-separated words; an empty value is a
/// bare flag.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Extensions(pub BTreeMap<String, String>);

impl Extensions {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(String::as_str)
    }

    pub fn has(&self, key: &str) -> bool {
        self.0.contains_key(key)
    }

    pub fn insert(&mut self, key: &str, value: &str) {
        self.0.insert(key.to_string(), value.to_string());
    }

    /// The body: tag, version, count, pairs.
    pub fn encode(&self) -> Vec<u8> {
        let mut body = Vec::with_capacity(8 + self.0.len() * 16);
        body.extend_from_slice(&TAG);
        body.extend_from_slice(&VERSION.to_le_bytes());
        body.extend_from_slice(&(self.0.len().min(usize::from(u16::MAX)) as u16).to_le_bytes());
        for (k, v) in self.0.iter().take(usize::from(u16::MAX)) {
            for s in [k, v] {
                body.extend(s.bytes().filter(|&b| b != 0));
                body.push(0);
            }
        }
        body
    }

    /// A body this build reads: `None` for one without the tag (a stock lottery packet), of
    /// another version, or cut short.
    pub fn decode(body: &[u8]) -> Option<Self> {
        let rest = body.strip_prefix(&TAG)?;
        let version = u16::from_le_bytes(rest.get(..2)?.try_into().ok()?);
        if version != VERSION {
            return None;
        }
        let count = u16::from_le_bytes(rest.get(2..4)?.try_into().ok()?);
        let mut r = &rest[4..];
        let mut cstr = || -> Option<String> {
            let end = r.iter().position(|&b| b == 0)?;
            let s = String::from_utf8_lossy(&r[..end]).into_owned();
            r = &r[end + 1..];
            Some(s)
        };
        let mut out = BTreeMap::new();
        for _ in 0..count {
            let k = cstr()?;
            let v = cstr()?;
            out.insert(k, v);
        }
        Some(Self(out))
    }
}

/// The client's list: who it is and the extensions it can use.
pub fn client_extensions(client: &str) -> Extensions {
    let mut ext = Extensions::default();
    ext.insert("client", client);
    ext
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_list_round_trips() {
        let mut ext = Extensions::default();
        ext.insert("server", "Private Server 0.1");
        ext.insert("spell.queue.grace_ms", "150");
        ext.insert("content.stream", "");
        let body = ext.encode();
        assert_eq!(&body[..4], b"BNLX");
        assert_eq!(Extensions::decode(&body), Some(ext));
    }

    #[test]
    fn a_stock_lottery_body_or_a_cut_one_is_not_a_list() {
        assert_eq!(Extensions::decode(&[]), None, "the stock query is empty");
        assert_eq!(Extensions::decode(&[1, 2, 3, 4, 5, 6, 7, 8]), None);
        let mut body = client_extensions("benilla").encode();
        body.pop();
        assert_eq!(Extensions::decode(&body), None);
        let mut v2 = client_extensions("benilla").encode();
        v2[4] = 2;
        assert_eq!(Extensions::decode(&v2), None, "another format version");
    }
}
