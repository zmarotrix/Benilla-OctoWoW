//! nampower's script surface, which OctoWoW's client mods add to 1.12 and pfUI, Cursive and
//! HealersMate read: unit fields and spell records by name, spell lookups by name, and the unit
//! token `GetUnitGUID`. The field names and layouts are nampower's (`UNIT_FIELDS.md`,
//! `DBC_FIELDS.md`): its `UnitFields` struct is the 1.12 unit descriptor from field 6, and its
//! `SpellRec` the 173 `Spell.dbc` columns. Deviation: none of this is in 1.12.1.
//!
//! nampower hands out reused tables unless `copy` is passed; these always build a fresh one,
//! which every caller written for the reused form reads the same.

use mlua::{Lua, Table, Value};

use super::binding_abi;

/// How a field's dwords read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FieldKind {
    U32,
    I32,
    F32,
    /// Bytes packed four to a dword, low byte first.
    U8,
    /// Two dwords, low first, read as SuperWoW's guid text.
    Guid,
    /// Two dwords, low first, read as a number.
    U64,
    /// A localized string, the client's locale's.
    Str,
}

/// `(lua name, first index, element count, kind)`.
type Field = (&'static str, usize, usize, FieldKind);

/// nampower's `UnitFields`, by update-field index (`UNIT_FIELD_CHARM` is 6).
pub(crate) const UNIT_FIELDS: &[Field] = &[
    ("charm", 6, 1, FieldKind::Guid),
    ("summon", 8, 1, FieldKind::Guid),
    ("charmedBy", 10, 1, FieldKind::Guid),
    ("summonedBy", 12, 1, FieldKind::Guid),
    ("createdBy", 14, 1, FieldKind::Guid),
    ("target", 16, 1, FieldKind::Guid),
    ("persuaded", 18, 1, FieldKind::Guid),
    ("channelObject", 20, 1, FieldKind::Guid),
    ("health", 22, 1, FieldKind::U32),
    ("power1", 23, 1, FieldKind::U32),
    ("power2", 24, 1, FieldKind::U32),
    ("power3", 25, 1, FieldKind::U32),
    ("power4", 26, 1, FieldKind::U32),
    ("power5", 27, 1, FieldKind::U32),
    ("maxHealth", 28, 1, FieldKind::U32),
    ("maxPower1", 29, 1, FieldKind::U32),
    ("maxPower2", 30, 1, FieldKind::U32),
    ("maxPower3", 31, 1, FieldKind::U32),
    ("maxPower4", 32, 1, FieldKind::U32),
    ("maxPower5", 33, 1, FieldKind::U32),
    ("level", 34, 1, FieldKind::U32),
    ("factionTemplate", 35, 1, FieldKind::U32),
    ("bytes0", 36, 1, FieldKind::U32),
    ("virtualItemDisplay", 37, 3, FieldKind::U32),
    ("virtualItemInfo", 40, 6, FieldKind::U32),
    ("flags", 46, 1, FieldKind::U32),
    ("aura", 47, 48, FieldKind::U32),
    ("auraFlags", 95, 6, FieldKind::U32),
    ("auraLevels", 101, 48, FieldKind::U8),
    ("auraApplications", 113, 48, FieldKind::U8),
    ("auraState", 125, 1, FieldKind::U32),
    ("baseAttackTime", 126, 1, FieldKind::U32),
    ("offhandAttackTime", 127, 1, FieldKind::U32),
    ("rangedAttackTime", 128, 1, FieldKind::U32),
    ("boundingRadius", 129, 1, FieldKind::F32),
    ("combatReach", 130, 1, FieldKind::F32),
    ("displayId", 131, 1, FieldKind::U32),
    ("nativeDisplayId", 132, 1, FieldKind::U32),
    ("mountDisplayId", 133, 1, FieldKind::U32),
    ("minDamage", 134, 1, FieldKind::F32),
    ("maxDamage", 135, 1, FieldKind::F32),
    ("minOffhandDamage", 136, 1, FieldKind::F32),
    ("maxOffhandDamage", 137, 1, FieldKind::F32),
    ("bytes1", 138, 1, FieldKind::U32),
    ("petNumber", 139, 1, FieldKind::U32),
    ("petNameTimestamp", 140, 1, FieldKind::U32),
    ("petExperience", 141, 1, FieldKind::U32),
    ("petNextLevelExp", 142, 1, FieldKind::U32),
    ("dynamicFlags", 143, 1, FieldKind::U32),
    ("channelSpell", 144, 1, FieldKind::U32),
    ("modCastSpeed", 145, 1, FieldKind::F32),
    ("createdBySpell", 146, 1, FieldKind::U32),
    ("npcFlags", 147, 1, FieldKind::U32),
    ("npcEmoteState", 148, 1, FieldKind::U32),
    ("trainingPoints", 149, 1, FieldKind::U32),
    ("stat0", 150, 1, FieldKind::U32),
    ("stat1", 151, 1, FieldKind::U32),
    ("stat2", 152, 1, FieldKind::U32),
    ("stat3", 153, 1, FieldKind::U32),
    ("stat4", 154, 1, FieldKind::U32),
    ("resistances", 155, 7, FieldKind::U32),
    ("baseMana", 162, 1, FieldKind::U32),
    ("baseHealth", 163, 1, FieldKind::U32),
    ("bytes2", 164, 1, FieldKind::U32),
    ("attackPower", 165, 1, FieldKind::U32),
    ("attackPowerMods", 166, 1, FieldKind::U32),
    ("attackPowerMultiplier", 167, 1, FieldKind::F32),
    ("rangedAttackPower", 168, 1, FieldKind::U32),
    ("rangedAttackPowerMods", 169, 1, FieldKind::U32),
    ("rangedAttackPowerMultiplier", 170, 1, FieldKind::F32),
    ("minRangedDamage", 171, 1, FieldKind::F32),
    ("maxRangedDamage", 172, 1, FieldKind::F32),
    ("powerCostModifier", 173, 7, FieldKind::F32),
    ("powerCostMultiplier", 180, 7, FieldKind::F32),
];

/// nampower's `SpellRec`, by `Spell.dbc` column.
pub(crate) const SPELL_FIELDS: &[Field] = &[
    ("id", 0, 1, FieldKind::U32),
    ("school", 1, 1, FieldKind::U32),
    ("category", 2, 1, FieldKind::U32),
    ("castUI", 3, 1, FieldKind::U32),
    ("dispel", 4, 1, FieldKind::U32),
    ("mechanic", 5, 1, FieldKind::U32),
    ("attributes", 6, 1, FieldKind::U32),
    ("attributesEx", 7, 1, FieldKind::U32),
    ("attributesEx2", 8, 1, FieldKind::U32),
    ("attributesEx3", 9, 1, FieldKind::U32),
    ("attributesEx4", 10, 1, FieldKind::U32),
    ("stances", 11, 1, FieldKind::U32),
    ("stancesNot", 12, 1, FieldKind::U32),
    ("targets", 13, 1, FieldKind::U32),
    ("targetCreatureType", 14, 1, FieldKind::U32),
    ("requiresSpellFocus", 15, 1, FieldKind::U32),
    ("casterAuraState", 16, 1, FieldKind::U32),
    ("targetAuraState", 17, 1, FieldKind::U32),
    ("castingTimeIndex", 18, 1, FieldKind::U32),
    ("recoveryTime", 19, 1, FieldKind::U32),
    ("categoryRecoveryTime", 20, 1, FieldKind::U32),
    ("interruptFlags", 21, 1, FieldKind::U32),
    ("auraInterruptFlags", 22, 1, FieldKind::U32),
    ("channelInterruptFlags", 23, 1, FieldKind::U32),
    ("procFlags", 24, 1, FieldKind::U32),
    ("procChance", 25, 1, FieldKind::U32),
    ("procCharges", 26, 1, FieldKind::U32),
    ("maxLevel", 27, 1, FieldKind::U32),
    ("baseLevel", 28, 1, FieldKind::U32),
    ("spellLevel", 29, 1, FieldKind::U32),
    ("durationIndex", 30, 1, FieldKind::U32),
    ("powerType", 31, 1, FieldKind::U32),
    ("manaCost", 32, 1, FieldKind::U32),
    ("manaCostPerlevel", 33, 1, FieldKind::U32),
    ("manaPerSecond", 34, 1, FieldKind::U32),
    ("manaPerSecondPerLevel", 35, 1, FieldKind::U32),
    ("rangeIndex", 36, 1, FieldKind::U32),
    ("speed", 37, 1, FieldKind::F32),
    ("modalNextSpell", 38, 1, FieldKind::U32),
    ("stackAmount", 39, 1, FieldKind::U32),
    ("totem", 40, 2, FieldKind::U32),
    ("reagent", 42, 8, FieldKind::I32),
    ("reagentCount", 50, 8, FieldKind::U32),
    ("equippedItemClass", 58, 1, FieldKind::I32),
    ("equippedItemSubClassMask", 59, 1, FieldKind::I32),
    ("equippedItemInventoryTypeMask", 60, 1, FieldKind::I32),
    ("effect", 61, 3, FieldKind::U32),
    ("effectDieSides", 64, 3, FieldKind::I32),
    ("effectBaseDice", 67, 3, FieldKind::U32),
    ("effectDicePerLevel", 70, 3, FieldKind::F32),
    ("effectRealPointsPerLevel", 73, 3, FieldKind::F32),
    ("effectBasePoints", 76, 3, FieldKind::I32),
    ("effectMechanic", 79, 3, FieldKind::U32),
    ("effectImplicitTargetA", 82, 3, FieldKind::U32),
    ("effectImplicitTargetB", 85, 3, FieldKind::U32),
    ("effectRadiusIndex", 88, 3, FieldKind::U32),
    ("effectApplyAuraName", 91, 3, FieldKind::U32),
    ("effectAmplitude", 94, 3, FieldKind::U32),
    ("effectMultipleValue", 97, 3, FieldKind::F32),
    ("effectChainTarget", 100, 3, FieldKind::U32),
    ("effectItemType", 103, 3, FieldKind::U32),
    ("effectMiscValue", 106, 3, FieldKind::I32),
    ("effectTriggerSpell", 109, 3, FieldKind::U32),
    ("effectPointsPerComboPoint", 112, 3, FieldKind::F32),
    ("spellVisual", 115, 1, FieldKind::U32),
    ("spellVisual2", 116, 1, FieldKind::U32),
    ("spellIconID", 117, 1, FieldKind::U32),
    ("activeIconID", 118, 1, FieldKind::U32),
    ("spellPriority", 119, 1, FieldKind::U32),
    ("name", 120, 1, FieldKind::Str),
    ("rank", 129, 1, FieldKind::Str),
    ("description", 138, 1, FieldKind::Str),
    ("tooltip", 147, 1, FieldKind::Str),
    ("manaCostPercentage", 156, 1, FieldKind::U32),
    ("startRecoveryCategory", 157, 1, FieldKind::U32),
    ("startRecoveryTime", 158, 1, FieldKind::U32),
    ("maxTargetLevel", 159, 1, FieldKind::U32),
    ("spellFamilyName", 160, 1, FieldKind::U32),
    ("spellFamilyFlags", 161, 1, FieldKind::U64),
    ("maxAffectedTargets", 163, 1, FieldKind::U32),
    ("dmgClass", 164, 1, FieldKind::U32),
    ("preventionType", 165, 1, FieldKind::U32),
    ("stanceBarOrder", 166, 1, FieldKind::U32),
    ("dmgMultiplier", 167, 3, FieldKind::F32),
    ("minFactionId", 170, 1, FieldKind::U32),
    ("minReputation", 171, 1, FieldKind::U32),
    ("requiredAuraVision", 172, 1, FieldKind::U32),
];

/// A unit's raw update fields by index, 0 where unsent: `GetUnitData`'s question.
pub struct UnitFieldsQuery(pub u64);

impl super::live::LiveQuery for UnitFieldsQuery {
    /// `None` while the object manager holds no unit under the guid.
    type Answer = Option<Vec<u32>>;
}

/// A `Spell.dbc` row with its localized strings: `GetSpellRec`'s question.
pub struct SpellRowQuery(pub u32);

/// What [`SpellRowQuery`] answers.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SpellRowAnswer {
    pub columns: Vec<u32>,
    pub name: String,
    pub rank: String,
    pub description: String,
    pub tooltip: String,
}

