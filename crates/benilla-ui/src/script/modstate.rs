//! The player's state as OctoWoW's client mods expose it: SuperWoW's `IsMounted`, `IsSwimming`,
//! `isIndoors` and `GetSpeed`, nampower's `PlayerIsMoving`, `PlayerIsRooted` and
//! `PlayerIsSwimming`, ClassicAPI's `IsIndoors`, `IsOutdoors` and `UnitStandState`, each read at
//! the call ([`PlayerStateQuery`]). Deviation: 1.12.1 exposes none of these.

use mlua::{Lua, Value};

/// The player's mover and fields as the reads see them.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PlayerState {
    pub mounted: bool,
    pub swimming: bool,
    pub indoors: bool,
    pub rooted: bool,
    /// Horizontal speed, yards a second.
    pub speed: f32,
    /// `UNIT_FIELD_BYTES_1` byte 0: 0 standing, 1 sitting, 3 asleep, 8 kneeling, …
    pub stand_state: u8,
}

/// The reads' question to the host, `None` with no player in the world.
pub struct PlayerStateQuery;

impl super::live::LiveQuery for PlayerStateQuery {
    type Answer = Option<PlayerState>;
}

/// What a swing was.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SwingKind {
    Hit,
    Crit,
    Miss,
    Dodge,
    Parry,
    Block,
    Glancing,
    Crushing,
}

impl SwingKind {
    /// The name smart macros' `[lastswing:…]` and `[incominghit:…]` take.
    pub fn name(self) -> &'static str {
        match self {
            Self::Hit => "hit",
            Self::Crit => "crit",
            Self::Miss => "miss",
            Self::Dodge => "dodge",
            Self::Parry => "parry",
            Self::Block => "block",
            Self::Glancing => "glancing",
            Self::Crushing => "crushing",
        }
    }
}

/// The player's swing clocks, each the seconds since, and the last swings out and in.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CombatTimers {
    pub since_main: Option<f64>,
    pub since_off: Option<f64>,
    pub since_ranged: Option<f64>,
    pub last_out: Option<(SwingKind, f64)>,
    pub last_in: Option<(SwingKind, f64)>,
}

/// The clocks' question to the host.
pub struct CombatTimersQuery;

impl super::live::LiveQuery for CombatTimersQuery {
    type Answer = CombatTimers;
}

/// `IsKeyDown(key)`'s question: whether the key WoW names `0` is held now.
pub struct KeyDownQuery(pub String);

impl super::live::LiveQuery for KeyDownQuery {
    type Answer = bool;
}

/// One totem-bar slot (1 fire, 2 earth, 3 water, 4 air): the totem's summoning spell, its name
/// and icon, seconds since it went down and its duration (0 for none).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TotemSlot {
    pub spell: u32,
    pub name: String,
    pub icon: String,
    pub elapsed: f64,
    pub duration: f64,
}

/// ClassicAPI's `GetTotemInfo(slot)`: the player's totem in that slot, `None` for an empty one.
pub struct TotemQuery(pub u8);

impl super::live::LiveQuery for TotemQuery {
    type Answer = Option<TotemSlot>;
}

/// The spell-school interrupt lockout on the player (Counterspell, Kick, Pummel, Earth Shock):
/// the locked schools as a mask (`1 << School`) and the seconds left, `(0, 0)` with none.
pub struct SchoolLockoutQuery;

impl super::live::LiveQuery for SchoolLockoutQuery {
    type Answer = (u32, f64);
}

/// ClassicAPI's item-set reads: the set an item belongs to, or a set by id.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemSetLookup {
    OfItem(u32),
    Set(u32),
}

/// An `ItemSet.dbc` row: its id, name, member items and `(spell, required count)` bonuses.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ItemSetRow {
    pub id: u32,
    pub name: String,
    pub items: Vec<u32>,
    pub bonuses: Vec<(u32, u32)>,
}

impl super::live::LiveQuery for ItemSetLookup {
    type Answer = Option<ItemSetRow>;
}

/// The cast or channel a unit is making: its spell, the seconds left and whether it channels;
/// `None` for none.
pub struct UnitCastQuery(pub u64);

