//! UnitXP SP3's commands, which OctoWoW's client mods add for pfUI, HealersMate and pfQuest: the
//! stock `UnitXP(unit)` answers a command named in its first argument instead, as the mod's
//! detour of `0x517350` does (`dllmain.cpp`). `nop`, `version`, `distanceBetween` (its five
//! meters, `distanceBetween.cpp`), `inSight` and `behind` (`inSight.cpp`); any other first
//! argument is the stock call. Deviation: 1.12.1 has none of these.

use mlua::{Lua, MultiValue, Value};

/// UnitXP SP3's build, the `coffTimeDateStamp` of the `UnitXP_SP3.dll` OctoWoW ships.
const COFF_TIME_DATE_STAMP: i64 = 1_774_797_347;

/// `OBJECT_FIELD_TYPE`'s player bit, the typemask word at field 2.
const TYPE_PLAYER: u32 = 0x10;
/// `UNIT_FIELD_BOUNDINGRADIUS` and `UNIT_FIELD_COMBATREACH`.
const FIELD_BOUNDING_RADIUS: usize = 129;
const FIELD_COMBAT_REACH: usize = 130;
/// `behind`'s default threshold: more than a right angle off the mob's facing.
const BEHIND_THRESHOLD: f32 = std::f32::consts::FRAC_PI_2;

/// A unit's world position (raw WoW) and facing (radians), for any unit the client holds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UnitBody {
    pub position: [f32; 3],
    pub facing: f32,
}

/// `distanceBetween` and `behind`'s question: the unit's body, `None` for one not held.
pub struct UnitBodyQuery(pub u64);

impl super::live::LiveQuery for UnitBodyQuery {
    type Answer = Option<UnitBody>;
}

/// `inSight`'s question: whether the world leaves the eye-height line between two feet positions
/// open, `None` with no world to test.
pub struct LineOfSightQuery {
    pub from: [f32; 3],
    pub to: [f32; 3],
}

impl super::live::LiveQuery for LineOfSightQuery {
    type Answer = Option<bool>;
}

/// Seconds left on the aura `spell` the player's own cast put on `unit`, `None` when the client
/// timed none ([`super::smartmacro`]'s aura comparisons on other units).
pub struct OwnAuraLeftQuery {
    pub unit: u64,
    pub spell: u32,
}

impl super::live::LiveQuery for OwnAuraLeftQuery {
    type Answer = Option<f64>;
}

/// Every unit the client holds within `0` yards of the player (centre to centre), by guid,
/// nearest first, the player excluded: smart macros' count mode and `/target` scan read it.
pub struct NearbyUnitsQuery(pub f32);

impl super::live::LiveQuery for NearbyUnitsQuery {
    type Answer = Vec<u64>;
}

/// One unit as the meters read it.
struct Measured {
    guid: u64,
    body: UnitBody,
    player: bool,
    combat_reach: f32,
    bounding_radius: f32,
}

fn measured(lua: &Lua, token: &str) -> mlua::Result<Option<Measured>> {
    let guid = {
        let model = lua.app_data_ref::<super::Model>().expect("model app_data");
        super::unit::guid_of(lua, &model, token)?.unwrap_or(0)
    };
    if guid == 0 {
        return Ok(None);
    }
    let Some(body) = super::live::ask(lua, &UnitBodyQuery(guid)).flatten() else {
        return Ok(None);
    };
    let fields = super::live::ask(lua, &super::UnitFieldsQuery(guid))
        .flatten()
        .unwrap_or_default();
    let word = |i: usize| fields.get(i).copied().unwrap_or(0);
    Ok(Some(Measured {
        guid,
        body,
        player: word(2) & TYPE_PLAYER != 0,
        combat_reach: f32::from_bits(word(FIELD_COMBAT_REACH)).max(0.0),
        bounding_radius: f32::from_bits(word(FIELD_BOUNDING_RADIUS)).max(0.0),
    }))
}

fn length(a: [f32; 3], b: [f32; 3]) -> f32 {
    let (x, y, z) = (a[0] - b[0], a[1] - b[1], a[2] - b[2]);
    (x * x + y * y + z * z).sqrt()
}

/// `UnitXP_distanceBetween` (`distanceBetween.cpp:16`): the meter's distance in yards.
fn distance(a: &Measured, b: &Measured, meter: &str) -> f32 {
    if a.guid == b.guid {
        return 0.0;
    }
    let (p0, p1) = (a.body.position, b.body.position);
    match meter {
        // vmangos `Unit::CanReachWithMeleeAutoAttack`: flat distance less both reaches, each at
        // least 1.5, plus 4/3, at least 5 in all; only within 6 yd of height.
        "meleeAutoAttack" if (p0[2] - p1[2]).abs() < 6.0 => {
            let reach = (a.combat_reach.max(1.5) + b.combat_reach.max(1.5) + 1.333_333_4).max(5.0);
            let flat = ((p0[0] - p1[0]).powi(2) + (p0[1] - p1[1]).powi(2)).sqrt();
            (flat - reach).max(0.0)
        }
        // One reach, a creature's: against a player the reach is not counted.
        "AoE" => {
            let mut reach = 0.0;
            if !a.player {
                reach = a.combat_reach;
            }
            if !b.player {
                reach = b.combat_reach;
            }
            (length(p0, p1) - reach).max(0.0)
        }
        "chains" => (length(p0, p1) - a.bounding_radius - b.bounding_radius).max(0.0),
        "Gaussian" => length(p0, p1),
        // The Lua default, and what an unknown meter or a far melee falls through to.
        _ if meter == "meleeAutoAttack" => length(p0, p1),
        _ => (length(p0, p1) - a.combat_reach - b.combat_reach).max(0.0),
    }
}