impl super::live::LiveQuery for SpellRowQuery {
    /// `None` for an id the file lacks.
    type Answer = Option<SpellRowAnswer>;
}

/// `SpellIcon.dbc`'s path for an icon id: `GetSpellIconTexture`'s question.
pub struct SpellIconQuery(pub u32);

impl super::live::LiveQuery for SpellIconQuery {
    type Answer = Option<String>;
}

/// The value of `field` read off `words` (indexed as the field's indices), strings off `strings`.
/// `IsSpellUsable` and `IsSpellInRange`'s question: the action bar's usable walk for the spell
/// against the selection, and its range pre-check against `unit`'s object (the selection's with
/// none). `None` for a spell the file lacks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpellNowQuery {
    pub spell: u32,
    pub unit: Option<u64>,
}

/// `(usable, out of power, in range)`, in range `None` where the check answers nothing.
pub type SpellNowAnswer = Option<(bool, bool, Option<bool>)>;

impl super::live::LiveQuery for SpellNowQuery {
    type Answer = SpellNowAnswer;
}

/// The client's spell queue ([`install_in_game`]'s `BenillaSpellQueue`): a press inside
/// `NP_SpellQueueWindowMs` of the cast or cooldown ahead of it is held and sent when the client
/// allows. The answer is the queued spell's id, 0 for none, as of after `op`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpellQueueQuery {
    /// What is queued.
    Peek,
    /// Drop what is queued (`/unqueue`).
    Clear,
    /// The next press casts or refuses as 1.12 does, never queued (`CastSpellByNameNoQueue`).
    SkipNext(bool),
}

