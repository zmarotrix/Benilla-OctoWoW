//! SuperWoW's script surface, which OctoWoW's client mods add to 1.12 and its interface and
//! addons read: the version globals, the sandboxed text files `ImportFile` and `ExportFile`, and
//! the guid text the unit getters hand out. Deviation: none of this is in 1.12.1; it is here for
//! the OctoWoW client, whose `AutoLogin` glue and third-party addons test for it.
//!
//! The rest of SuperWoW's surface lives with the stock verb it extends (the guid and `markN`
//! tokens in [`super::unit::resolve`], `UnitExists`'s second return beside it).

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use mlua::{Lua, Value};

use super::binding_abi;

/// The SuperWoW release this surface follows (its `SUPERWOW_VERSION`).
pub const SUPERWOW_VERSION: &str = "2.2";

/// The globals [`install`] adds in both states, by type: the surface gates' exceptions, so a
/// global past 1.12 that is not SuperWoW's still fails them.
#[cfg(test)]
pub(crate) const SHARED_GLOBALS: &[(&str, &str)] = &[
    ("SUPERWOW_VERSION", "string"),
    ("SUPERWOW_STRING", "string"),
    ("SpellInfo", "function"),
    ("ImportFile", "function"),
    ("ExportFile", "function"),
];

/// SuperWoW's in-game events, after the stock table's names ([`super::glue::in_game_events`]):
/// cast starts, launches, interruptions, channels and swings of every unit.
pub const IN_GAME_EVENTS: &[&str] = &["UNIT_CASTEVENT", "RAW_COMBATLOG"];

/// The globals [`install_in_game`] adds to the in-game state alone.
#[cfg(test)]
pub(crate) const IN_GAME_GLOBALS: &[&str] = &[
    "SetMouseoverUnit",
    "SetAutoloot",
    "GetPlayerBuffID",
    "UnitPosition",
    "Clickthrough",
    "CombatLogAdd",
    "TrackUnit",
    "UntrackUnit",
];

static IMPORTS_DIR: OnceLock<PathBuf> = OnceLock::new();

/// The one folder `ImportFile` reads and `ExportFile` writes, SuperWoW's `<game>\imports`. Set once
/// at startup; later calls are ignored. Without it both answer nil.
pub fn set_imports_dir(dir: impl Into<PathBuf>) {
    let _ = IMPORTS_DIR.set(dir.into());
}

/// A unit's guid as SuperWoW writes it: `0x` and 16 upper-case hex digits, the text its guid
/// tokens take back.
pub fn guid_text(guid: u64) -> String {
    format!("0x{guid:016X}")
}

/// `name` as a file in the imports folder: a bare file name, `.txt` added when it has no
/// extension, so no name reaches outside the folder.
fn import_path(dir: &Path, name: &str) -> Option<PathBuf> {
    let name = name.trim();
    let bare = !name.is_empty()
        && !name.contains(['/', '\\', ':'])
        && name != "."
        && name != ".."
        && !name.chars().any(char::is_control);
    if !bare {
        return None;
    }
    let mut path = dir.join(name);
    if path.extension().is_none() {
        path.set_extension("txt");
    }
    Some(path)
}

/// `SpellInfo(id)`'s question: any `Spell.dbc` row, known or not.
pub struct SpellInfoQuery(pub u32);

/// A spell's `SpellInfo` facts; `rank` is `""` for a rankless spell, as `GetSpellName`'s is.
#[derive(Clone, Debug, PartialEq)]
pub struct SpellInfoAnswer {
    pub name: String,
    pub rank: String,
    pub icon: Option<String>,
    /// `SpellRange.dbc`'s pair for the row's range index, in yards.
    pub min_range: f32,
    pub max_range: f32,
}

impl super::live::LiveQuery for SpellInfoQuery {
    /// `None` for an id with no row.
    type Answer = Option<SpellInfoAnswer>;
}

/// `UnitPosition(unit)`'s question: the unit's world position, raw WoW `[x, y, z]`, for a
/// friendly unit alone (the player, or a party or raid member the client holds), as SuperWoW
/// limits it.
pub struct UnitPositionQuery(pub u64);

impl super::live::LiveQuery for UnitPositionQuery {
    /// `None` for a unit not held or not friendly.
    type Answer = Option<[f32; 3]>;
}