/// `UnitXP_behind` (`inSight.cpp:130`): whether `me` stands more than the threshold off `mob`'s
/// facing. Deviation: the mod reads a fighting, standing mob's facing off its target, as the
/// server stops updating it; benilla reads the facing it holds.
fn behind(me: &Measured, mob: &Measured) -> Option<bool> {
    if me.guid == mob.guid {
        return None;
    }
    let forward = (mob.body.facing.cos(), mob.body.facing.sin());
    let check = (
        me.body.position[0] - mob.body.position[0],
        me.body.position[1] - mob.body.position[1],
    );
    let norm = (check.0 * check.0 + check.1 * check.1).sqrt();
    if norm == 0.0 {
        return None;
    }
    let cos = ((forward.0 * check.0 + forward.1 * check.1) / norm).clamp(-1.0, 1.0);
    Some(cos.acos() >= BEHIND_THRESHOLD)
}

/// How far UnitXP SP3's targeting looks, yards.
const TARGET_RANGE: f32 = 41.0;

/// A global called with `args`, `Nil` where it is missing or raised.
fn global(lua: &Lua, name: &str, args: impl mlua::IntoLuaMulti) -> Value {
    lua.globals()
        .get::<mlua::Function>(name)
        .and_then(|f| f.call::<Value>(args))
        .unwrap_or(Value::Nil)
}

fn truthy(v: &Value) -> bool {
    !matches!(v, Value::Nil | Value::Boolean(false))
}

/// UnitXP SP3's targeting (`UnitXP("target", mode)`): the living enemies within reach and in
/// sight, nearest first, and the one `mode` picks, selected; whether one was.
fn target(lua: &Lua, mode: &str) -> mlua::Result<bool> {
    let enemies: Vec<String> = super::live::ask(lua, &NearbyUnitsQuery(TARGET_RANGE))
        .unwrap_or_default()
        .into_iter()
        .map(super::superwow::guid_text)
        .filter(|g| {
            truthy(&global(lua, "UnitCanAttack", ("player", g.as_str())))
                && !truthy(&global(lua, "UnitIsDead", g.as_str()))
                && !matches!(
                    global(lua, "UnitXP", ("inSight", "player", g.as_str())),
                    Value::Boolean(false)
                )
        })
        .collect();
    let health = |g: &str| match global(lua, "UnitHealth", g) {
        Value::Integer(n) => n as f64,
        Value::Number(n) => n,
        _ => 0.0,
    };
    let current = match global(lua, "GetUnitGUID", "target") {
        Value::String(s) => s.to_str().ok().map(|s| s.to_string()),
        _ => None,
    };
    let at = current
        .as_ref()
        .and_then(|c| enemies.iter().position(|g| g == c));
    let pick = match mode {
        "nearestEnemy" => enemies.first().cloned(),
        "mostHP" => enemies
            .iter()
            .max_by(|a, b| health(a).total_cmp(&health(b)))
            .cloned(),
        "worldBoss" => enemies
            .iter()
            .find(|g| {
                matches!(global(lua, "UnitClassification", g.as_str()),
                    Value::String(s) if s.to_str().is_ok_and(|s| s == "worldboss"))
            })
            .cloned(),
        "nextEnemyInCycle" | "nextEnemyConsideringDistance" if !enemies.is_empty() => {
            Some(enemies[at.map_or(0, |i| (i + 1) % enemies.len())].clone())
        }
        "previousEnemyInCycle" if !enemies.is_empty() => {
            let n = enemies.len();
            Some(enemies[at.map_or(n - 1, |i| (i + n - 1) % n)].clone())
        }
        _ => None,
    };
    Ok(match pick {
        Some(g) => {
            global(lua, "TargetUnit", g.as_str());
            true
        }
        None => false,
    })
}

fn text(v: Option<&Value>) -> Option<String> {
    match v? {
        Value::String(s) => s.to_str().ok().map(|s| s.to_string()),
        _ => None,
    }
}