impl super::live::LiveQuery for SpellQueueQuery {
    type Answer = u32;
}

/// A `SpellRange.dbc` row by index: min and max yards and flags; `None` for no row.
pub struct SpellRangeQuery(pub u32);

impl super::live::LiveQuery for SpellRangeQuery {
    type Answer = Option<(f32, f32, u32)>;
}

/// A spell's base cast time in milliseconds off `SpellCastTimes.dbc`, before haste; `None` for a
/// spell the file lacks.
pub struct CastTimeQuery(pub u32);

impl super::live::LiveQuery for CastTimeQuery {
    type Answer = Option<u32>;
}

fn field_value(
    lua: &Lua,
    field: &Field,
    words: &[u32],
    strings: &dyn Fn(&str) -> String,
) -> mlua::Result<Value> {
    let &(name, first, count, kind) = field;
    let word = |i: usize| words.get(i).copied().unwrap_or(0);
    let one = |i: usize| -> mlua::Result<Value> {
        Ok(match kind {
            FieldKind::U32 => Value::Integer(i64::from(word(first + i))),
            FieldKind::I32 => Value::Integer(i64::from(word(first + i) as i32)),
            FieldKind::F32 => Value::Number(f64::from(f32::from_bits(word(first + i)))),
            FieldKind::U8 => {
                Value::Integer(i64::from((word(first + i / 4) >> (8 * (i % 4))) as u8))
            }
            FieldKind::Guid | FieldKind::U64 => {
                let at = first + 2 * i;
                let v = u64::from(word(at)) | (u64::from(word(at + 1)) << 32);
                if kind == FieldKind::Guid {
                    Value::String(lua.create_string(super::superwow::guid_text(v))?)
                } else {
                    Value::Number(v as f64)
                }
            }
            FieldKind::Str => Value::String(lua.create_string(strings(name))?),
        })
    };
    if count == 1 {
        return one(0);
    }
    let t = lua.create_table()?;
    for i in 0..count {
        t.raw_set(i + 1, one(i)?)?;
    }
    Ok(Value::Table(t))
}