impl super::live::LiveQuery for UnitCastQuery {
    type Answer = Option<(u32, f64, bool)>;
}

/// Our last spell's result on a unit: `Some(true)` partly resisted, `Some(false)` fully, `None`
/// for neither.
pub struct ResistQuery(pub u64);

impl super::live::LiveQuery for ResistQuery {
    type Answer = Option<bool>;
}

/// Whether a unit is known immune to a spell (its school and mechanic) or to a school, as our
/// spells' immune misses taught.
pub struct ImmuneQuery {
    pub unit: u64,
    pub spell: Option<u32>,
    pub school: Option<u32>,
}

impl super::live::LiveQuery for ImmuneQuery {
    type Answer = bool;
}

/// How fast a unit's health falls: seconds to 0% and to 20%, `None` while it is not falling.
pub struct UnitTrendQuery(pub u64);

impl super::live::LiveQuery for UnitTrendQuery {
    type Answer = Option<(f64, f64)>;
}

/// SuperWoW's `CursorPosition()`: the world point under the mouse in WoW coordinates, `None` on
/// the sky or over the interface.
pub struct CursorWorldQuery;

impl super::live::LiveQuery for CursorWorldQuery {
    type Answer = Option<[f32; 3]>;
}

/// The run speed a `GetSpeed` percentage reads against, 7 yd/s.
const RUN_SPEED: f32 = 7.0;

/// The globals [`install_in_game`] adds: the surface gates' exceptions.
#[cfg(test)]
pub(crate) const IN_GAME_GLOBALS: &[&str] = &[
    "IsMounted",
    "IsSwimming",
    "isIndoors",
    "IsIndoors",
    "IsOutdoors",
    "GetSpeed",
    "PlayerIsMoving",
    "PlayerIsRooted",
    "PlayerIsSwimming",
    "UnitStandState",
    "IsKeyDown",
    "GetTotemInfo",
    "GetTotemTimeLeft",
    "BenillaSchoolLockout",
    "BenillaItemSet",
    "BenillaUnitCast",
    "BenillaUnitTrend",
    "BenillaUnitResisted",
    "BenillaUnitImmune",
    "CursorPosition",
    "CanLootUnit",
];

fn state(lua: &Lua) -> PlayerState {
    super::live::ask(lua, &PlayerStateQuery)
        .flatten()
        .unwrap_or_default()
}

/// A flag the way the mods return one: 1 or nil.
fn flag(b: bool) -> Value {
    if b {
        Value::Integer(1)
    } else {
        Value::Nil
    }
}

