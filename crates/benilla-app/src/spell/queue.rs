//! nampower's spell queue, which OctoWoW's client mods add to 1.12: a player's spell press inside
//! `NP_SpellQueueWindowMs` of the end of the cast in flight or of its own cooldown (the GCD
//! included) is held instead of refused, then sent through the same press path once the client
//! allows, so a caster chains spells with no latency gap. `SPELL_QUEUE_EVENT` signals the hold and
//! the send. Deviation: 1.12.1 refuses such a press (0x61, 0x3c).
//!
//! The state is shared behind one lock, so the ladder, the live reads and the send each read it
//! through `Res` and none of them conflicts with another.

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use bevy::prelude::*;

pub(crate) const CVAR_WINDOW: &str = "NP_SpellQueueWindowMs";
const CVAR_CAST_TIME: &str = "NP_QueueCastTimeSpells";
const CVAR_INSTANT: &str = "NP_QueueInstantSpells";
const CVAR_CHANNELING: &str = "NP_QueueChannelingSpells";

/// How long a held press waits past its moment for the client to allow it (an answer that is
/// slow to clear the in-flight slot) before it is dropped.
const PATIENCE: Duration = Duration::from_secs(2);

/// nampower's `SPELL_QUEUE_EVENT` codes: a GCD spell's hold and send, a spell's without a GCD.
pub(crate) const NORMAL_QUEUED: u8 = 2;
pub(crate) const NON_GCD_QUEUED: u8 = 4;

/// One held press: the spell, where it was aimed, and when it may go.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct QueuedCast {
    pub(crate) spell_id: u32,
    pub(crate) on_self: bool,
    /// The selection at the press; the send aims there if the selection moved since.
    pub(crate) unit: Option<u64>,
    pub(crate) ready_at: Instant,
    /// The hold's code; its send signals the next one.
    pub(crate) code: u8,
}

#[derive(Default)]
struct State {
    queued: Option<QueuedCast>,
    /// `CastSpellByNameNoQueue`: the next press bypasses the queue.
    skip_next: bool,
    /// The held press is being sent: its own ladder must not hold it again.
    sending: bool,
    /// `SPELL_QUEUE_EVENT`s owed, `(code, spell)`.
    events: Vec<(u8, u32)>,
    /// When our last cast with a cast time ends, off its `SMSG_SPELL_START` and delays; read only
    /// while the in-flight slot holds a cast.
    cast_end: Option<Instant>,
}

/// The queue ([`State`] behind its lock).
#[derive(Resource, Clone, Default)]
pub(crate) struct SpellQueue(Arc<Mutex<State>>);

impl SpellQueue {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The held spell's id, if one is.
    pub(crate) fn peek(&self) -> Option<u32> {
        self.lock().queued.map(|q| q.spell_id)
    }

    /// `/unqueue`: drop the held press.
    pub(crate) fn clear(&self) {
        self.lock().queued = None;
    }

    pub(crate) fn skip_next(&self, on: bool) {
        self.lock().skip_next = on;
    }

    /// What is left of our cast in flight, nothing for one with no cast time.
    pub(crate) fn cast_left(&self, now: Instant) -> std::time::Duration {
        self.lock()
            .cast_end
            .map_or(std::time::Duration::ZERO, |end| {
                end.saturating_duration_since(now)
            })
    }

    /// The press `spell_id` reached the ladder, which would refuse it after `wait`: whether it is
    /// held instead. A press that is not held replaces nothing; one that casts drops the hold
    /// ([`Self::pressed`]).
    pub(crate) fn offer(
        &self,
        settings: &QueueSettings,
        press: Press,
        wait: Option<Duration>,
        now: Instant,
    ) -> bool {
        let mut state = self.lock();
        if state.sending {
            return false;
        }
        if std::mem::take(&mut state.skip_next) {
            return false;
        }
        let Some(wait) = wait else {
            return false;
        };
        if !settings.allows(press.kind) || wait > settings.window {
            return false;
        }
        if state.queued.is_some_and(|q| q.spell_id == press.spell_id) {
            return true;
        }
        let code = if press.gcd {
            NORMAL_QUEUED
        } else {
            NON_GCD_QUEUED
        };
        state.queued = Some(QueuedCast {
            spell_id: press.spell_id,
            on_self: press.on_self,
            unit: press.unit,
            ready_at: now + wait,
            code,
        });
        state.events.push((code, press.spell_id));
        true
    }

