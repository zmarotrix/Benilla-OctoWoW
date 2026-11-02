//! The player's swing clocks and the last swings both ways, timed off the packets: each
//! `SMSG_ATTACKERSTATEUPDATE` of ours restarts the main or off hand's clock and is the last swing,
//! one at us is the last incoming hit, and our Auto Shot's or Shoot's `SMSG_SPELL_GO` restarts the
//! ranged clock. Smart macros read them (`[swingtimer]`, `[lastswing]`, `[incominghit]`,
//! `[slamclip]`, `[rangedclip]`). Deviation: the reference keeps no swing clock for scripts.

use benilla_protocol::{SessionEvent, SessionEventKind};
use bevy::prelude::*;

use benilla_ui::script::live::LiveAsk;
use benilla_ui::script::{CombatTimers, CombatTimersQuery, SwingKind};

use crate::net::{NetHandlerApp, SelfGuid};

/// Auto Shot and Shoot, the ranged clock's spells.
const RANGED_SPELLS: [u32; 2] = [75, 5019];
/// `SMSG_ATTACKERSTATEUPDATE`'s hit bits (vmangos `HitInfo`).
const HIT_OFFHAND: u32 = 0x4;
const HIT_MISS: u32 = 0x10;
const HIT_CRIT: u32 = 0x80;
const HIT_GLANCING: u32 = 0x4000;
const HIT_CRUSHING: u32 = 0x8000;

/// The clocks, in `Time::elapsed_secs_f64`.
#[derive(Resource, Default)]
pub(crate) struct SwingClocks {
    main: Option<f64>,
    off: Option<f64>,
    ranged: Option<f64>,
    last_out: Option<(SwingKind, f64)>,
    last_in: Option<(SwingKind, f64)>,
}

/// What a swing was, from its hit bits and the victim's state.
fn kind(hit_info: u32, victim_state: u32) -> SwingKind {
    match victim_state {
        2 => SwingKind::Dodge,
        3 => SwingKind::Parry,
        5 => SwingKind::Block,
        _ if hit_info & HIT_MISS != 0 => SwingKind::Miss,
        _ if hit_info & HIT_CRUSHING != 0 => SwingKind::Crushing,
        _ if hit_info & HIT_GLANCING != 0 => SwingKind::Glancing,
        _ if hit_info & HIT_CRIT != 0 => SwingKind::Crit,
        _ => SwingKind::Hit,
    }
}

fn on_attacker_state(
    In(ev): In<SessionEvent>,
    me: Res<SelfGuid>,
    time: Res<Time>,
    mut clocks: ResMut<SwingClocks>,
) {
    let SessionEvent::AttackerState(a) = ev else {
        return;
    };
    let Some(me) = me.0 else {
        return;
    };
    let now = time.elapsed_secs_f64();
    let k = kind(a.hit_info, a.victim_state);
    if a.attacker == me {
        if a.hit_info & HIT_OFFHAND != 0 {
            clocks.off = Some(now);
        } else {
            clocks.main = Some(now);
        }
        clocks.last_out = Some((k, now));
    } else if a.victim == me {
        clocks.last_in = Some((k, now));
    }
}

fn on_spell_go(
    In(ev): In<SessionEvent>,
    me: Res<SelfGuid>,
    time: Res<Time>,
    mut clocks: ResMut<SwingClocks>,
) {
    if let SessionEvent::SpellGo {
        caster, spell_id, ..
    } = ev
    {
        if me.0 == Some(caster) && RANGED_SPELLS.contains(&spell_id) {
            clocks.ranged = Some(time.elapsed_secs_f64());
        }
    }
}

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct CombatTimersLive<'w> {
    clocks: Res<'w, SwingClocks>,
    time: Res<'w, Time>,
}

impl crate::script_calls::live::LiveFamily for CombatTimersLive<'_> {
    fn answer(&mut self, ask: &mut LiveAsk<'_>) -> bool {
        ask.answer(|_: &CombatTimersQuery| {
            let now = self.time.elapsed_secs_f64();
            let since = |t: Option<f64>| t.map(|t| now - t);
            let c = &self.clocks;
            Some(CombatTimers {
                since_main: since(c.main),
                since_off: since(c.off),
                since_ranged: since(c.ranged),
                last_out: c.last_out.map(|(k, t)| (k, now - t)),
                last_in: c.last_in.map(|(k, t)| (k, now - t)),
            })
        })
    }
}

pub(crate) struct CombatTimersPlugin;

impl Plugin for CombatTimersPlugin {
    fn build(&self, app: &mut App) {
        use SessionEventKind as K;
        app.init_resource::<SwingClocks>()
            .net_handler(K::AttackerState, on_attacker_state)
            .net_handler(K::SpellGo, on_spell_go);
    }
}