pub(super) fn install_in_game(lua: &Lua) -> mlua::Result<()> {
    let g = lua.globals();
    type Read = fn(&PlayerState) -> bool;
    let reads: [(&str, Read); 8] = [
        ("IsMounted", |s| s.mounted),
        ("IsSwimming", |s| s.swimming),
        ("PlayerIsSwimming", |s| s.swimming),
        ("isIndoors", |s| s.indoors),
        ("IsIndoors", |s| s.indoors),
        ("IsOutdoors", |s| !s.indoors),
        ("PlayerIsMoving", |s| s.speed > 0.01),
        ("PlayerIsRooted", |s| s.rooted),
    ];
    for (name, read) in reads {
        g.set(
            name,
            lua.create_function(move |lua, _: mlua::MultiValue| Ok(flag(read(&state(lua)))))?,
        )?;
    }
    // `GetSpeed()`: the speed in yards a second and as a percentage of the run speed.
    g.set(
        "GetSpeed",
        lua.create_function(|lua, _: mlua::MultiValue| {
            let s = state(lua).speed;
            Ok((f64::from(s), f64::from(s / RUN_SPEED * 100.0)))
        })?,
    )?;
    // `IsKeyDown(key)`: whether a key, by its binding name (`F`, `SPACE`, `LSHIFT`), is held.
    g.set(
        "IsKeyDown",
        lua.create_function(|lua, key: String| {
            let held =
                super::live::ask(lua, &KeyDownQuery(key.to_ascii_uppercase())).unwrap_or(false);
            Ok(flag(held))
        })?,
    )?;
    // ClassicAPI's totem bar: `GetTotemInfo(slot)` returns haveTotem, name, startTime, duration,
    // icon; `GetTotemTimeLeft(slot)` the seconds left (0 for none or no timer).
    g.set(
        "GetTotemInfo",
        lua.create_function(|lua, slot: Option<i64>| {
            let slot = u8::try_from(slot.unwrap_or(0)).unwrap_or(0);
            let now = lua
                .globals()
                .get::<mlua::Function>("GetTime")
                .and_then(|f| f.call::<f64>(()))
                .unwrap_or(0.0);
            Ok(match super::live::ask(lua, &TotemQuery(slot)).flatten() {
                Some(t) => (true, t.name, now - t.elapsed, t.duration, t.icon),
                None => (false, String::new(), 0.0, 0.0, String::new()),
            })
        })?,
    )?;
    g.set(
        "GetTotemTimeLeft",
        lua.create_function(|lua, slot: Option<i64>| {
            let slot = u8::try_from(slot.unwrap_or(0)).unwrap_or(0);
            Ok(super::live::ask(lua, &TotemQuery(slot))
                .flatten()
                .filter(|t| t.duration > 0.0)
                .map_or(0.0, |t| (t.duration - t.elapsed).max(0.0)))
        })?,
    )?;
    // `BenillaSchoolLockout()`: the locked school mask and the seconds left, the adapters'
    // `C_LossOfControl` reads it.
    g.set(
        "BenillaSchoolLockout",
        lua.create_function(|lua, _: mlua::MultiValue| {
            let (mask, left) = super::live::ask(lua, &SchoolLockoutQuery).unwrap_or((0, 0.0));
            Ok((i64::from(mask), left))
        })?,
    )?;
    // `BenillaItemSet(itemId)` or `BenillaItemSet(nil, setId)`: the set's id, name, member item
    // ids and `{spellID, threshold}` bonuses; nil for no set or an uncached item.
    g.set(
        "BenillaItemSet",
        lua.create_function(|lua, (item, set): (Option<i64>, Option<i64>)| {
            let query = match (item, set) {
                (Some(item), _) => ItemSetLookup::OfItem(u32::try_from(item).unwrap_or(0)),
                (None, Some(set)) => ItemSetLookup::Set(u32::try_from(set).unwrap_or(0)),
                (None, None) => return Ok(mlua::MultiValue::new()),
            };
            let Some(row) = super::live::ask(lua, &query).flatten() else {
                return Ok(mlua::MultiValue::new());
            };
            let items = lua.create_sequence_from(row.items.iter().map(|&i| i64::from(i)))?;
            let bonuses = lua.create_table()?;
            for (i, (spell, need)) in row.bonuses.iter().enumerate() {
                let b = lua.create_table()?;
                b.raw_set(1, i64::from(*spell))?;
                b.raw_set(2, i64::from(*need))?;
                bonuses.raw_set(i + 1, b)?;
            }
            Ok(mlua::MultiValue::from_vec(vec![
                Value::Integer(i64::from(row.id)),
                Value::String(lua.create_string(&row.name)?),
                Value::Table(items),
                Value::Table(bonuses),
            ]))
        })?,
    )?;
    // `BenillaUnitCast(unit)`: the unit's cast or channel as spell id, seconds left and whether
    // it channels; nil for none.
    g.set(
        "BenillaUnitCast",
        lua.create_function(|lua, unit: String| {
            let guid = {
                let model = lua.app_data_ref::<super::Model>().expect("model app_data");
                super::unit::guid_of(lua, &model, &unit)?.unwrap_or(0)
            };
            if guid == 0 {
                return Ok((None, None, None));
            }
            Ok(
                match super::live::ask(lua, &UnitCastQuery(guid)).flatten() {
                    Some((spell, left, channel)) => {
                        (Some(i64::from(spell)), Some(left), Some(channel))
                    }
                    None => (None, None, None),
                },
            )
        })?,
    )?;
    // `BenillaUnitTrend(unit)`: seconds until the unit dies and until it reaches 20% at the
    // rate its health is falling; nil while it is not.
    g.set(
        "BenillaUnitTrend",
        lua.create_function(|lua, unit: String| {
            let guid = {
                let model = lua.app_data_ref::<super::Model>().expect("model app_data");
                super::unit::guid_of(lua, &model, &unit)?.unwrap_or(0)
            };
            if guid == 0 {
                return Ok((None, None));
            }
            Ok(
                match super::live::ask(lua, &UnitTrendQuery(guid)).flatten() {
                    Some((ttk, tte)) => (Some(ttk), Some(tte)),
                    None => (None, None),
                },
            )
        })?,
    )?;
    // `BenillaUnitResisted(unit)`: "full" or "partial" when our last spell on the unit was
    // resisted, nil otherwise.
    g.set(
        "BenillaUnitResisted",
        lua.create_function(|lua, unit: String| {
            let guid = {
                let model = lua.app_data_ref::<super::Model>().expect("model app_data");
                super::unit::guid_of(lua, &model, &unit)?.unwrap_or(0)
            };
            if guid == 0 {
                return Ok(None);
            }
            Ok(super::live::ask(lua, &ResistQuery(guid))
                .flatten()
                .map(|partial| if partial { "partial" } else { "full" }))
        })?,
    )?;
    // `BenillaUnitImmune(unit, spellIdOrSchool)`: whether the unit is known immune to the spell
    // (a number) or the school (a name: "fire", "frost", ...).
    g.set(
        "BenillaUnitImmune",
        lua.create_function(|lua, (unit, what): (String, Value)| {
            let guid = {
                let model = lua.app_data_ref::<super::Model>().expect("model app_data");
                super::unit::guid_of(lua, &model, &unit)?.unwrap_or(0)
            };
            if guid == 0 {
                return Ok(false);
            }
            let (spell, school) = match &what {
                Value::Integer(n) => (u32::try_from(*n).ok(), None),
                Value::Number(n) => (Some(*n as u32), None),
                Value::String(s) => {
                    let name = s.to_string_lossy().to_ascii_lowercase();
                    let school = [
                        "physical", "holy", "fire", "nature", "frost", "shadow", "arcane",
                    ]
                    .iter()
                    .position(|k| *k == name);
                    match school {
                        Some(i) => (None, Some(i as u32)),
                        None => return Ok(false),
                    }
                }
                _ => return Ok(false),
            };
            Ok(super::live::ask(
                lua,
                &ImmuneQuery {
                    unit: guid,
                    spell,
                    school,
                },
            )
            .unwrap_or(false))
        })?,
    )?;
    // SuperWoW's `CursorPosition()`: x, y, z of the ground under the mouse, nil on the sky.
    g.set(
        "CursorPosition",
        lua.create_function(|lua, _: mlua::MultiValue| {
            Ok(match super::live::ask(lua, &CursorWorldQuery).flatten() {
                Some([x, y, z]) => (Some(f64::from(x)), Some(f64::from(y)), Some(f64::from(z))),
                None => (None, None, None),
            })
        })?,
    )?;
    // SuperWoW's `CanLootUnit(unit)`: 1 while the unit's corpse has loot for the player
    // (`UNIT_DYNAMIC_FLAGS` lootable).
    g.set(
        "CanLootUnit",
        lua.create_function(|lua, unit: String| {
            let guid = {
                let model = lua.app_data_ref::<super::Model>().expect("model app_data");
                super::unit::guid_of(lua, &model, &unit)?.unwrap_or(0)
            };
            let lootable = guid != 0
                && super::live::ask(lua, &super::UnitFieldsQuery(guid))
                    .flatten()
                    .is_some_and(|w| w.get(143).copied().unwrap_or(0) & 1 != 0);
            Ok(flag(lootable))
        })?,
    )?;
    // `UnitStandState(unit)`: the player's stand state; another unit's is not read here.
    g.set(
        "UnitStandState",
        lua.create_function(|lua, unit: Option<String>| {
            Ok(
                match unit.as_deref().map(str::to_ascii_lowercase).as_deref() {
                    Some("player") | None => Some(i64::from(state(lua).stand_state)),
                    _ => None,
                },
            )
        })?,
    )?;
    Ok(())
}