    /// A press went to the ladder unheld: a held one behind it is stale.
    pub(crate) fn pressed(&self) {
        let mut state = self.lock();
        if !state.sending {
            state.queued = None;
        }
    }

    /// The held press whose moment came, taken for its send; `None` before then. One that waited
    /// past [`PATIENCE`] is dropped.
    fn take_due(&self, now: Instant, clear: bool) -> Option<QueuedCast> {
        let mut state = self.lock();
        let q = state.queued?;
        if now >= q.ready_at + PATIENCE {
            state.queued = None;
            return None;
        }
        if now < q.ready_at || !clear {
            return None;
        }
        state.queued = None;
        state.events.push((q.code + 1, q.spell_id));
        Some(q)
    }

    fn sending(&self, on: bool) {
        self.lock().sending = on;
    }

    fn take_events(&self) -> Vec<(u8, u32)> {
        std::mem::take(&mut self.lock().events)
    }
}

/// What the queue reads of a press.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Press {
    pub(crate) spell_id: u32,
    pub(crate) on_self: bool,
    pub(crate) unit: Option<u64>,
    pub(crate) kind: SpellKind,
    /// The spell starts the global cooldown.
    pub(crate) gcd: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SpellKind {
    Instant,
    CastTime,
    Channeled,
}

/// The queue's CVars, mirrored so the ladder never reads the CVar table.
#[derive(Resource, Clone, Copy, Debug)]
pub(crate) struct QueueSettings {
    window: Duration,
    cast_time: bool,
    instant: bool,
    channeled: bool,
}

impl Default for QueueSettings {
    fn default() -> Self {
        Self {
            window: Duration::from_millis(500),
            cast_time: true,
            instant: true,
            channeled: true,
        }
    }
}

impl QueueSettings {
    fn allows(&self, kind: SpellKind) -> bool {
        !self.window.is_zero()
            && match kind {
                SpellKind::Instant => self.instant,
                SpellKind::CastTime => self.cast_time,
                SpellKind::Channeled => self.channeled,
            }
    }
}

fn sync_settings(cvars: Option<Res<crate::cvars::Cvars>>, mut settings: ResMut<QueueSettings>) {
    let Some(cvars) = cvars.as_deref() else {
        return;
    };
    let on = |name: &str| cvars.get(name).is_none_or(|v| v.trim() != "0");
    let window = cvars
        .get(CVAR_WINDOW)
        .and_then(|v| v.trim().parse::<f64>().ok())
        .map_or(500, |ms| ms.clamp(0.0, 5000.0) as u64);
    let now = QueueSettings {
        window: Duration::from_millis(window),
        cast_time: on(CVAR_CAST_TIME),
        instant: on(CVAR_INSTANT),
        channeled: on(CVAR_CHANNELING),
    };
    if settings.window != now.window
        || settings.cast_time != now.cast_time
        || settings.instant != now.instant
        || settings.channeled != now.channeled
    {
        *settings = now;
    }
}

/// Our cast's start and its pushbacks, which time the queue's wait.
fn on_own_cast(
    In(ev): In<benilla_protocol::SessionEvent>,
    me: Res<crate::net::SelfGuid>,
    queue: Res<SpellQueue>,
) {
    use benilla_protocol::SessionEvent;
    let now = Instant::now();
    match ev {
        SessionEvent::SpellStart {
            caster,
            cast_time_ms,
            ..
        } if me.0 == Some(caster) => {
            queue.lock().cast_end =
                Some(now + std::time::Duration::from_millis(u64::from(cast_time_ms)));
        }
        SessionEvent::SpellDelayed { caster, delay_ms } if me.0 == Some(caster) => {
            if let Some(end) = queue.lock().cast_end.as_mut() {
                *end += std::time::Duration::from_millis(u64::from(delay_ms));
            }
        }
        _ => {}
    }
}