/// The mod's answer for `args`, `None` when the first argument names no command it handles.
fn command(lua: &Lua, args: &MultiValue) -> mlua::Result<Option<Value>> {
    if args.len() < 2 {
        return Ok(None);
    }
    let Some(cmd) = text(args.front()) else {
        return Ok(None);
    };
    let arg = |i: usize| text(args.get(i));
    let boolean = |b: Option<bool>| b.map_or(Value::Nil, Value::Boolean);
    Ok(Some(match cmd.as_str() {
        "nop" => Value::Boolean(true),
        "version" => match arg(1).as_deref() {
            Some("coffTimeDateStamp") => Value::Integer(COFF_TIME_DATE_STAMP),
            Some("additionalInformation") => {
                Value::String(lua.create_string("Konaka-main (built into Benilla)")?)
            }
            _ => Value::Nil,
        },
        "distanceBetween" if args.len() >= 3 => {
            let (Some(u0), Some(u1)) = (arg(1), arg(2)) else {
                return Ok(Some(Value::Nil));
            };
            let meter = arg(3).unwrap_or_default();
            match (measured(lua, &u0)?, measured(lua, &u1)?) {
                (Some(a), Some(b)) => Value::Number(f64::from(distance(&a, &b, &meter))),
                _ => Value::Nil,
            }
        }
        "inSight" if args.len() >= 3 => {
            let (Some(u0), Some(u1)) = (arg(1), arg(2)) else {
                return Ok(Some(Value::Nil));
            };
            match (measured(lua, &u0)?, measured(lua, &u1)?) {
                (Some(a), Some(b)) if a.guid == b.guid => Value::Boolean(true),
                (Some(a), Some(b)) => boolean(
                    super::live::ask(
                        lua,
                        &LineOfSightQuery {
                            from: a.body.position,
                            to: b.body.position,
                        },
                    )
                    .flatten(),
                ),
                _ => Value::Nil,
            }
        }
        "target" => Value::Boolean(target(lua, &arg(1).unwrap_or_default())?),
        "behind" if args.len() >= 3 => {
            let (Some(u0), Some(u1)) = (arg(1), arg(2)) else {
                return Ok(Some(Value::Nil));
            };
            match (measured(lua, &u0)?, measured(lua, &u1)?) {
                (Some(me), Some(mob)) => boolean(behind(&me, &mob)),
                _ => Value::Nil,
            }
        }
        _ => return Ok(None),
    }))
}

/// Wrap the stock `UnitXP`: a command answers itself, anything else is the stock call.
pub(super) fn install_in_game(lua: &Lua) -> mlua::Result<()> {
    let g = lua.globals();
    let stock: mlua::Function = g.get("UnitXP")?;
    g.set(
        "UnitXP",
        lua.create_function(move |lua, args: MultiValue| match command(lua, &args)? {
            Some(answer) => Ok(MultiValue::from_vec(vec![answer])),
            None => stock.call::<MultiValue>(args),
        })?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit(guid: u64, at: [f32; 3], facing: f32, player: bool, reach: f32) -> Measured {
        Measured {
            guid,
            body: UnitBody {
                position: at,
                facing,
            },
            player,
            combat_reach: reach,
            bounding_radius: 0.5,
        }
    }

    #[test]
    fn the_meters_follow_the_mod() {
        let me = unit(1, [0.0, 0.0, 0.0], 0.0, true, 1.5);
        let mob = unit(2, [20.0, 0.0, 0.0], 0.0, false, 2.0);
        assert_eq!(
            distance(&me, &mob, ""),
            20.0 - 1.5 - 2.0,
            "ranged, the Lua default"
        );
        assert_eq!(distance(&me, &mob, "Gaussian"), 20.0);
        assert_eq!(
            distance(&me, &mob, "AoE"),
            18.0,
            "the creature's reach alone"
        );
        assert_eq!(distance(&me, &mob, "chains"), 19.0);
        // 1.5 + 2 + 4/3 is under the 5-yard floor.
        assert_eq!(distance(&me, &mob, "meleeAutoAttack"), 15.0);
        assert_eq!(distance(&me, &me, ""), 0.0);
    }

    #[test]
    fn behind_is_more_than_a_right_angle_off_the_mobs_facing() {
        let mob = unit(2, [0.0, 0.0, 0.0], 0.0, false, 1.0); // facing +X
        let front = unit(1, [5.0, 0.0, 0.0], 0.0, true, 1.0);
        let back = unit(1, [-5.0, 0.5, 0.0], 0.0, true, 1.0);
        assert_eq!(behind(&front, &mob), Some(false));
        assert_eq!(behind(&back, &mob), Some(true));
    }

    #[test]
    fn commands_answer_and_a_unit_falls_through_to_the_stock_call() {
        let s = crate::script::UiScript::new().unwrap();
        assert!(s.eval::<bool>(r#"return UnitXP("nop", "nop")"#).unwrap());
        assert_eq!(
            s.eval::<i64>(r#"return UnitXP("version", "coffTimeDateStamp")"#)
                .unwrap(),
            COFF_TIME_DATE_STAMP
        );
        assert!(s
            .eval::<bool>(r#"return UnitXP("distanceBetween", "player", "target") == nil"#)
            .unwrap());
        assert_eq!(s.eval::<i64>(r#"return UnitXP("player")"#).unwrap(), 0);
    }
}
