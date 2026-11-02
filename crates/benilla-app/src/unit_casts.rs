//! Two reads of other units the smart macros add to 1.12: the cast or channel each unit is making
//! (`[casting]`, `[casttime]`, `[channeltime]` on a target), timed off its `SMSG_SPELL_START` and
//! its channel field, and how fast each unit's health falls, the time-to-kill and time-to-execute
//! estimates (`[ttk]`, `[tte]`), and whether our last spell on a unit was resisted (`[resisted]`)
//! off our own spell miss and damage logs. Deviation: the reference keeps none of these.

use std::collections::{HashMap, VecDeque};

use benilla_protocol::{SessionEvent, SessionEventKind};
use bevy::prelude::*;

use benilla_ui::script::live::LiveAsk;
use benilla_ui::script::{ResistQuery, UnitCastQuery, UnitTrendQuery};

use crate::net::{FieldChanged, NetHandlerApp, SelfGuid};
use crate::ui_action::Spells;

const FIELD_HEALTH: u16 = 22;
const FIELD_MAX_HEALTH: u16 = 28;
const FIELD_CHANNEL_SPELL: u16 = 144;
const COL_DURATION_INDEX: usize = 30;
/// The window a health trend is fitted over, and how many samples it keeps.
const TREND_WINDOW: f64 = 10.0;
const TREND_SAMPLES: usize = 40;
/// Units whose health stopped moving this long ago are dropped.
const TREND_STALE: f64 = 30.0;
/// The execute threshold `[tte]` reads to.
const EXECUTE_PCT: f64 = 20.0;
/// `SpellMissInfo`'s resist.
const MISS_RESIST: u8 = 2;

#[derive(Clone, Copy, Debug, PartialEq)]
struct Cast {
    spell: u32,
    end: f64,
    channel: bool,
}

/// Each unit's cast in progress, in `Time::elapsed_secs_f64`.
#[derive(Resource, Default)]
pub(crate) struct UnitCasts(HashMap<u64, Cast>);

/// Our last spell result on each unit: `Some(true)` partly resisted, `Some(false)` fully.
#[derive(Resource, Default)]
pub(crate) struct LastResists(HashMap<u64, bool>);

/// Each unit's recent health, `(time, percent)`, oldest first.
#[derive(Resource, Default)]
pub(crate) struct HealthTrends(HashMap<u64, VecDeque<(f64, f64)>>);

fn on_cast_packet(In(ev): In<SessionEvent>, time: Res<Time>, mut casts: ResMut<UnitCasts>) {
    let now = time.elapsed_secs_f64();
    match ev {
        SessionEvent::SpellStart {
            caster,
            spell_id,
            cast_time_ms,
            ..
        } if cast_time_ms > 0 => {
            casts.0.insert(
                caster,
                Cast {
                    spell: spell_id,
                    end: now + f64::from(cast_time_ms) / 1000.0,
                    channel: false,
                },
            );
        }
        SessionEvent::SpellGo {
            caster, spell_id, ..
        }
        | SessionEvent::SpellFailedOther { caster, spell_id } => {
            if casts
                .0
                .get(&caster)
                .is_some_and(|c| !c.channel && c.spell == spell_id)
            {
                casts.0.remove(&caster);
            }
        }
        SessionEvent::SpellDelayed { caster, delay_ms } => {
            if let Some(c) = casts.0.get_mut(&caster) {
                c.end += f64::from(delay_ms) / 1000.0;
            }
        }
        _ => {}
    }
    casts.0.retain(|_, c| c.end > now);
}

/// Our spell's outcome on its targets: a resist recorded, a clean hit clearing it.
fn on_spell_result(In(ev): In<SessionEvent>, me: Res<SelfGuid>, mut resists: ResMut<LastResists>) {
    let Some(me) = me.0 else {
        return;
    };
    match ev {
        SessionEvent::SpellDamageLog(d) if d.attacker == me => {
            if d.resist > 0 {
                resists.0.insert(d.target, true);
            } else {
                resists.0.remove(&d.target);
            }
        }
        SessionEvent::SpellLogMiss(m) if m.caster == me => {
            for &(target, info) in &m.misses {
                if info == MISS_RESIST {
                    resists.0.insert(target, false);
                }
            }
        }
        _ => {}
    }
    if resists.0.len() > 256 {
        resists.0.clear();
    }
}

fn on_session_end(
    In(_): In<SessionEvent>,
    mut casts: ResMut<UnitCasts>,
    mut trends: ResMut<HealthTrends>,
    mut resists: ResMut<LastResists>,
) {
    casts.0.clear();
    trends.0.clear();
    resists.0.clear();
}