/// Every field of `fields` into one table.
fn record_table(
    lua: &Lua,
    fields: &[Field],
    words: &[u32],
    strings: &dyn Fn(&str) -> String,
) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    for f in fields {
        t.raw_set(f.0, field_value(lua, f, words, strings)?)?;
    }
    Ok(t)
}

/// The field named `name`, raising nampower's error for an unknown one.
fn field_named<'a>(fields: &'a [Field], name: &str) -> mlua::Result<&'a Field> {
    fields
        .iter()
        .find(|f| f.0 == name)
        .ok_or_else(|| mlua::Error::runtime(format!("Unknown field name: {name}")))
}

/// The guid a unit token or guid text names, 0 for nobody.
fn unit_guid(lua: &Lua, token: &str) -> mlua::Result<u64> {
    let model = lua.app_data_ref::<super::Model>().expect("model app_data");
    Ok(super::unit::guid_of(lua, &model, token)?.unwrap_or(0))
}

/// A unit's raw fields, `None` for nobody or a unit not held.
fn unit_words(lua: &Lua, token: &str) -> mlua::Result<Option<Vec<u32>>> {
    let guid = unit_guid(lua, token)?;
    if guid == 0 {
        return Ok(None);
    }
    Ok(super::live::ask(lua, &UnitFieldsQuery(guid)).flatten())
}

/// A spell's row, `None` for an id the file lacks or no host to ask.
fn spell_row(lua: &Lua, id: Value, usage: &'static str) -> mlua::Result<Option<SpellRowAnswer>> {
    let id = binding_abi::number_arg(lua, id, usage)?;
    let Ok(id) = u32::try_from(id) else {
        return Ok(None);
    };
    Ok(super::live::ask(lua, &SpellRowQuery(id)).flatten())
}

/// The row's localized string named `name`.
fn row_string(row: &SpellRowAnswer) -> impl Fn(&str) -> String + '_ {
    move |name| match name {
        "name" => row.name.clone(),
        "rank" => row.rank.clone(),
        "description" => row.description.clone(),
        _ => row.tooltip.clone(),
    }
}

