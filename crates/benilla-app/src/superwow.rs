//! SuperWoW's events, which OctoWoW's client mods add to 1.12 for castbar and swing-timer addons:
//! `UNIT_CASTEVENT` on every unit's cast start, launch, interruption and channel, and on every
//! melee swing. Each handler stamps its event on the lane ([`crate::lanes`]), so it fires at its
//! packet's place among the stock events. Deviation: 1.12.1 signals none of this.

use std::collections::VecDeque;

use benilla_protocol::{SessionEvent, SessionEventKind};
use benilla_ui::script::ScriptValue;
use bevy::prelude::*;

use crate::lanes::{LaneApp, Stamp, Until};
use crate::nampower_events::{Ctx, NamedEvent};
use crate::net::{FieldChanged, NetHandlerApp, SelfGuid};
use crate::script_calls::VmWith;

/// `SMSG_ATTACKERSTATEUPDATE`'s offhand bit (vmangos `HITINFO_LEFTSWING`).
const HIT_INFO_OFFHAND: u32 = 0x4;

/// The kinds `UNIT_CASTEVENT`'s third argument names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CastEventKind {
    Start,
    Cast,
    Fail,
    Channel,
    MainHand,
    OffHand,
}

impl CastEventKind {
    fn name(self) -> &'static str {
        match self {
            Self::Start => "START",
            Self::Cast => "CAST",
            Self::Fail => "FAIL",
            Self::Channel => "CHANNEL",
            Self::MainHand => "MAINHAND",
            Self::OffHand => "OFFHAND",
        }
    }
}

/// One `UNIT_CASTEVENT`: caster, target (0 for none), kind, spell (0 for a swing) and duration in
/// milliseconds (0 where there is none).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CastEvent {
    caster: u64,
    target: u64,
    kind: CastEventKind,
    spell_id: u32,
    duration_ms: u32,
}

impl CastEvent {
    fn args(self) -> Vec<ScriptValue> {
        vec![
            ScriptValue::Str(benilla_ui::script::guid_text(self.caster)),
            ScriptValue::Str(benilla_ui::script::guid_text(self.target)),
            ScriptValue::Str(self.kind.name().to_string()),
            ScriptValue::Int(i64::from(self.spell_id)),
            ScriptValue::Int(i64::from(self.duration_ms)),
        ]
    }
}

/// What the handlers raised, each stamped as its handler returned: `UNIT_CASTEVENT`s and
/// nampower's events ([`crate::nampower_events`]), by name with their arguments.
#[derive(Resource, Default)]
pub(crate) struct SuperWowLane(VecDeque<(Stamp, NamedEvent)>);

impl SuperWowLane {
    fn push(&mut self, event: NamedEvent) {
        self.push_at(crate::lanes::stamp(), event);
    }

    /// Queue `event` at `at`, behind everything stamped before it: a field edge is read after the
    /// packets stamped later than it were queued.
    fn push_at(&mut self, at: Stamp, event: NamedEvent) {
        let place = self
            .0
            .iter()
            .rposition(|(s, _)| *s <= at)
            .map_or(0, |p| p + 1);
        self.0.insert(place, (at, event));
    }

    /// Raise `event` from outside a handler, stamped now.
    pub(crate) fn raise(&mut self, event: NamedEvent) {
        self.push(event);
    }

    fn peek(&self) -> Option<Stamp> {
        self.0.front().map(|(s, _)| *s)
    }
}

/// SuperWoW's `TrackUnit` list as the VM last held it, for the minimap.
#[derive(Resource, Default)]
pub(crate) struct SuperWowTracked(pub(crate) Vec<u64>);

/// Copy the VM's tracked list when it moved.
fn sync_tracked(
    script: Option<NonSend<benilla_ui::script::UiScript>>,
    mut tracked: ResMut<SuperWowTracked>,
) {
    let now = script.map(|s| s.tracked_units()).unwrap_or_default();
    if tracked.0 != now {
        tracked.0 = now;
    }
}

pub(crate) struct SuperWowPlugin;

impl Plugin for SuperWowPlugin {
    fn build(&self, app: &mut App) {
        use SessionEventKind as K;
        app.init_resource::<SuperWowLane>()
            .init_resource::<SuperWowTracked>()
            .add_systems(Update, sync_tracked.in_set(crate::ui_script::UiFeed))
            .add_lane(
                "superwow",
                |world| world.get_resource::<SuperWowLane>()?.peek(),
                fire_lane,
                None,
            )
            .add_systems(
                Update,
                drop_lane
                    .in_set(crate::ui_script::UiFeed)
                    .run_if(not(crate::ui_script::ingame_ui_up)),
            )
            .net_handler(K::SpellStart, on_spell_start)
            .net_handler(K::SpellGo, on_spell_go)
            .net_handler(K::SpellFailedOther, on_spell_failed_other)
            .net_handler(K::ChannelStart, on_channel_start)
            .net_handler(K::AttackerState, on_attacker_state)
            .net_handler(K::CastResult, on_cast_result)
            .net_handler(K::SpellDelayed, on_spell_delayed)
            .net_handler(K::SpellDamageLog, on_spell_damage_log)
            .net_handler(K::PeriodicAuraLog, on_periodic_aura_log)
            .net_handler(K::SpellHealLog, on_spell_heal_log)
            .net_handler(K::SpellLogMiss, on_spell_log_miss);
    }
}

