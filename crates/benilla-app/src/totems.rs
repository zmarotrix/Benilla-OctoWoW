//! Two player states ClassicAPI adds to 1.12 for OctoWoW's addons and the smart macros: the totem
//! bar (`GetTotemInfo`, `[totem]`), timed from each `SMSG_SPELL_GO` of ours that summons a totem,
//! and the spell-school interrupt lockout (`C_LossOfControl`, `[locked]`), read off the
//! `SMSG_SPELL_COOLDOWN` the server sends when a Counterspell, Kick, Pummel or Earth Shock locks a
//! school. Deviation: the reference shows neither.

use benilla_protocol::{SessionEvent, SessionEventKind};
use bevy::prelude::*;

use benilla_ui::script::live::LiveAsk;
use benilla_ui::script::{SchoolLockoutQuery, TotemQuery, TotemSlot};

use crate::net::{NetHandlerApp, SelfGuid};
use crate::ui_action::Spells;

/// `Spell.dbc` columns read here.
const COL_DURATION_INDEX: usize = 30;
const COL_EFFECT_1: usize = 61;
/// `SUMMON_TOTEM_SLOT1`..`4`: fire, earth, water, air.
const SUMMON_TOTEM_FIRST: u32 = 87;
const TOTEM_SLOTS: usize = 4;
/// A lockout is short; a longer cooldown list is a cast's own.
const LOCKOUT_MAX_MS: u32 = 10_000;
/// How long after our own cast its cooldown list may arrive.
const OWN_LIST_WINDOW: f64 = 1.0;

/// One totem down: its spell and when it went down and ends, in `Time::elapsed_secs_f64`.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Totem {
    spell: u32,
    start: f64,
    /// `None` for a summon with no duration row.
    end: Option<f64>,
}

#[derive(Resource, Default)]
pub(crate) struct PlayerTotems([Option<Totem>; TOTEM_SLOTS]);

/// The locked school mask and when the lockout ends.
#[derive(Resource, Default)]
pub(crate) struct SchoolLockout {
    mask: u32,
    end: f64,
    /// Our last cast and when it went, whose own cooldown list is no lockout.
    last_cast: Option<(u32, f64)>,
}

fn on_spell_go(
    In(ev): In<SessionEvent>,
    me: Res<SelfGuid>,
    spells: Option<Res<Spells>>,
    time: Res<Time>,
    mut totems: ResMut<PlayerTotems>,
    mut lockout: ResMut<SchoolLockout>,
) {
    let SessionEvent::SpellGo {
        caster, spell_id, ..
    } = ev
    else {
        return;
    };
    if me.0 != Some(caster) {
        return;
    }
    let now = time.elapsed_secs_f64();
    lockout.last_cast = Some((spell_id, now));
    let Some(spells) = spells.as_deref() else {
        return;
    };
    let Some(row) = spells.catalog.row(spell_id) else {
        return;
    };
    let col = |c: usize| row.columns.get(c).copied().unwrap_or(0);
    let duration = spells
        .durations
        .get(col(COL_DURATION_INDEX))
        .filter(|d| !d.is_permanent() && d.base_ms > 0)
        .map(|d| f64::from(d.base_ms) / 1000.0);
    for k in 0..3 {
        let slot = col(COL_EFFECT_1 + k).wrapping_sub(SUMMON_TOTEM_FIRST) as usize;
        if let Some(cell) = totems.0.get_mut(slot) {
            *cell = Some(Totem {
                spell: spell_id,
                start: now,
                end: duration.map(|d| now + d),
            });
        }
    }
}

/// A cooldown list for us that is not our last cast's: the school lockout an interrupt puts on
/// every spell of the school, each for the lockout's time.
fn on_cooldowns(
    In(ev): In<SessionEvent>,
    me: Res<SelfGuid>,
    spells: Option<Res<Spells>>,
    time: Res<Time>,
    mut lockout: ResMut<SchoolLockout>,
) {
    let SessionEvent::SpellCooldowns { caster, cooldowns } = ev else {
        return;
    };
    if me.0 != Some(caster) {
        return;
    }
    let now = time.elapsed_secs_f64();
    let own = lockout
        .last_cast
        .filter(|(_, at)| now - at <= OWN_LIST_WINDOW)
        .map(|(spell, _)| spell);
    if let Some((mask, ms)) = lockout_of(&cooldowns, own, |id| {
        spells.as_deref()?.catalog.get(id).map(|d| d.school)
    }) {
        lockout.mask = mask;
        lockout.end = now + f64::from(ms) / 1000.0;
    }
}