/// A unit's channel field and health edges: a channel timed off its spell's duration, a health
/// sample for the trend.
fn read_fields(
    mut edges: MessageReader<FieldChanged>,
    stores: Query<&crate::net::ObjectStore>,
    spells: Option<Res<Spells>>,
    time: Res<Time>,
    mut casts: ResMut<UnitCasts>,
    mut trends: ResMut<HealthTrends>,
) {
    let now = time.elapsed_secs_f64();
    for edge in edges.read().filter(|e| e.is_unit()) {
        match edge.index {
            FIELD_CHANNEL_SPELL if edge.new == 0 => {
                if casts.0.get(&edge.guid).is_some_and(|c| c.channel) {
                    casts.0.remove(&edge.guid);
                }
            }
            FIELD_CHANNEL_SPELL => {
                let secs = spells
                    .as_deref()
                    .and_then(|s| {
                        let row = s.catalog.row(edge.new)?;
                        let index = row.columns.get(COL_DURATION_INDEX).copied()?;
                        s.durations.get(index).filter(|d| d.base_ms > 0)
                    })
                    .map_or(0.0, |d| f64::from(d.base_ms) / 1000.0);
                casts.0.insert(
                    edge.guid,
                    Cast {
                        spell: edge.new,
                        end: now + secs,
                        channel: true,
                    },
                );
            }
            FIELD_HEALTH | FIELD_MAX_HEALTH => {
                let Ok(store) = stores.get(edge.entity) else {
                    continue;
                };
                let health = store.0.field(FIELD_HEALTH).unwrap_or(0);
                let max = store.0.field(FIELD_MAX_HEALTH).unwrap_or(0);
                if max == 0 {
                    continue;
                }
                let pct = f64::from(health) * 100.0 / f64::from(max);
                let samples = trends.0.entry(edge.guid).or_default();
                samples.push_back((now, pct));
                while samples.len() > TREND_SAMPLES
                    || samples.front().is_some_and(|(t, _)| now - t > TREND_WINDOW)
                {
                    samples.pop_front();
                }
            }
            _ => {}
        }
    }
    trends
        .0
        .retain(|_, s| s.back().is_some_and(|(t, _)| now - t < TREND_STALE));
}

/// Seconds until the trend reaches 0% and [`EXECUTE_PCT`]: a least-squares line through the
/// samples. `None` without two samples a second apart or while health is not falling.
fn estimate(samples: &VecDeque<(f64, f64)>, now: f64) -> Option<(f64, f64)> {
    let (first, last) = (samples.front()?, samples.back()?);
    if samples.len() < 2 || last.0 - first.0 < 1.0 {
        return None;
    }
    let n = samples.len() as f64;
    let (mt, mp) = samples
        .iter()
        .fold((0.0, 0.0), |(a, b), (t, p)| (a + t / n, b + p / n));
    let (num, den) = samples.iter().fold((0.0, 0.0), |(num, den), (t, p)| {
        (num + (t - mt) * (p - mp), den + (t - mt) * (t - mt))
    });
    let slope = num / den;
    if !slope.is_finite() || slope >= -1e-6 {
        return None;
    }
    // Where the line stands now, then how long it takes to fall.
    let pct_now = (mp + slope * (now - mt)).clamp(0.0, 100.0);
    let ttk = pct_now / -slope;
    let tte = ((pct_now - EXECUTE_PCT) / -slope).max(0.0);
    Some((ttk, tte))
}

/// What the cast and trend reads look at.
#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct UnitCastLive<'w> {
    casts: Res<'w, UnitCasts>,
    trends: Res<'w, HealthTrends>,
    resists: Res<'w, LastResists>,
    time: Res<'w, Time>,
}

impl crate::script_calls::live::LiveFamily for UnitCastLive<'_> {
    fn answer(&mut self, ask: &mut LiveAsk<'_>) -> bool {
        let now = self.time.elapsed_secs_f64();
        ask.answer(|q: &UnitCastQuery| {
            Some(
                self.casts
                    .0
                    .get(&q.0)
                    .filter(|c| c.end > now)
                    .map(|c| (c.spell, c.end - now, c.channel)),
            )
        }) || ask.answer(|q: &UnitTrendQuery| {
            Some(self.trends.0.get(&q.0).and_then(|s| estimate(s, now)))
        }) || ask.answer(|q: &ResistQuery| Some(self.resists.0.get(&q.0).copied()))
    }
}

pub(crate) struct UnitCastsPlugin;

impl Plugin for UnitCastsPlugin {
    fn build(&self, app: &mut App) {
        use SessionEventKind as K;
        app.init_resource::<UnitCasts>()
            .init_resource::<HealthTrends>()
            .init_resource::<LastResists>()
            .net_handler(K::SpellDamageLog, on_spell_result)
            .net_handler(K::SpellLogMiss, on_spell_result)
            .net_handler(K::SpellStart, on_cast_packet)
            .net_handler(K::SpellGo, on_cast_packet)
            .net_handler(K::SpellFailedOther, on_cast_packet)
            .net_handler(K::SpellDelayed, on_cast_packet)
            .net_handler(K::Disconnected, on_session_end)
            // Outside the dispatch: the timers only answer calls made after the frame's edges.
            .add_systems(Last, read_fields);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_falling_health_line_estimates_kill_and_execute() {
        // 100% to 80% over 4 s: 5% a second.
        let samples: VecDeque<_> = [(0.0, 100.0), (2.0, 90.0), (4.0, 80.0)].into();
        let (ttk, tte) = estimate(&samples, 4.0).unwrap();
        assert!((ttk - 16.0).abs() < 1e-6, "{ttk}");
        assert!((tte - 12.0).abs() < 1e-6, "{tte}");
    }

    #[test]
    fn a_steady_or_short_trend_estimates_nothing() {
        let steady: VecDeque<_> = [(0.0, 50.0), (3.0, 50.0)].into();
        assert_eq!(estimate(&steady, 3.0), None);
        let short: VecDeque<_> = [(0.0, 50.0), (0.5, 40.0)].into();
        assert_eq!(estimate(&short, 0.5), None);
    }
}