/// The player's or pet's book slot of a spell named as `CastSpellByName` names it.
fn book_named(lua: &Lua, name: &str) -> Option<super::spellbook::NamedSpell> {
    let model = lua.app_data_ref::<super::Model>().expect("model app_data");
    super::spellbook::named_in_books(lua, &model, name.as_bytes())
}

/// A spell named by id or by a name the books hold, 0 for neither.
fn spell_id_of(lua: &Lua, spell: Value, usage: &'static str) -> mlua::Result<u32> {
    Ok(match spell {
        Value::Integer(n) => u32::try_from(n).unwrap_or(0),
        Value::Number(n) => n as u32,
        Value::String(s) => {
            let text = s.to_str()?.to_string();
            match text.trim().parse::<u32>() {
                Ok(n) => n,
                Err(_) => book_named(lua, &text).map_or(0, |f| f.spell_id),
            }
        }
        _ => return Err(mlua::Error::runtime(usage)),
    })
}

/// nampower's cooldown detail table for a `(start, duration, enabled)` in `GetTime` seconds, `None`
/// for none. A running cooldown of 1.5 s or less is the global one.
fn cooldown_table(
    lua: &Lua,
    triple: Option<(f64, f64, bool)>,
    item: u32,
) -> mlua::Result<mlua::Table> {
    let now = super::clock::now(lua);
    let t = lua.create_table()?;
    let (start, duration) = triple.filter(|c| c.2).map_or((0.0, 0.0), |c| (c.0, c.1));
    let remaining_ms = (((start + duration) - now) * 1000.0).max(0.0).round() as i64;
    let on = remaining_ms > 0;
    let gcd = on && duration <= 1.5;
    let duration_ms = (duration * 1000.0).round() as i64;
    t.set("isOnCooldown", i64::from(on))?;
    t.set("cooldownRemainingMs", remaining_ms)?;
    t.set("itemId", i64::from(item))?;
    t.set("itemHasActiveSpell", i64::from(item != 0))?;
    t.set("itemActiveSpellId", 0)?;
    let part = |prefix: &str, active: bool| -> mlua::Result<()> {
        let (s, d, r) = if active {
            (start, duration_ms, remaining_ms)
        } else {
            (0.0, 0, 0)
        };
        t.set(format!("{prefix}StartS"), s)?;
        t.set(format!("{prefix}DurationMs"), d)?;
        t.set(format!("{prefix}RemainingMs"), r)?;
        Ok(())
    };
    part("individual", on && !gcd)?;
    t.set("isOnIndividualCooldown", i64::from(on && !gcd))?;
    part("category", false)?;
    t.set("categoryId", 0)?;
    t.set("isOnCategoryCooldown", 0)?;
    part("gcdCategory", gcd)?;
    t.set("gcdCategoryId", if gcd { 133 } else { 0 })?;
    t.set("isOnGcdCategoryCooldown", i64::from(gcd))?;
    Ok(t)
}

/// The in-game globals [`install_in_game`] adds: the surface gates' exceptions.
#[cfg(test)]
pub(crate) const IN_GAME_GLOBALS: &[&str] = &[
    "GetUnitGUID",
    "GetUnitData",
    "GetUnitField",
    "GetSpellRec",
    "GetSpellRecField",
    "GetSpellIconTexture",
    "GetSpellIdForName",
    "GetSpellNameAndRankForId",
    "GetSpellSlotTypeIdForName",
    "IsSpellUsable",
    "IsSpellInRange",
    "GetSpellIdCooldown",
    "GetItemIdCooldown",
    "BenillaSpellQueue",
    "BenillaTargetAtCursor",
    "GetSpellRangeData",
];

/// nampower's events, which the in-game table gains beside SuperWoW's.
pub const IN_GAME_EVENTS: &[&str] = &[
    "SPELL_START_SELF",
    "SPELL_START_OTHER",
    "SPELL_GO_SELF",
    "SPELL_GO_OTHER",
    "SPELL_FAILED_SELF",
    "SPELL_FAILED_OTHER",
    "SPELL_DELAYED_SELF",
    "SPELL_DELAYED_OTHER",
    "SPELL_CHANNEL_START",
    "SPELL_CAST_EVENT",
    "SPELL_QUEUE_EVENT",
    "AURA_CAST_ON_SELF",
    "AURA_CAST_ON_OTHER",
    "AUTO_ATTACK_SELF",
    "AUTO_ATTACK_OTHER",
    "SPELL_DAMAGE_EVENT_SELF",
    "SPELL_DAMAGE_EVENT_OTHER",
    "SPELL_HEAL_BY_SELF",
    "SPELL_HEAL_BY_OTHER",
    "SPELL_HEAL_ON_SELF",
    "SPELL_MISS_SELF",
    "SPELL_MISS_OTHER",
    "UNIT_DIED",
    "BUFF_ADDED_SELF",
    "BUFF_REMOVED_SELF",
    "BUFF_ADDED_OTHER",
    "BUFF_REMOVED_OTHER",
    "DEBUFF_ADDED_SELF",
    "DEBUFF_REMOVED_SELF",
    "DEBUFF_ADDED_OTHER",
    "DEBUFF_REMOVED_OTHER",
    "UNIT_HEALTH_GUID",
    "UNIT_FLAGS_GUID",
    "UNIT_AURA_GUID",
    "UNIT_MODEL_CHANGED_GUID",
    "UNIT_COMBAT_GUID",
];