/// The globals, in the glue VM and the in-game one alike (SuperWoW 1.3 added the file pair to
/// both).
pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    let g = lua.globals();
    g.set("SUPERWOW_VERSION", SUPERWOW_VERSION)?;
    g.set("SUPERWOW_STRING", format!("SuperWoW {SUPERWOW_VERSION}"))?;

    // `SpellInfo(spellId)`: name, rank, icon, min range, max range; nil for an unknown id.
    g.set(
        "SpellInfo",
        lua.create_function(|lua, id: Value| {
            let id = binding_abi::number_arg(lua, id, "Usage: SpellInfo(spellId)")?;
            let Ok(id) = u32::try_from(id) else {
                return Ok(mlua::MultiValue::from_vec(vec![Value::Nil]));
            };
            let Some(Some(info)) = super::live::ask(lua, &SpellInfoQuery(id)) else {
                return Ok(mlua::MultiValue::from_vec(vec![Value::Nil]));
            };
            Ok(mlua::MultiValue::from_vec(vec![
                Value::String(lua.create_string(&info.name)?),
                Value::String(lua.create_string(&info.rank)?),
                match info.icon {
                    Some(icon) => Value::String(lua.create_string(&icon)?),
                    None => Value::Nil,
                },
                Value::Number(f64::from(info.min_range)),
                Value::Number(f64::from(info.max_range)),
            ]))
        })?,
    )?;

    // `ImportFile("filename")`: the file's text, nil when it is missing or unreadable.
    g.set(
        "ImportFile",
        lua.create_function(|lua, name: Value| {
            let name = binding_abi::string_arg(lua, name, r#"Usage: ImportFile("filename")"#)?;
            let text = IMPORTS_DIR
                .get()
                .and_then(|dir| import_path(dir, &name))
                .and_then(|path| std::fs::read(path).ok());
            Ok(match text {
                Some(bytes) => Value::String(lua.create_string(bytes)?),
                None => Value::Nil,
            })
        })?,
    )?;

    // `ExportFile("filename", "text")`: writes the text whole, creating the folder; 1 on success.
    g.set(
        "ExportFile",
        lua.create_function(|lua, (name, text): (Value, Value)| {
            const USAGE: &str = r#"Usage: ExportFile("filename","text")"#;
            let name = binding_abi::string_arg(lua, name, USAGE)?;
            let text = match text {
                Value::String(s) => s.as_bytes().to_vec(),
                other => binding_abi::string_arg(lua, other, USAGE)?.into_bytes(),
            };
            let written = IMPORTS_DIR.get().and_then(|dir| {
                let path = import_path(dir, &name)?;
                std::fs::create_dir_all(dir).ok()?;
                std::fs::write(path, text).ok()
            });
            Ok(binding_abi::flag(written.is_some()))
        })?,
    )?;
    Ok(())
}

/// `Clickthrough(0 | 1)` and `Clickthrough()`: benilla's `clickThrough` CVar
/// (`crate::target::clicks_through` in the app), the same way.
///
/// `SetAutoloot(0 | 1)` and `SetAutoloot()`: benilla's `autoLootDefault` CVar, through
/// `SetCVar` and `GetCVar` so its callback and persistence run as a settings click's do.
const CVAR_WRAPPERS: &str = r#"
function Clickthrough(on)
    if on == nil then
        return tonumber(GetCVar("clickThrough")) or 0
    end
    local enable = on and on ~= 0 and on ~= "0"
    SetCVar("clickThrough", enable and "1" or "0")
end

function SetAutoloot(on)
    if on == nil then
        return tonumber(GetCVar("autoLootDefault")) or 0
    end
    local enable = on and on ~= 0 and on ~= "0"
    SetCVar("autoLootDefault", enable and "1" or "0")
end
"#;

