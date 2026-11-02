//! nampower's events, which OctoWoW's client mods add for pfUI, Cursive and HealersMate: the
//! spell packets' (`SPELL_START_*`, `SPELL_GO_*`, `AURA_CAST_ON_*`, …), the combat logs'
//! (`SPELL_DAMAGE_EVENT_*`, `SPELL_HEAL_*`, `SPELL_MISS_*`, `AUTO_ATTACK_*`) and the unit fields'
//! (`BUFF_*`/`DEBUFF_*`, `UNIT_DIED`, the `*_GUID` events), with nampower's arguments, each behind
//! its `NP_Enable*` CVar where nampower gates it. They ride SuperWoW's lane
//! ([`crate::superwow`]), so each fires at its packet's place. Deviation: 1.12.1 signals none.

use benilla_protocol::field::{FIELD_UNIT_DISPLAYID, FIELD_UNIT_HEALTH};
use benilla_protocol::messages::PeriodicTick;
use benilla_protocol::SessionEvent;
use benilla_ui::script::{guid_text, ScriptValue};

use crate::cvars::Cvars;
use crate::ui_action::Spells;

/// One named event and its arguments.
pub(crate) type NamedEvent = (&'static str, Vec<ScriptValue>);

/// `SPELL_QUEUE_EVENT(code, spellId)`, the client's spell queue's hold or send
/// ([`crate::spell::queue`]).
pub(crate) fn queue_event(code: u8, spell_id: u32) -> NamedEvent {
    (
        "SPELL_QUEUE_EVENT",
        vec![
            ScriptValue::Int(i64::from(code)),
            ScriptValue::Int(i64::from(spell_id)),
        ],
    )
}

/// `UNIT_FIELD_FLAGS`, `UNIT_FIELD_AURA` (48 spell ids), `UNIT_FIELD_AURALEVELS` and
/// `UNIT_FIELD_AURAAPPLICATIONS` (a byte per slot, four to the dword).
const FIELD_UNIT_FLAGS: u16 = 46;
const FIELD_AURA: u16 = 47;
const AURA_SLOTS: u16 = 48;
const FIELD_AURA_LEVELS: u16 = 101;
const FIELD_AURA_APPLICATIONS: u16 = 113;
/// Slots 0-31 hold buffs, 32-47 debuffs.
const FIRST_DEBUFF_SLOT: u16 = 32;

/// `Spell.dbc` columns the events read.
const COL_ATTRIBUTES_EX: usize = 7;
const COL_DURATION_INDEX: usize = 30;
const COL_EFFECT_1: usize = 61;
const COL_EFFECT_APPLY_AURA_1: usize = 91;
const COL_EFFECT_AMPLITUDE_1: usize = 94;
const COL_EFFECT_MISC_1: usize = 106;
const COL_START_RECOVERY_TIME: usize = 158;
/// `SPELL_ATTR_EX_CHANNELED_1` and `_2`.
const CHANNELED: u32 = 0x4 | 0x40;
/// The effects that put an aura on their targets: `APPLY_AURA`, `PERSISTENT_AREA_AURA`,
/// `APPLY_AREA_AURA_PARTY`.
const AURA_EFFECTS: [u32; 3] = [6, 27, 35];

/// What the events read besides their packet.
pub(crate) struct Ctx<'a> {
    pub(crate) me: Option<u64>,
    pub(crate) cvars: Option<&'a Cvars>,
    pub(crate) spells: Option<&'a Spells>,
}

fn s(guid: u64) -> ScriptValue {
    ScriptValue::Str(guid_text(guid))
}

fn i(n: impl Into<i64>) -> ScriptValue {
    ScriptValue::Int(n.into())
}

fn flag(b: bool) -> ScriptValue {
    i(i64::from(b))
}