/// Nothing is up to hear the events: they go.
fn drop_lane(mut lane: ResMut<SuperWowLane>) {
    lane.0.clear();
}

/// The cast event a packet makes, if any; `me` is the caster of our own channel.
fn cast_event(ev: &SessionEvent, me: Option<u64>) -> Option<CastEvent> {
    Some(match ev {
        SessionEvent::SpellStart {
            caster,
            spell_id,
            cast_time_ms,
            target,
            ..
        } => CastEvent {
            caster: *caster,
            target: target.unwrap_or(0),
            kind: CastEventKind::Start,
            spell_id: *spell_id,
            duration_ms: *cast_time_ms,
        },
        SessionEvent::SpellGo {
            caster,
            spell_id,
            target,
            hits,
            ..
        } => CastEvent {
            caster: *caster,
            target: target.or_else(|| hits.first().copied()).unwrap_or(0),
            kind: CastEventKind::Cast,
            spell_id: *spell_id,
            duration_ms: 0,
        },
        SessionEvent::SpellFailedOther { caster, spell_id } => CastEvent {
            caster: *caster,
            target: 0,
            kind: CastEventKind::Fail,
            spell_id: *spell_id,
            duration_ms: 0,
        },
        // `MSG_CHANNEL_START` is our own channel alone.
        SessionEvent::ChannelStart {
            spell_id,
            duration_ms,
        } => CastEvent {
            caster: me?,
            target: 0,
            kind: CastEventKind::Channel,
            spell_id: *spell_id,
            duration_ms: *duration_ms,
        },
        SessionEvent::AttackerState(s) => CastEvent {
            caster: s.attacker,
            target: s.victim,
            kind: if s.hit_info & HIT_INFO_OFFHAND != 0 {
                CastEventKind::OffHand
            } else {
                CastEventKind::MainHand
            },
            spell_id: 0,
            duration_ms: 0,
        },
        _ => return None,
    })
}

/// What the handlers read besides their packet.
#[derive(bevy::ecs::system::SystemParam)]
struct Raise<'w> {
    me: Res<'w, SelfGuid>,
    cvars: Option<Res<'w, crate::cvars::Cvars>>,
    spells: Option<Res<'w, crate::ui_action::Spells>>,
    lane: ResMut<'w, SuperWowLane>,
}

impl Raise<'_> {
    /// Stamp the cast event `ev` makes, then nampower's.
    fn stamp(&mut self, ev: &SessionEvent) {
        if let Some(event) = cast_event(ev, self.me.0) {
            self.lane.push(("UNIT_CASTEVENT", event.args()));
        }
        let ctx = Ctx {
            me: self.me.0,
            cvars: self.cvars.as_deref(),
            spells: self.spells.as_deref(),
        };
        for event in ctx.packet_events(ev) {
            self.lane.push(event);
        }
    }
}

fn on_spell_start(In(ev): In<SessionEvent>, mut raise: Raise) {
    if matches!(ev, SessionEvent::SpellStart { .. }) {
        raise.stamp(&ev);
    }
}

fn on_spell_go(In(ev): In<SessionEvent>, mut raise: Raise) {
    if matches!(ev, SessionEvent::SpellGo { .. }) {
        raise.stamp(&ev);
    }
}

fn on_spell_failed_other(In(ev): In<SessionEvent>, mut raise: Raise) {
    if matches!(ev, SessionEvent::SpellFailedOther { .. }) {
        raise.stamp(&ev);
    }
}

fn on_channel_start(In(ev): In<SessionEvent>, mut raise: Raise) {
    if matches!(ev, SessionEvent::ChannelStart { .. }) {
        raise.stamp(&ev);
    }
}

fn on_attacker_state(In(ev): In<SessionEvent>, mut raise: Raise) {
    if matches!(ev, SessionEvent::AttackerState(_)) {
        raise.stamp(&ev);
    }
}

fn on_cast_result(In(ev): In<SessionEvent>, mut raise: Raise) {
    if matches!(ev, SessionEvent::CastResult { .. }) {
        raise.stamp(&ev);
    }
}