/// Send the held press whose moment came, through the press path a script's `CastSpell` takes,
/// once nothing is in flight; then signal the queue's events.
fn send_due(
    queue: Res<SpellQueue>,
    mut lane: ResMut<crate::superwow::SuperWowLane>,
    mut vm: crate::script_calls::VmWith<crate::spell::ScriptCast<'static, 'static>>,
) {
    let now = Instant::now();
    let due = vm.turn(|_, cast| {
        let clear = cast.ladder.pending.current().is_none() && cast.ladder.ground.spell().is_none();
        let q = queue.take_due(now, clear)?;
        // Aimed where the press was: the selection if it still is, else that unit.
        let unit = q
            .unit
            .filter(|_| !q.on_self)
            .filter(|&g| cast.targeting.selection.guid != Some(g));
        Some((q, unit))
    });
    if let Some((q, unit)) = due {
        debug!("spell queue: sending the held {}", q.spell_id);
        queue.sending(true);
        vm.engine_cast_spell(q.spell_id, q.on_self, unit, |mut cast| {
            crate::ui_spellbook::cast_spell(&mut cast, q.spell_id, q.on_self, unit);
        });
        queue.sending(false);
    }
    for (code, spell) in queue.take_events() {
        lane.raise(crate::nampower_events::queue_event(code, spell));
    }
}

pub(crate) fn register(app: &mut App) {
    use crate::net::NetHandlerApp;
    use benilla_protocol::SessionEventKind as K;
    app.init_resource::<SpellQueue>()
        .net_handler(K::SpellStart, on_own_cast)
        .net_handler(K::SpellDelayed, on_own_cast)
        .init_resource::<QueueSettings>()
        .add_systems(PreUpdate, sync_settings)
        // After the frame's packets and calls, outside the dispatch: the send is the player's
        // press, made late.
        .add_systems(Last, send_due.run_if(crate::ui_script::ingame_ui_up));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(spell_id: u32) -> Press {
        Press {
            spell_id,
            on_self: false,
            unit: Some(7),
            kind: SpellKind::CastTime,
            gcd: true,
        }
    }

    #[test]
    fn a_press_inside_the_window_is_held_and_sent_when_due() {
        let q = SpellQueue::default();
        let s = QueueSettings::default();
        let now = Instant::now();
        assert!(q.offer(&s, press(133), Some(Duration::from_millis(300)), now));
        assert_eq!(q.peek(), Some(133));
        assert_eq!(q.take_due(now, true), None, "not before its moment");
        let at = now + Duration::from_millis(300);
        assert_eq!(
            q.take_due(at, false),
            None,
            "not while something is in flight"
        );
        assert_eq!(q.take_due(at, true).map(|c| c.spell_id), Some(133));
        assert_eq!(q.peek(), None);
        assert_eq!(
            q.take_events(),
            vec![(NORMAL_QUEUED, 133), (NORMAL_QUEUED + 1, 133)]
        );
    }

    #[test]
    fn a_press_outside_the_window_or_with_nothing_ahead_is_not_held() {
        let q = SpellQueue::default();
        let s = QueueSettings::default();
        let now = Instant::now();
        assert!(!q.offer(&s, press(133), Some(Duration::from_millis(900)), now));
        assert!(!q.offer(&s, press(133), None, now));
        assert_eq!(q.peek(), None);
        assert!(q.take_events().is_empty());
    }

    #[test]
    fn skip_next_bypasses_one_press_and_unqueue_drops_the_hold() {
        let q = SpellQueue::default();
        let s = QueueSettings::default();
        let now = Instant::now();
        let wait = Some(Duration::from_millis(100));
        q.skip_next(true);
        assert!(!q.offer(&s, press(133), wait, now));
        assert!(q.offer(&s, press(133), wait, now));
        q.clear();
        assert_eq!(q.peek(), None);
    }

    #[test]
    fn a_later_press_replaces_the_hold_and_a_stale_one_expires() {
        let q = SpellQueue::default();
        let s = QueueSettings::default();
        let now = Instant::now();
        let wait = Some(Duration::from_millis(100));
        assert!(q.offer(&s, press(133), wait, now));
        assert!(q.offer(&s, press(116), wait, now));
        assert_eq!(q.peek(), Some(116));
        assert_eq!(
            q.take_due(now + PATIENCE + Duration::from_millis(200), false),
            None
        );
        assert_eq!(q.peek(), None);
    }

    #[test]
    fn a_held_press_sending_is_not_held_again() {
        let q = SpellQueue::default();
        let s = QueueSettings::default();
        let now = Instant::now();
        q.sending(true);
        assert!(!q.offer(&s, press(133), Some(Duration::ZERO), now));
        q.sending(false);
    }

    #[test]
    fn a_disabled_kind_is_not_held() {
        let q = SpellQueue::default();
        let s = QueueSettings {
            instant: false,
            ..QueueSettings::default()
        };
        let mut p = press(1752);
        p.kind = SpellKind::Instant;
        p.gcd = false;
        assert!(!q.offer(&s, p, Some(Duration::from_millis(100)), Instant::now()));
    }
}