impl Ctx<'_> {
    /// A gating CVar is set to anything but 0.
    fn on(&self, cvar: &str) -> bool {
        self.cvars
            .and_then(|c| c.get(cvar))
            .is_some_and(|v| !matches!(v.trim(), "" | "0"))
    }

    fn mine(&self, guid: u64) -> bool {
        self.me == Some(guid)
    }

    fn column(&self, spell: u32, col: usize) -> u32 {
        self.spells
            .and_then(|s| s.catalog.row(spell))
            .and_then(|r| r.columns.get(col).copied())
            .unwrap_or(0)
    }

    fn channeled(&self, spell: u32) -> bool {
        self.column(spell, COL_ATTRIBUTES_EX) & CHANNELED != 0
    }

    /// The spell's base duration in milliseconds, 0 for none or a permanent one.
    fn duration_ms(&self, spell: u32) -> i64 {
        let index = self.column(spell, COL_DURATION_INDEX);
        self.spells
            .and_then(|s| s.durations.get(index))
            .filter(|d| !d.is_permanent())
            .map_or(0, |d| i64::from(d.base_ms.max(0)))
    }

    /// `SPELL_CAST_EVENT`'s cast type: 3 a channel, 1 off the global cooldown, else 0.
    fn cast_type(&self, spell: u32) -> i64 {
        if self.channeled(spell) {
            3
        } else if self.column(spell, COL_START_RECOVERY_TIME) == 0 {
            1
        } else {
            0
        }
    }

    /// The events a packet makes, in nampower's order.
    pub(crate) fn packet_events(&self, ev: &SessionEvent) -> Vec<NamedEvent> {
        let mut out: Vec<NamedEvent> = Vec::new();
        match ev {
            SessionEvent::SpellStart {
                caster,
                spell_id,
                cast_flags,
                cast_time_ms,
                target,
                ..
            } => {
                let target = target.unwrap_or(0);
                // Every cast of ours the server took opens with its start, instants included:
                // nampower signals the attempt as it sends, which the start stands in for.
                if self.mine(*caster) {
                    out.push((
                        "SPELL_CAST_EVENT",
                        vec![i(1), i(*spell_id), i(self.cast_type(*spell_id)), s(0), i(0)],
                    ));
                }
                if self.on("NP_EnableSpellStartEvents") {
                    let channel = *cast_time_ms == 0 && self.channeled(*spell_id);
                    let duration = if channel {
                        self.duration_ms(*spell_id)
                    } else {
                        0
                    };
                    let name = if self.mine(*caster) {
                        "SPELL_START_SELF"
                    } else {
                        "SPELL_START_OTHER"
                    };
                    out.push((
                        name,
                        vec![
                            i(0),
                            i(*spell_id),
                            s(*caster),
                            s(target),
                            i(*cast_flags),
                            i(*cast_time_ms),
                            i(duration),
                            i(i64::from(channel)),
                        ],
                    ));
                }
            }
            SessionEvent::SpellGo {
                caster,
                spell_id,
                cast_flags,
                hits,
                misses,
                target,
                ..
            } => {
                let mine = self.mine(*caster);
                if self.on("NP_EnableSpellGoEvents") {
                    let name = if mine {
                        "SPELL_GO_SELF"
                    } else {
                        "SPELL_GO_OTHER"
                    };
                    let target = target.or_else(|| hits.first().copied()).unwrap_or(0);
                    out.push((
                        name,
                        vec![
                            i(0),
                            i(*spell_id),
                            s(*caster),
                            s(target),
                            i(*cast_flags),
                            i(hits.len() as i64),
                            i(misses.len() as i64),
                        ],
                    ));
                }
                let miss = if mine {
                    "SPELL_MISS_SELF"
                } else {
                    "SPELL_MISS_OTHER"
                };
                for m in misses {
                    out.push((miss, vec![s(*caster), s(m.target), i(*spell_id), i(m.info)]));
                }
                if self.on("NP_EnableAuraCastEvents") {
                    self.aura_casts(*caster, *spell_id, hits, &mut out);
                }
            }
            SessionEvent::SpellFailedOther { caster, spell_id } => {
                if !self.mine(*caster) {
                    out.push(("SPELL_FAILED_OTHER", vec![s(*caster), i(*spell_id)]));
                }
            }
            SessionEvent::CastResult {
                spell_id,
                success: false,
                reason,
                ..
            } => {
                out.push((
                    "SPELL_FAILED_SELF",
                    vec![i(*spell_id), i(reason.unwrap_or(0)), i(1)],
                ));
            }
            SessionEvent::SpellDelayed { caster, delay_ms } => {
                let name = if self.mine(*caster) {
                    "SPELL_DELAYED_SELF"
                } else {
                    "SPELL_DELAYED_OTHER"
                };
                out.push((name, vec![s(*caster), i(*delay_ms)]));
            }
            SessionEvent::ChannelStart {
                spell_id,
                duration_ms,
            } => {
                out.push((
                    "SPELL_CHANNEL_START",
                    vec![i(*spell_id), s(0), i(*duration_ms)],
                ));
            }
            SessionEvent::AttackerState(a) => {
                if self.on("NP_EnableAutoAttackEvents") {
                    let name = if self.mine(a.attacker) {
                        "AUTO_ATTACK_SELF"
                    } else {
                        "AUTO_ATTACK_OTHER"
                    };
                    out.push((
                        name,
                        vec![
                            s(a.attacker),
                            s(a.victim),
                            i(a.damage),
                            i(a.hit_info),
                            i(a.victim_state),
                            i(1),
                            i(a.blocked),
                            i(a.absorb),
                            i(a.resist),
                        ],
                    ));
                }
                out.push(self.combat_guid(a.victim, a.damage, a.school, a.hit_info));
            }
            SessionEvent::SpellDamageLog(d) => {
                out.push(self.spell_damage(
                    d.target,
                    d.attacker,
                    d.spell_id,
                    d.damage,
                    (d.absorb, d.blocked, d.resist),
                    d.hit_info,
                    u32::from(d.school),
                ));
            }
            SessionEvent::PeriodicAuraLog(p) => {
                for tick in &p.ticks {
                    match *tick {
                        PeriodicTick::Damage {
                            amount,
                            school,
                            absorb,
                            resist,
                        } => out.push(self.spell_damage(
                            p.target,
                            p.caster,
                            p.spell_id,
                            amount,
                            (absorb, 0, resist),
                            0,
                            school,
                        )),
                        PeriodicTick::Heal { amount } => self.heal(
                            p.target, p.caster, p.spell_id, amount, false, true, &mut out,
                        ),
                        _ => {}
                    }
                }
            }
            SessionEvent::SpellHealLog(h) => self.heal(
                h.target, h.healer, h.spell_id, h.amount, h.critical, false, &mut out,
            ),
            SessionEvent::SpellLogMiss(m) => {
                let name = if self.mine(m.caster) {
                    "SPELL_MISS_SELF"
                } else {
                    "SPELL_MISS_OTHER"
                };
                for &(target, info) in &m.misses {
                    out.push((name, vec![s(m.caster), s(target), i(m.spell_id), i(info)]));
                }
            }
            _ => {}
        }
        out
    }

    /// `AURA_CAST_ON_*`: one per hit target and aura effect of the spell.
    fn aura_casts(&self, caster: u64, spell: u32, hits: &[u64], out: &mut Vec<NamedEvent>) {
        let duration = self.duration_ms(spell);
        for k in 0..3 {
            let effect = self.column(spell, COL_EFFECT_1 + k);
            if !AURA_EFFECTS.contains(&effect) {
                continue;
            }
            let targets: &[u64] = if hits.is_empty() { &[caster] } else { hits };
            for &target in targets {
                let name = if self.mine(target) {
                    "AURA_CAST_ON_SELF"
                } else {
                    "AURA_CAST_ON_OTHER"
                };
                out.push((
                    name,
                    vec![
                        i(spell),
                        s(caster),
                        s(target),
                        i(effect),
                        i(self.column(spell, COL_EFFECT_APPLY_AURA_1 + k)),
                        i(self.column(spell, COL_EFFECT_AMPLITUDE_1 + k)),
                        i(self.column(spell, COL_EFFECT_MISC_1 + k)),
                        i(duration),
                        i(0),
                    ],
                ));
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn spell_damage(
        &self,
        target: u64,
        caster: u64,
        spell: u32,
        amount: u32,
        (absorb, block, resist): (u32, u32, i32),
        hit_info: u32,
        school: u32,
    ) -> NamedEvent {
        let name = if self.mine(caster) {
            "SPELL_DAMAGE_EVENT_SELF"
        } else {
            "SPELL_DAMAGE_EVENT_OTHER"
        };
        let effects = format!(
            "{},{},{},{}",
            self.column(spell, COL_EFFECT_1),
            self.column(spell, COL_EFFECT_1 + 1),
            self.column(spell, COL_EFFECT_1 + 2),
            self.column(spell, COL_EFFECT_APPLY_AURA_1),
        );
        (
            name,
            vec![
                s(target),
                s(caster),
                i(spell),
                i(amount),
                ScriptValue::Str(format!("{absorb},{block},{resist}")),
                i(hit_info),
                i(school),
                ScriptValue::Str(effects),
            ],
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn heal(
        &self,
        target: u64,
        caster: u64,
        spell: u32,
        amount: u32,
        critical: bool,
        periodic: bool,
        out: &mut Vec<NamedEvent>,
    ) {
        if !self.on("NP_EnableSpellHealEvents") {
            return;
        }
        let args = vec![
            s(target),
            s(caster),
            i(spell),
            i(amount),
            flag(critical),
            flag(periodic),
        ];
        let by = if self.mine(caster) {
            "SPELL_HEAL_BY_SELF"
        } else {
            "SPELL_HEAL_BY_OTHER"
        };
        out.push((by, args.clone()));
        if self.mine(target) {
            out.push(("SPELL_HEAL_ON_SELF", args));
        }
    }

    /// `UNIT_COMBAT_GUID` for a swing's victim, with the floating text's action word.
    fn combat_guid(&self, victim: u64, damage: u32, school: u32, hit_info: u32) -> NamedEvent {
        let action = if damage > 0 { "WOUND" } else { "MISS" };
        (
            "UNIT_COMBAT_GUID",
            vec![
                s(victim),
                ScriptValue::Str(action.to_string()),
                i(damage),
                i(school),
                i(hit_info & 0xffff),
                i(0),
                i(0),
                i(0),
            ],
        )
    }

    /// The events one unit field edge makes. `field` reads the unit's fields as they stand.
    pub(crate) fn field_events(
        &self,
        guid: u64,
        index: u16,
        old: u32,
        new: u32,
        field: &dyn Fn(u16) -> u32,
    ) -> Vec<NamedEvent> {
        let mut out = Vec::new();
        let guid_event = |name| (name, self.guid_args(guid));
        let gated = self.on("NP_EnableUnitEventsGuid");
        match index {
            FIELD_UNIT_HEALTH => {
                if old as i32 > 0 && new == 0 {
                    out.push(("UNIT_DIED", vec![s(guid)]));
                }
                if gated {
                    out.push(guid_event("UNIT_HEALTH_GUID"));
                }
            }
            FIELD_UNIT_FLAGS if gated => out.push(guid_event("UNIT_FLAGS_GUID")),
            FIELD_UNIT_DISPLAYID if gated => out.push(guid_event("UNIT_MODEL_CHANGED_GUID")),
            i if (FIELD_AURA..FIELD_AURA + AURA_SLOTS).contains(&i) => {
                let slot = i - FIELD_AURA;
                if old != 0 {
                    out.push(self.aura_event(guid, slot, old, false, 1, field));
                }
                if new != 0 {
                    out.push(self.aura_event(guid, slot, new, true, 0, field));
                }
                if gated {
                    out.push(guid_event("UNIT_AURA_GUID"));
                }
            }
            i if (FIELD_AURA_APPLICATIONS..FIELD_AURA_APPLICATIONS + 12).contains(&i) => {
                for byte in 0..4u16 {
                    let shift = 8 * byte;
                    let (was, now) = ((old >> shift) & 0xff, (new >> shift) & 0xff);
                    let slot = 4 * (i - FIELD_AURA_APPLICATIONS) + byte;
                    let spell = field(FIELD_AURA + slot);
                    if was != now && spell != 0 {
                        out.push(self.aura_event(guid, slot, spell, now > was, 2, field));
                    }
                }
            }
            _ => {}
        }
        out
    }

    /// One `BUFF_*`/`DEBUFF_*` event: `state` 0 added, 1 removed, 2 a stack change.
    fn aura_event(
        &self,
        guid: u64,
        slot: u16,
        spell: u32,
        added: bool,
        state: i64,
        field: &dyn Fn(u16) -> u32,
    ) -> NamedEvent {
        let debuff = slot >= FIRST_DEBUFF_SLOT;
        let name = match (debuff, added, self.mine(guid)) {
            (false, true, true) => "BUFF_ADDED_SELF",
            (false, false, true) => "BUFF_REMOVED_SELF",
            (false, true, false) => "BUFF_ADDED_OTHER",
            (false, false, false) => "BUFF_REMOVED_OTHER",
            (true, true, true) => "DEBUFF_ADDED_SELF",
            (true, false, true) => "DEBUFF_REMOVED_SELF",
            (true, true, false) => "DEBUFF_ADDED_OTHER",
            (true, false, false) => "DEBUFF_REMOVED_OTHER",
        };
        let byte = |base: u16| (field(base + slot / 4) >> (8 * (slot % 4))) & 0xff;
        // The 1-based slot `UnitBuff`/`UnitDebuff` would show it at: the filled slots of its
        // range up to it, 0 once it is gone.
        let first = if debuff { FIRST_DEBUFF_SLOT } else { 0 };
        let lua_slot = if state == 1 {
            0
        } else {
            (first..=slot)
                .filter(|&k| field(FIELD_AURA + k) != 0)
                .count() as i64
        };
        let stacks = if state == 1 {
            0
        } else {
            i64::from(byte(FIELD_AURA_APPLICATIONS)) + 1
        };
        (
            name,
            vec![
                s(guid),
                i(lua_slot),
                i(spell),
                i(stacks),
                i(byte(FIELD_AURA_LEVELS)),
                i(slot),
                i(state),
            ],
        )
    }

    /// The `*_GUID` events' arguments. Deviation: the token flags beyond the player read 0.
    fn guid_args(&self, guid: u64) -> Vec<ScriptValue> {
        vec![s(guid), flag(self.mine(guid)), i(0), i(0), i(0), i(0), i(0)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> Ctx<'static> {
        Ctx {
            me: Some(0x10),
            cvars: None,
            spells: None,
        }
    }

    #[test]
    fn our_start_signals_the_cast_and_ungated_events_fire_without_cvars() {
        let start = SessionEvent::SpellStart {
            caster: 0x10,
            spell_id: 133,
            cast_flags: 0,
            cast_time_ms: 3500,
            target: Some(0xA),
            ammo_display_id: None,
        };
        let names: Vec<_> = ctx()
            .packet_events(&start)
            .into_iter()
            .map(|e| e.0)
            .collect();
        assert_eq!(names, ["SPELL_CAST_EVENT"], "the start event is gated off");
        let failed = SessionEvent::CastResult {
            spell_id: 133,
            success: false,
            reason: Some(12),
            arg: None,
        };
        let events = ctx().packet_events(&failed);
        assert_eq!(events[0].0, "SPELL_FAILED_SELF");
        assert_eq!(events[0].1[1], ScriptValue::Int(12));
    }

    #[test]
    fn aura_slots_name_buff_or_debuff_and_count_lua_slots() {
        // Debuffs in slots 32 and 34; 34 is the second shown.
        let fields = |i: u16| match i {
            79 | 81 => 700,
            _ => 0,
        };
        let added = ctx().field_events(0xF1, FIELD_AURA + 34, 0, 700, &fields);
        assert_eq!(added[0].0, "DEBUFF_ADDED_OTHER");
        assert_eq!(added[0].1[1], ScriptValue::Int(2));
        assert_eq!(added[0].1[3], ScriptValue::Int(1), "one stack");
        let removed = ctx().field_events(0x10, FIELD_AURA + 3, 55, 0, &fields);
        assert_eq!(removed[0].0, "BUFF_REMOVED_SELF");
        assert_eq!(removed[0].1[6], ScriptValue::Int(1));
        let died = ctx().field_events(0xF1, FIELD_UNIT_HEALTH, 40, 0, &fields);
        assert_eq!(died[0], ("UNIT_DIED", vec![s(0xF1)]));
    }
}