/// The in-game globals: the ones that name units or the player's settings.
pub(super) fn install_in_game(lua: &Lua) -> mlua::Result<()> {
    let g = lua.globals();

    // `SetMouseoverUnit(unit)` names the unit `mouseover` resolves to, over the world pick, until
    // `SetMouseoverUnit()` clears it: a unit frame's enter and leave.
    g.set(
        "SetMouseoverUnit",
        lua.create_function(|lua, unit: Option<Value>| {
            let guid = match unit {
                None | Some(Value::Nil) => 0,
                Some(v) => {
                    let token = binding_abi::string_arg(lua, v, "Usage: SetMouseoverUnit([unit])")?;
                    let model = lua.app_data_ref::<super::Model>().expect("model app_data");
                    super::unit::guid_of(lua, &model, &token)?.unwrap_or(0)
                }
            };
            lua.app_data_mut::<super::Model>()
                .expect("model app_data")
                .unit_guids
                .script_mouseover = guid;
            Ok(())
        })?,
    )?;

    // `UnitPosition(unit)`: a friendly unit's world x, y, z; nil for anyone else.
    g.set(
        "UnitPosition",
        lua.create_function(|lua, unit: Value| {
            let token = binding_abi::string_arg(lua, unit, "Usage: UnitPosition(unit)")?;
            let guid = {
                let model = lua.app_data_ref::<super::Model>().expect("model app_data");
                super::unit::guid_of(lua, &model, &token)?.unwrap_or(0)
            };
            let at = (guid != 0)
                .then(|| super::live::ask(lua, &UnitPositionQuery(guid)).flatten())
                .flatten();
            Ok(match at {
                Some([x, y, z]) => mlua::MultiValue::from_vec(vec![
                    Value::Number(f64::from(x)),
                    Value::Number(f64::from(y)),
                    Value::Number(f64::from(z)),
                ]),
                None => mlua::MultiValue::from_vec(vec![Value::Nil]),
            })
        })?,
    )?;

    // `CombatLogAdd("text" [, raw])`: a line of the addon's own into the combat log file, or the
    // raw one.
    g.set(
        "CombatLogAdd",
        lua.create_function(|lua, (text, raw): (Value, Option<Value>)| {
            let text =
                binding_abi::string_arg(lua, text, r#"Usage: CombatLogAdd("text" [, raw])"#)?;
            let raw = binding_abi::bool_or_default(raw.as_ref(), false);
            super::calls::submit(lua, super::ScriptCall::CombatLogAdd { text, raw })
        })?,
    )?;

    // `TrackUnit(unit)`: a friendly unit onto the minimap, drawn as a group member is.
    g.set(
        "TrackUnit",
        lua.create_function(|lua, unit: Value| {
            let token = binding_abi::string_arg(lua, unit, "Usage: TrackUnit(unit)")?;
            let friendly = lua
                .globals()
                .get::<mlua::Function>("UnitIsFriend")?
                .call::<Value>(("player", token.as_str()))?;
            if matches!(friendly, Value::Nil | Value::Boolean(false)) {
                return Ok(());
            }
            let guid = {
                let model = lua.app_data_ref::<super::Model>().expect("model app_data");
                super::unit::guid_of(lua, &model, &token)?.unwrap_or(0)
            };
            let mut model = lua.app_data_mut::<super::Model>().expect("model app_data");
            if guid != 0 && !model.tracked_units.contains(&guid) {
                model.tracked_units.push(guid);
            }
            Ok(())
        })?,
    )?;

    // `UntrackUnit(unit)` takes one off; `UntrackUnit("all")` empties the list.
    g.set(
        "UntrackUnit",
        lua.create_function(|lua, unit: Value| {
            let token = binding_abi::string_arg(lua, unit, "Usage: UntrackUnit(unit)")?;
            if token.eq_ignore_ascii_case("all") {
                lua.app_data_mut::<super::Model>()
                    .expect("model app_data")
                    .tracked_units
                    .clear();
                return Ok(());
            }
            let guid = {
                let model = lua.app_data_ref::<super::Model>().expect("model app_data");
                super::unit::guid_of(lua, &model, &token)?.unwrap_or(0)
            };
            lua.app_data_mut::<super::Model>()
                .expect("model app_data")
                .tracked_units
                .retain(|g| *g != guid);
            Ok(())
        })?,
    )?;

    lua.load(CVAR_WRAPPERS).set_name("SuperWoW").exec()
}

impl super::UiScript {
    /// SuperWoW's `TrackUnit` list, for the minimap.
    pub fn tracked_units(&self) -> Vec<u64> {
        self.model_ref().tracked_units.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_guid_reads_as_superwow_writes_it() {
        assert_eq!(guid_text(0xF130_0012_3400_ABCD), "0xF13000123400ABCD");
        assert_eq!(guid_text(5), "0x0000000000000005");
    }

    /// The pair round-trips through the folder, in the glue VM (where `AutoLogin` runs) and the
    /// in-game one, and the version globals are there for the mod checks.
    #[test]
    fn export_then_import_round_trips_in_both_vms() {
        let dir = std::env::temp_dir().join(format!("benilla_imports_{}", std::process::id()));
        set_imports_dir(&dir);
        let glue = crate::script::UiScript::new_glue().unwrap();
        let read = glue
            .eval::<String>(r#"ExportFile("acct", "a;b") return ImportFile("acct")"#)
            .unwrap();
        assert_eq!(read, "a;b");
        assert!(dir.join("acct.txt").is_file());
        let game = crate::script::UiScript::new().unwrap();
        assert_eq!(
            game.eval::<String>(r#"return ImportFile("acct")"#).unwrap(),
            "a;b"
        );
        assert!(game
            .eval::<bool>(r#"return ImportFile("missing") == nil"#)
            .unwrap());
        assert!(game
            .eval::<bool>(r#"return ExportFile("../x", "y") == nil"#)
            .unwrap());
        assert_eq!(
            game.eval::<String>("return SUPERWOW_VERSION").unwrap(),
            SUPERWOW_VERSION
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `SetMouseoverUnit` names the unit `mouseover` resolves to over the pick, kept across a
    /// pushed snapshot, until a bare call clears it; `SetAutoloot` reads and writes the CVar.
    #[test]
    fn the_mouseover_override_and_autoloot() {
        use crate::script::UnitGuids;
        use std::collections::HashMap;
        let guids = UnitGuids {
            player: 0x10,
            party: [0x21, 0, 0, 0],
            held: HashMap::from([(0x10, 0), (0x21, 0)]),
            ..Default::default()
        };
        let mut s = crate::script::UiScript::new().unwrap();
        s.set_unit_guids(&guids);
        let names_party1 = |s: &crate::script::UiScript| {
            s.model_ref().unit_guids.guid_of("mouseover") == Some(0x21)
        };
        assert!(!names_party1(&s));
        s.run(r#"SetMouseoverUnit("party1")"#).unwrap();
        assert!(names_party1(&s));
        s.set_unit_guids(&guids);
        assert!(names_party1(&s), "a pushed snapshot keeps the override");
        s.run("SetMouseoverUnit()").unwrap();
        assert!(!names_party1(&s));

        s.register_cvars([("autoLootDefault", "0")]);
        s.run("SetAutoloot(1)").unwrap();
        assert_eq!(s.eval::<i64>("return SetAutoloot()").unwrap(), 1);
        s.run("SetAutoloot(0)").unwrap();
        assert_eq!(
            s.eval::<String>(r#"return GetCVar("autoLootDefault")"#)
                .unwrap(),
            "0"
        );
    }

    /// `TrackUnit` keeps a friendly unit once, `UntrackUnit` drops one or all; `CombatLogAdd`
    /// hands its line to the host; `Clickthrough` reads and writes its CVar.
    #[test]
    fn tracking_the_log_line_and_click_through() {
        use crate::script::{ScriptCall, UnitGuids, UnitState};
        let mut s = crate::script::UiScript::new().unwrap();
        s.keep_calls();
        s.set_unit_guids(&UnitGuids {
            player: 0x10,
            party: [0x21, 0x22, 0, 0],
            ..Default::default()
        });
        let friend = |guid| UnitState {
            guid,
            exists: true,
            ..Default::default()
        };
        s.set_unit("player", Some(friend(0x10)));
        s.set_unit("party1", Some(friend(0x21)));
        s.set_unit("party2", Some(friend(0x22)));
        s.run(r#"UnitIsFriend = function() return 1 end"#).unwrap();
        s.run(r#"TrackUnit("party1") TrackUnit("party1") TrackUnit("party2")"#)
            .unwrap();
        assert_eq!(s.tracked_units(), vec![0x21, 0x22]);
        s.run(r#"UntrackUnit("party1")"#).unwrap();
        assert_eq!(s.tracked_units(), vec![0x22]);
        s.run(r#"UntrackUnit("all")"#).unwrap();
        assert!(s.tracked_units().is_empty());
        s.run(r#"UnitIsFriend = function() return nil end TrackUnit("party1")"#)
            .unwrap();
        assert!(
            s.tracked_units().is_empty(),
            "an unfriendly unit is not tracked"
        );

        s.run(r#"CombatLogAdd("hello", 1)"#).unwrap();
        assert!(s.take_script_calls().iter().any(|c| matches!(
            c,
            ScriptCall::CombatLogAdd { text, raw: true } if text == "hello"
        )));

        s.register_cvars([("clickThrough", "0")]);
        s.run("Clickthrough(1)").unwrap();
        assert_eq!(s.eval::<i64>("return Clickthrough()").unwrap(), 1);
    }

    #[test]
    fn an_import_name_never_leaves_the_folder() {
        let dir = Path::new("imports");
        assert_eq!(import_path(dir, "accounts"), Some(dir.join("accounts.txt")));
        assert_eq!(import_path(dir, "data.lua"), Some(dir.join("data.lua")));
        for bad in ["", "..", "../secret", "a/b", "a\\b", "C:x", "x\0y"] {
            assert_eq!(import_path(dir, bad), None, "{bad:?}");
        }
    }
}
