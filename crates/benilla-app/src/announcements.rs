//! The server's announcements, kept between sessions for the login screen's feed: world broadcasts
//! (`SMSG_SERVER_MESSAGE`, shutdown and restart countdowns among them), server notices
//! (`SMSG_NOTIFICATION`) and the system lines a server's announce command sends
//! (`CHAT_MSG_SYSTEM` opening with a bracketed "server" or "announce" tag). Each is kept in
//! `benilla-config/announcements.tsv`, newest last, and the login screen's script reads them as
//! `BENILLA_ANNOUNCEMENTS`, newest first (`assets/ui/AccountLoginRealmlist.xml`). Deviation: 1.12.1
//! keeps none of these.

use benilla_protocol::{SessionEvent, SessionEventKind};
use benilla_ui::script::{UiScript, VmKind};
use bevy::prelude::*;

use crate::net::NetHandlerApp;

/// The history's length: the oldest goes past it.
const KEEP: usize = 40;
/// `CHAT_MSG_SYSTEM`'s type byte.
const CHAT_SYSTEM: u8 = 0x0A;

/// One announcement: when it arrived (Unix seconds) and its text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Announcement {
    pub(crate) at: i64,
    pub(crate) text: String,
}

/// The kept announcements, oldest first.
#[derive(Resource, Default)]
pub(crate) struct Announcements(pub(crate) Vec<Announcement>);

impl Announcements {
    /// Keep `text`, unless it repeats the newest (a countdown re-sent each minute keeps its own
    /// lines, as their texts differ).
    fn add(&mut self, at: i64, text: &str) {
        let text = text.trim();
        if text.is_empty() || self.0.last().is_some_and(|a| a.text == text) {
            return;
        }
        self.0.push(Announcement {
            at,
            text: text.to_string(),
        });
        let over = self.0.len().saturating_sub(KEEP);
        self.0.drain(..over);
    }

    fn to_tsv(&self) -> String {
        self.0
            .iter()
            .map(|a| format!("{}\t{}\n", a.at, a.text.replace(['\t', '\n', '\r'], " ")))
            .collect()
    }

    fn from_tsv(text: &str) -> Self {
        let mut out = Self::default();
        for line in text.lines() {
            if let Some((at, body)) = line.split_once('\t') {
                if let Ok(at) = at.parse() {
                    out.add(at, body);
                }
            }
        }
        out
    }
}

/// Whether a system line is an announce command's: a bracketed tag that names the server or an
/// announcement, as `[Server Announcement]`, `[SERVER]` or `[Announce]`.
fn is_announcement(text: &str) -> bool {
    let Some(rest) = text.trim_start().strip_prefix('[') else {
        return false;
    };
    let Some((tag, _)) = rest.split_once(']') else {
        return false;
    };
    let tag = tag.to_ascii_lowercase();
    tag.contains("server") || tag.contains("announce")
}

fn path() -> Option<std::path::PathBuf> {
    crate::local_state::home().map(|h| h.join("announcements.tsv"))
}

fn load(mut kept: ResMut<Announcements>) {
    if let Some(text) = path().and_then(|p| std::fs::read_to_string(p).ok()) {
        *kept = Announcements::from_tsv(&text);
    }
}

fn keep(kept: &mut Announcements, text: &str) {
    kept.add(benilla_ui::civil::unix_seconds(), text);
    if let Some(p) = path() {
        if let Err(e) = crate::local_state::write_atomic(&p, &kept.to_tsv()) {
            warn!("announcements: cannot write {}: {e}", p.display());
        }
    }
}

fn on_server_message(
    In(ev): In<SessionEvent>,
    messages: Option<Res<crate::ui_chat::ServerMessages>>,
    mut kept: ResMut<Announcements>,
) {
    if let (SessionEvent::ServerMessage { message_type, text }, Some(messages)) = (ev, messages) {
        keep(&mut kept, &messages.0.compose(message_type, &text));
    }
}

