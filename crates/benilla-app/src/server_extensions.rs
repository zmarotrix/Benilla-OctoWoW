//! The app's half of the extension handshake ([`benilla_protocol::messages::extensions`]): whether
//! a login opens it, and what the server offered, for the features built on it to read.
//!
//! `serverExtensions` decides: `on` always, `off` never, `auto` (the default) only for a server on
//! this machine or a private network, so a public server sees nothing a stock client would not
//! send it. Deviation: 1.12.1 has no such handshake.

use benilla_protocol::messages::extensions::{client_extensions, Extensions};
use benilla_protocol::SessionEvent;
use bevy::prelude::*;

pub(crate) const CVAR: &str = "serverExtensions";

/// `serverExtensions`, parsed; anything unrecognised is [`Mode::Auto`].
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Mode {
    #[default]
    Auto,
    On,
    Off,
}

impl Mode {
    pub(crate) fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "on" | "1" => Self::On,
            "off" | "0" => Self::Off,
            _ => Self::Auto,
        }
    }

    /// Whether a login to the realmlist `host` opens the handshake.
    pub(crate) fn wanted(self, host: &str) -> bool {
        match self {
            Self::On => true,
            Self::Off => false,
            Self::Auto => is_local(host),
        }
    }
}

/// The realmlist's first host is this machine or on a private network.
fn is_local(realmlist: &str) -> bool {
    let first = realmlist.split(';').next().unwrap_or("").trim();
    let host = match first.strip_prefix('[') {
        Some(rest) => rest.split(']').next().unwrap_or(""),
        None if first.matches(':').count() > 1 => first,
        None => first.split(':').next().unwrap_or(""),
    };
    let host = host.to_ascii_lowercase();
    if host == "localhost" || host.ends_with(".localhost") || host.ends_with(".local") {
        return true;
    }
    match host.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V4(ip)) => ip.is_loopback() || ip.is_private() || ip.is_link_local(),
        Ok(std::net::IpAddr::V6(ip)) => ip.is_loopback() || (ip.segments()[0] & 0xfe00) == 0xfc00,
        Err(_) => false,
    }
}

/// What this client tells a server it supports.
pub(crate) fn ours() -> Extensions {
    client_extensions(&format!("benilla {}", env!("CARGO_PKG_VERSION")))
}

/// What the server of the current world link offered; `None` for a stock server or none asked.
#[derive(Resource, Default, Debug)]
pub(crate) struct ServerExtensions(pub(crate) Option<Extensions>);

fn sync_mode(cvars: Option<Res<crate::cvars::Cvars>>, mut mode: ResMut<Mode>) {
    let Some(cvars) = cvars else { return };
    let now = Mode::parse(cvars.get(CVAR).unwrap_or(""));
    if *mode != now {
        *mode = now;
    }
}

fn on_event(In(ev): In<SessionEvent>, mut offered: ResMut<ServerExtensions>) {
    match ev {
        SessionEvent::ServerExtensions { extensions } => {
            let list: Vec<String> = extensions
                .0
                .iter()
                .map(|(k, v)| {
                    if v.is_empty() {
                        k.clone()
                    } else {
                        format!("{k}={v}")
                    }
                })
                .collect();
            info!("net: server extensions: {}", list.join(", "));
            offered.0 = Some(extensions);
        }
        // A new world link starts with nothing offered.
        SessionEvent::WorldAuthenticated { .. } => offered.0 = None,
        _ => {}
    }
}

pub(crate) fn register(app: &mut App) {
    use crate::net::NetHandlerApp;
    use benilla_protocol::SessionEventKind as K;
    app.init_resource::<Mode>()
        .init_resource::<ServerExtensions>()
        .add_systems(PreUpdate, sync_mode)
        .net_handler(K::WorldAuthenticated, on_event)
        .net_handler(K::ServerExtensions, on_event);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_opens_the_handshake_only_on_this_machine_or_a_private_network() {
        for host in [
            "127.0.0.1",
            "127.0.0.1:3727",
            "localhost",
            "192.168.1.20:3724",
            "10.0.0.5",
            "[::1]:3724",
            "::1",
            "dev.local",
        ] {
            assert!(Mode::Auto.wanted(host), "{host}");
        }
        for host in ["play.octowow.st", "logon.example.org:3724", "8.8.8.8", ""] {
            assert!(!Mode::Auto.wanted(host), "{host}");
        }
        assert!(Mode::On.wanted("play.octowow.st"));
        assert!(!Mode::Off.wanted("127.0.0.1"));
    }

    #[test]
    fn the_setting_parses_leniently() {
        assert_eq!(Mode::parse("ON"), Mode::On);
        assert_eq!(Mode::parse(" off "), Mode::Off);
        assert_eq!(Mode::parse("0"), Mode::Off);
        assert_eq!(Mode::parse("auto"), Mode::Auto);
        assert_eq!(Mode::parse("bogus"), Mode::Auto);
    }
}