/// The schools a cooldown list locks and for how long: every entry short and of one length, none
/// the cast `own` that may have sent it.
fn lockout_of(
    pairs: &[(u32, u32)],
    own: Option<u32>,
    school: impl Fn(u32) -> Option<u32>,
) -> Option<(u32, u32)> {
    let &(_, ms) = pairs.first()?;
    if ms == 0 || ms > LOCKOUT_MAX_MS {
        return None;
    }
    if pairs.iter().any(|&(id, m)| m != ms || Some(id) == own) {
        return None;
    }
    let mask = pairs
        .iter()
        .filter_map(|&(id, _)| school(id))
        .filter(|&s| s < 32)
        .fold(0u32, |m, s| m | (1 << s));
    (mask != 0).then_some((mask, ms))
}

fn on_session_end(
    In(_): In<SessionEvent>,
    mut totems: ResMut<PlayerTotems>,
    mut lockout: ResMut<SchoolLockout>,
) {
    *totems = PlayerTotems::default();
    *lockout = SchoolLockout::default();
}

/// What the totem bar and lockout reads look at.
#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct TotemLive<'w> {
    totems: Res<'w, PlayerTotems>,
    lockout: Res<'w, SchoolLockout>,
    spells: Option<Res<'w, Spells>>,
    time: Res<'w, Time>,
}

impl crate::script_calls::live::LiveFamily for TotemLive<'_> {
    fn answer(&mut self, ask: &mut LiveAsk<'_>) -> bool {
        let now = self.time.elapsed_secs_f64();
        ask.answer(|q: &TotemQuery| {
            let slot = usize::from(q.0).checked_sub(1)?;
            let totem = self
                .totems
                .0
                .get(slot)
                .copied()
                .flatten()
                .filter(|t| t.end.is_none_or(|end| end > now));
            let Some(t) = totem else {
                return Some(None);
            };
            let spells = self.spells.as_deref();
            Some(Some(TotemSlot {
                spell: t.spell,
                name: spells
                    .and_then(|s| s.catalog.get(t.spell))
                    .map(|d| d.name.clone())
                    .unwrap_or_default(),
                icon: spells
                    .and_then(|s| s.catalog.icon_path(t.spell))
                    .unwrap_or_default()
                    .to_owned(),
                elapsed: now - t.start,
                duration: t.end.map_or(0.0, |end| end - t.start),
            }))
        }) || ask.answer(|_: &SchoolLockoutQuery| {
            let left = self.lockout.end - now;
            Some(if left > 0.0 && self.lockout.mask != 0 {
                (self.lockout.mask, left)
            } else {
                (0, 0.0)
            })
        })
    }
}

pub(crate) struct TotemsPlugin;

impl Plugin for TotemsPlugin {
    fn build(&self, app: &mut App) {
        use SessionEventKind as K;
        app.init_resource::<PlayerTotems>()
            .init_resource::<SchoolLockout>()
            .net_handler(K::SpellGo, on_spell_go)
            .net_handler(K::SpellCooldowns, on_cooldowns)
            .net_handler(K::Disconnected, on_session_end);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_interrupt_list_locks_its_schools() {
        // Frostbolt (frost, 4) and Frost Nova (frost) locked for 8 s.
        let school = |id| Some(if id == 116 || id == 122 { 4 } else { 2 });
        assert_eq!(
            lockout_of(&[(116, 8000), (122, 8000)], Some(133), school),
            Some((1 << 4, 8000))
        );
    }

    #[test]
    fn a_casts_own_list_or_a_long_one_is_no_lockout() {
        let school = |_| Some(4);
        assert_eq!(lockout_of(&[(116, 8000)], Some(116), school), None);
        assert_eq!(lockout_of(&[(12051, 480_000)], None, school), None);
        assert_eq!(lockout_of(&[(116, 8000), (122, 6000)], None, school), None);
        assert_eq!(lockout_of(&[], None, school), None);
    }
}