fn on_notification(In(ev): In<SessionEvent>, mut kept: ResMut<Announcements>) {
    if let SessionEvent::Notification { text } = ev {
        keep(&mut kept, &text);
    }
}

fn on_chat(In(ev): In<SessionEvent>, mut kept: ResMut<Announcements>) {
    if let SessionEvent::Chat(msg) = ev {
        if msg.chat_type == CHAT_SYSTEM && is_announcement(&msg.text) {
            keep(&mut kept, &msg.text);
        }
    }
}

/// The history as the login screen reads it, newest first, each with its local date and time.
fn feed_rows(kept: &Announcements) -> Vec<(String, String)> {
    kept.0
        .iter()
        .rev()
        .map(|a| {
            let (c, _) = benilla_ui::civil::local(a.at);
            let when = format!(
                "{}-{:02}-{:02} {:02}:{:02}",
                c.year, c.month, c.day, c.hour, c.min
            );
            (when, a.text.clone())
        })
        .collect()
}

/// Give a login screen VM the history when it is new or the history moved.
fn publish(
    script: Option<NonSend<UiScript>>,
    kept: Res<Announcements>,
    mut last: Local<Option<u64>>,
) {
    let Some(script) = script else {
        return;
    };
    if script.vm_kind() != VmKind::Glue {
        *last = None;
        return;
    }
    let session = script.session();
    if *last == Some(session) && !kept.is_changed() {
        return;
    }
    *last = Some(session);
    let lua = script.lua();
    let Ok(rows) = lua.create_table() else {
        return;
    };
    for (i, (when, text)) in feed_rows(&kept).into_iter().enumerate() {
        let Ok(row) = lua.create_table() else {
            return;
        };
        let _ = row.set("time", when);
        let _ = row.set("text", text);
        let _ = rows.set(i + 1, row);
    }
    let g = lua.globals();
    let _ = g.set("BENILLA_ANNOUNCEMENTS", rows);
    // The feed redraws when this count moves, at its next frame.
    let serial = g
        .get::<Option<i64>>("BENILLA_ANNOUNCEMENTS_SERIAL")
        .ok()
        .flatten()
        .unwrap_or(0);
    let _ = g.set("BENILLA_ANNOUNCEMENTS_SERIAL", serial + 1);
}

pub(crate) struct AnnouncementsPlugin;

impl Plugin for AnnouncementsPlugin {
    fn build(&self, app: &mut App) {
        use SessionEventKind as K;
        app.init_resource::<Announcements>()
            .add_systems(Startup, load)
            .add_systems(Last, publish)
            .net_handler(K::ServerMessage, on_server_message)
            .net_handler(K::Notification, on_notification)
            .net_handler(K::Chat, on_chat);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn announce_tags_are_kept_and_other_system_lines_are_not() {
        assert!(is_announcement(
            "[Server Announcement] Restart in 5 minutes"
        ));
        assert!(is_announcement("[SERVER] Double XP weekend"));
        assert!(is_announcement("  [Announce] Event at 8pm"));
        assert!(!is_announcement("You have learned a new spell"));
        assert!(!is_announcement("[Guild] Someone has come online"));
    }

    #[test]
    fn the_history_skips_repeats_caps_its_length_and_round_trips() {
        let mut kept = Announcements::default();
        kept.add(1, "Restart in 5 minutes");
        kept.add(2, "Restart in 5 minutes");
        kept.add(3, "Restart\tin 4\nminutes");
        assert_eq!(kept.0.len(), 2);
        for i in 0..50 {
            kept.add(10 + i, &format!("line {i}"));
        }
        assert_eq!(kept.0.len(), KEEP);
        assert_eq!(kept.0.last().unwrap().text, "line 49");
        let back = Announcements::from_tsv(&kept.to_tsv());
        assert_eq!(back.0, kept.0);
    }
}