fn on_spell_delayed(In(ev): In<SessionEvent>, mut raise: Raise) {
    if matches!(ev, SessionEvent::SpellDelayed { .. }) {
        raise.stamp(&ev);
    }
}

fn on_spell_damage_log(In(ev): In<SessionEvent>, mut raise: Raise) {
    if matches!(ev, SessionEvent::SpellDamageLog(_)) {
        raise.stamp(&ev);
    }
}

fn on_periodic_aura_log(In(ev): In<SessionEvent>, mut raise: Raise) {
    if matches!(ev, SessionEvent::PeriodicAuraLog(_)) {
        raise.stamp(&ev);
    }
}

fn on_spell_heal_log(In(ev): In<SessionEvent>, mut raise: Raise) {
    if matches!(ev, SessionEvent::SpellHealLog(_)) {
        raise.stamp(&ev);
    }
}

fn on_spell_log_miss(In(ev): In<SessionEvent>, mut raise: Raise) {
    if matches!(ev, SessionEvent::SpellLogMiss(_)) {
        raise.stamp(&ev);
    }
}

/// nampower's field events off this frame's unit field edges, each at its edge's stamp.
/// The update packet's edges as nampower's events, each pushed at its edge's stamp: a leg of the
/// packet's end walk ([`crate::field_callbacks::run`]), inside the dispatch.
pub(crate) fn field_events(world: &mut World, edges: &[FieldChanged]) {
    let me = world.get_resource::<SelfGuid>().and_then(|g| g.0);
    let mut out = Vec::new();
    {
        let ctx = Ctx {
            me,
            cvars: world.get_resource::<crate::cvars::Cvars>(),
            spells: world.get_resource::<crate::ui_action::Spells>(),
        };
        for edge in edges.iter().filter(|e| e.is_unit()) {
            let store = world.get::<crate::net::ObjectStore>(edge.entity);
            let field = |i: u16| store.and_then(|s| s.0.field(i)).unwrap_or(0);
            for event in ctx.field_events(edge.guid, edge.index, edge.old, edge.new, &field) {
                out.push((edge.at, event));
            }
        }
    }
    if let Some(mut lane) = world.get_resource_mut::<SuperWowLane>() {
        for (at, event) in out {
            lane.push_at(at, event);
        }
    }
}

/// The lane's firing: each item `until` admits, until one whose firing stamped anything.
fn fire_lane(In(until): In<Until>, mut vm: VmWith<ResMut<'static, SuperWowLane>>) {
    if !vm.run(|script| script.is_some()) {
        return;
    }
    while let Some(at) = vm.own().peek().filter(|&s| until.admits(s)) {
        let Some((_, event)) = vm.own().0.pop_front() else {
            break;
        };
        let (name, args) = event;
        let ((), moved) = crate::lanes::firing(at, || {
            vm.run(|script| {
                if let Some(script) = script {
                    script.fire_event(name, args);
                }
            })
        });
        if moved {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_packet_names_its_kind_and_units() {
        let start = SessionEvent::SpellStart {
            caster: 0xF1,
            spell_id: 133,
            cast_flags: 0,
            cast_time_ms: 3500,
            target: Some(0xA),
            ammo_display_id: None,
        };
        assert_eq!(
            cast_event(&start, None),
            Some(CastEvent {
                caster: 0xF1,
                target: 0xA,
                kind: CastEventKind::Start,
                spell_id: 133,
                duration_ms: 3500,
            })
        );
        let channel = SessionEvent::ChannelStart {
            spell_id: 5143,
            duration_ms: 5000,
        };
        assert_eq!(cast_event(&channel, None), None, "no player, no caster");
        assert_eq!(
            cast_event(&channel, Some(0x10)).map(|e| (e.caster, e.kind)),
            Some((0x10, CastEventKind::Channel))
        );
        let swing = |hit_info| {
            SessionEvent::AttackerState(benilla_protocol::messages::AttackerState {
                attacker: 0x10,
                victim: 0xF1,
                hit_info,
                damage: 0,
                victim_state: 1,
                absorb: 0,
                resist: 0,
                blocked: 0,
                melee_spell_id: 0,
                school: 0,
            })
        };
        assert_eq!(
            cast_event(&swing(0x2), None).map(|e| e.kind),
            Some(CastEventKind::MainHand)
        );
        assert_eq!(
            cast_event(&swing(0x6), None).map(|e| e.kind),
            Some(CastEventKind::OffHand)
        );
        let args = cast_event(&start, None).unwrap().args();
        assert_eq!(args[0], ScriptValue::Str("0x00000000000000F1".into()));
        assert_eq!(args[2], ScriptValue::Str("START".into()));
    }
}
