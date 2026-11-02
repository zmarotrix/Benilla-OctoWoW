//! The auras the player's own casts put on other units, timed: each `SMSG_SPELL_GO` of ours that
//! hits with an aura-applying effect starts its `Spell.dbc` duration on every unit it hit, a recast
//! restarting it. Smart macros read the time left (`[debuff:Rake<3]`), which 1.12 shows only for
//! the player's own auras. Deviation: the reference keeps no such clock.

use std::collections::HashMap;

use benilla_protocol::{SessionEvent, SessionEventKind};
use bevy::prelude::*;

use benilla_ui::script::live::LiveAsk;
use benilla_ui::script::OwnAuraLeftQuery;

use crate::net::{NetHandlerApp, SelfGuid};
use crate::ui_action::Spells;

/// `Spell.dbc` columns read here.
const COL_DURATION_INDEX: usize = 30;
const COL_EFFECT_1: usize = 61;
/// `APPLY_AURA`, `PERSISTENT_AREA_AURA`, `APPLY_AREA_AURA_PARTY`.
const AURA_EFFECTS: [u32; 3] = [6, 27, 35];

/// When each of our auras on a unit ends, by `(unit, spell)`, in `Time::elapsed_secs_f64`.
#[derive(Resource, Default)]
pub(crate) struct OwnAuras(HashMap<(u64, u32), f64>);

fn on_spell_go(
    In(ev): In<SessionEvent>,
    me: Res<SelfGuid>,
    spells: Option<Res<Spells>>,
    time: Res<Time>,
    mut auras: ResMut<OwnAuras>,
) {
    let SessionEvent::SpellGo {
        caster,
        spell_id,
        hits,
        ..
    } = ev
    else {
        return;
    };
    if me.0 != Some(caster) || hits.is_empty() {
        return;
    }
    let Some(spells) = spells.as_deref() else {
        return;
    };
    let Some(row) = spells.catalog.row(spell_id) else {
        return;
    };
    let col = |c: usize| row.columns.get(c).copied().unwrap_or(0);
    if !(0..3).any(|k| AURA_EFFECTS.contains(&col(COL_EFFECT_1 + k))) {
        return;
    }
    let Some(duration) = spells
        .durations
        .get(col(COL_DURATION_INDEX))
        .filter(|d| !d.is_permanent() && d.base_ms > 0)
    else {
        return;
    };
    let now = time.elapsed_secs_f64();
    let ends = now + f64::from(duration.base_ms) / 1000.0;
    // Old entries go as a new one lands, so the table holds what can still be up.
    auras.0.retain(|_, &mut end| end > now);
    for unit in hits.iter().filter(|&&u| u != caster) {
        auras.0.insert((*unit, spell_id), ends);
    }
}

/// What the smart macros' aura reads look at.
#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct OwnAuraLive<'w> {
    auras: Res<'w, OwnAuras>,
    time: Res<'w, Time>,
}

impl crate::script_calls::live::LiveFamily for OwnAuraLive<'_> {
    fn answer(&mut self, ask: &mut LiveAsk<'_>) -> bool {
        ask.answer(|q: &OwnAuraLeftQuery| {
            let now = self.time.elapsed_secs_f64();
            Some(
                self.auras
                    .0
                    .get(&(q.unit, q.spell))
                    .map(|end| end - now)
                    .filter(|left| *left > 0.0),
            )
        })
    }
}

pub(crate) struct OwnAurasPlugin;

impl Plugin for OwnAurasPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<OwnAuras>()
            .net_handler(SessionEventKind::SpellGo, on_spell_go);
    }
}