/// The in-game globals.
pub(super) fn install_in_game(lua: &Lua) -> mlua::Result<()> {
    let g = lua.globals();

    // `GetSpellRangeData(rangeIndex)`: the `SpellRange.dbc` row's min range, max range and
    // flags (bit 0 the melee family); nil for no row.
    g.set(
        "GetSpellRangeData",
        lua.create_function(|lua, index: i64| {
            let Ok(index) = u32::try_from(index) else {
                return Ok((None, None, None));
            };
            Ok(
                match super::live::ask(lua, &SpellRangeQuery(index)).flatten() {
                    Some((min, max, flags)) => (
                        Some(f64::from(min)),
                        Some(f64::from(max)),
                        Some(i64::from(flags)),
                    ),
                    None => (None, None, None),
                },
            )
        })?,
    )?;

    // `BenillaTargetAtCursor()`: the waiting ground spell placed at the world point under the
    // mouse, the smart macros' `[cursor]` and ClassicAPI's `C_Spell.CastAtCursor`; 1 when a
    // spell was waiting.
    g.set(
        "BenillaTargetAtCursor",
        lua.create_function(|lua, _: mlua::MultiValue| {
            if !super::protected::gate(lua, super::protected::Tag::CAST)
                || !super::action::spell_targeting(lua)
            {
                return Ok(false);
            }
            super::calls::submit(lua, super::ScriptCall::SpellTargetCursor)?;
            Ok(true)
        })?,
    )?;

    // `BenillaSpellQueue([op])`: the queued spell's id (0 for none) after `op`, "clear" to drop
    // it, "skip" or "unskip" to have the next press bypass the queue. The adapters' nampower
    // queue verbs and the smart macros' `/unqueue` and `[queuedspell]` read it.
    g.set(
        "BenillaSpellQueue",
        lua.create_function(|lua, op: Option<String>| {
            let query = match op.as_deref() {
                Some("clear") => SpellQueueQuery::Clear,
                Some("skip") => SpellQueueQuery::SkipNext(true),
                Some("unskip") => SpellQueueQuery::SkipNext(false),
                _ => SpellQueueQuery::Peek,
            };
            Ok(i64::from(super::live::ask(lua, &query).unwrap_or(0)))
        })?,
    )?;

    // `GetUnitGUID(unit)`: the unit's guid as SuperWoW's tokens take it, nil for nobody. pfUI's
    // castbars call it unguarded.
    g.set(
        "GetUnitGUID",
        lua.create_function(|lua, unit: Value| {
            let token = binding_abi::string_arg(lua, unit, "Usage: GetUnitGUID(unitToken)")?;
            Ok(match unit_guid(lua, &token)? {
                0 => Value::Nil,
                guid => Value::String(lua.create_string(super::superwow::guid_text(guid))?),
            })
        })?,
    )?;

    // `GetUnitData(unit [, copy])`: every unit field, nil for a unit not held.
    g.set(
        "GetUnitData",
        lua.create_function(|lua, (unit, _copy): (Value, Option<Value>)| {
            let token =
                binding_abi::string_arg(lua, unit, "Usage: GetUnitData(unitToken, [copy])")?;
            Ok(match unit_words(lua, &token)? {
                Some(words) => {
                    Value::Table(record_table(lua, UNIT_FIELDS, &words, &|_| String::new())?)
                }
                None => Value::Nil,
            })
        })?,
    )?;

    // `GetUnitField(unit, field [, copy])`: one field, nil for a unit not held; an unknown field
    // name raises.
    g.set(
        "GetUnitField",
        lua.create_function(|lua, (unit, name, _copy): (Value, Value, Option<Value>)| {
            const USAGE: &str = "Usage: GetUnitField(unitToken, fieldName, [copy])";
            let token = binding_abi::string_arg(lua, unit, USAGE)?;
            let name = binding_abi::string_arg(lua, name, USAGE)?;
            let field = field_named(UNIT_FIELDS, &name)?;
            match unit_words(lua, &token)? {
                Some(words) => field_value(lua, field, &words, &|_| String::new()),
                None => Ok(Value::Nil),
            }
        })?,
    )?;

    // `GetSpellRec(spellId [, copy])`: every field of the row, nil for an unknown id.
    g.set(
        "GetSpellRec",
        lua.create_function(|lua, (id, _copy): (Value, Option<Value>)| {
            let row = spell_row(lua, id, "Usage: GetSpellRec(spellId, [copy])")?;
            Ok(match row {
                Some(row) => Value::Table(record_table(
                    lua,
                    SPELL_FIELDS,
                    &row.columns,
                    &row_string(&row),
                )?),
                None => Value::Nil,
            })
        })?,
    )?;

    // `GetSpellRecField(spellId, field [, copy])`: one field; an unknown field name raises.
    g.set(
        "GetSpellRecField",
        lua.create_function(|lua, (id, name, _copy): (Value, Value, Option<Value>)| {
            const USAGE: &str = "Usage: GetSpellRecField(spellId, fieldName, [copy])";
            let name = binding_abi::string_arg(lua, name, USAGE)?;
            let field = field_named(SPELL_FIELDS, &name)?;
            match spell_row(lua, id, USAGE)? {
                Some(row) => field_value(lua, field, &row.columns, &row_string(&row)),
                None => Ok(Value::Nil),
            }
        })?,
    )?;

    // `GetSpellIconTexture(spellIconId)`: the icon's path, nil for none.
    g.set(
        "GetSpellIconTexture",
        lua.create_function(|lua, id: Value| {
            let id = binding_abi::number_arg(lua, id, "Usage: GetSpellIconTexture(spellIconId)")?;
            let Ok(id) = u32::try_from(id) else {
                return Ok(None);
            };
            Ok(super::live::ask(lua, &SpellIconQuery(id)).flatten())
        })?,
    )?;

    // `GetSpellIdForName(name)`: the book's spell id for the name (its highest rank without one),
    // 0 when the books lack it.
    g.set(
        "GetSpellIdForName",
        lua.create_function(|lua, name: Value| {
            let name = binding_abi::string_arg(lua, name, "Usage: GetSpellIdForName(spellName)")?;
            Ok(i64::from(book_named(lua, &name).map_or(0, |f| f.spell_id)))
        })?,
    )?;

    // `GetSpellNameAndRankForId(id)`: the row's name and rank.
    g.set(
        "GetSpellNameAndRankForId",
        lua.create_function(|lua, id: Value| {
            let row = spell_row(lua, id, "Usage: GetSpellNameAndRankForId(spellId)")?;
            Ok(match row {
                Some(row) => (Some(row.name), Some(row.rank)),
                None => (None, None),
            })
        })?,
    )?;

    // `GetSpellSlotTypeIdForName(name)`: the 1-based book slot, "spell" or "pet", and the id;
    // 0, "unknown", 0 when the books lack it.
    g.set(
        "GetSpellSlotTypeIdForName",
        lua.create_function(|lua, name: Value| {
            let name =
                binding_abi::string_arg(lua, name, "Usage: GetSpellSlotTypeIdForName(spellName)")?;
            Ok(match book_named(lua, &name) {
                Some(f) => (
                    i64::from(f.slot),
                    if f.pet { "pet" } else { "spell" },
                    i64::from(f.spell_id),
                ),
                None => (0, "unknown", 0),
            })
        })?,
    )?;

    // `IsSpellUsable(spell)`: the action bar's usable walk for the spell, as `IsUsableAction`
    // answers a slot holding it: usable, then out of power, each 1 or 0.
    g.set(
        "IsSpellUsable",
        lua.create_function(|lua, spell: Value| {
            let id = spell_id_of(lua, spell, "Usage: IsSpellUsable(spellNameOrId)")?;
            let now = super::live::ask(
                lua,
                &SpellNowQuery {
                    spell: id,
                    unit: None,
                },
            )
            .flatten();
            Ok(now.map_or((0, 0), |(u, oom, _)| (i64::from(u), i64::from(oom))))
        })?,
    )?;

    // `IsSpellInRange(spell[, unit])`: 1 in range, 0 out of it, -1 for a spell or unit it cannot
    // test, the range pre-check `IsActionInRange` runs, against the unit ("target" by default).
    g.set(
        "IsSpellInRange",
        lua.create_function(|lua, (spell, unit): (Value, Option<Value>)| {
            let id = spell_id_of(lua, spell, "Usage: IsSpellInRange(spellNameOrId[, unit])")?;
            let token = match unit {
                Some(Value::Nil) | None => "target".to_string(),
                Some(u) => binding_abi::string_arg(lua, u, "Usage: IsSpellInRange(spell[, unit])")?,
            };
            let guid = unit_guid(lua, &token)?;
            if id == 0 || guid == 0 {
                return Ok(-1);
            }
            let query = SpellNowQuery {
                spell: id,
                unit: Some(guid),
            };
            Ok(match super::live::ask(lua, &query).flatten() {
                Some((_, _, Some(true))) => 1,
                Some((_, _, Some(false))) => 0,
                _ => -1,
            })
        })?,
    )?;

    // `GetSpellIdCooldown(spellId)` and `GetItemIdCooldown(itemId)`: the cooldown lists'
    // answer for the spell or item as a detail table.
    g.set(
        "GetSpellIdCooldown",
        lua.create_function(|lua, id: Value| {
            let id = binding_abi::number_arg(lua, id, "Usage: GetSpellIdCooldown(spellId)")?;
            let id = u32::try_from(id).unwrap_or(0);
            let query = super::cooldown_read::CooldownQuery::Spell(id);
            let triple = super::cooldown_read::read_live(lua, query).flatten();
            cooldown_table(lua, triple, 0)
        })?,
    )?;
    g.set(
        "GetItemIdCooldown",
        lua.create_function(|lua, id: Value| {
            let id = binding_abi::number_arg(lua, id, "Usage: GetItemIdCooldown(itemId)")?;
            let id = u32::try_from(id).unwrap_or(0);
            let query = super::cooldown_read::CooldownQuery::Item(id);
            let triple = super::cooldown_read::read_live(lua, query).flatten();
            cooldown_table(lua, triple, id)
        })?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::script::{UiScript, UnitGuids};

    /// Each table runs gapless to its end: the unit block from 6 to 187 (1.12's `UNIT_END` 188
    /// less its padding word), the spell row over all 173 columns with each string's eight
    /// locale columns and its flag column skipped.
    #[test]
    fn the_field_tables_tile_their_records() {
        let width = |f: &Field| match f.3 {
            FieldKind::U8 => f.2.div_ceil(4),
            FieldKind::Guid | FieldKind::U64 => 2 * f.2,
            FieldKind::Str => 9,
            _ => f.2,
        };
        for (fields, start, end) in [(UNIT_FIELDS, 6, 187), (SPELL_FIELDS, 0, 173)] {
            let mut at = start;
            for f in fields {
                assert_eq!(f.1, at, "{} starts where the one before ends", f.0);
                at += width(f);
            }
            assert_eq!(at, end);
        }
    }

    #[test]
    fn get_unit_guid_reads_the_token() {
        let mut s = UiScript::new().unwrap();
        s.set_unit_guids(&UnitGuids {
            player: 0x10,
            ..Default::default()
        });
        assert_eq!(
            s.eval::<String>(r#"return GetUnitGUID("player")"#).unwrap(),
            "0x0000000000000010"
        );
        assert!(s
            .eval::<bool>(r#"return GetUnitGUID("target") == nil"#)
            .unwrap());
    }

    /// The readers decode each kind: packed bytes, a guid, a float, a signed array, a string.
    #[test]
    fn fields_decode_by_kind() {
        let lua = Lua::new();
        let mut words = vec![0u32; 188];
        words[16] = 0x21; // target, low half
        words[22] = 500; // health
        words[101] = 0x0302_0100; // auraLevels 1..4
        words[145] = 1.25f32.to_bits(); // modCastSpeed
        let read = |name: &str| {
            field_value(
                &lua,
                field_named(UNIT_FIELDS, name).unwrap(),
                &words,
                &|_| String::new(),
            )
            .unwrap()
        };
        assert_eq!(read("health"), Value::Integer(500));
        assert_eq!(read("modCastSpeed"), Value::Number(1.25));
        let Value::String(t) = read("target") else {
            panic!()
        };
        assert_eq!(t.to_str().unwrap(), "0x0000000000000021");
        let Value::Table(levels) = read("auraLevels") else {
            panic!()
        };
        assert_eq!(levels.raw_len(), 48);
        assert_eq!(levels.raw_get::<i64>(2).unwrap(), 1);
        assert_eq!(levels.raw_get::<i64>(4).unwrap(), 3);
        assert!(field_named(UNIT_FIELDS, "nope").is_err());
    }
}
