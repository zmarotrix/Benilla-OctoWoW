//! Smart macros: SuperCleveRoidMacros' conditional macro language built into the client. Its
//! commands replace the stock slash handlers (`/cast`, `/use`, `/target`, …) through the layer
//! (`ClientModAdapters.xml`), each argument compiled once ([`parse`]), its conditions read off the
//! client's own state ([`eval`]) and its action taken natively ([`run`]). The host's macro runner
//! reads [`MacroFlow`]'s flags between lines for `/stopmacro`, `/skipmacro` and `/firstaction`.
//! Deviation: 1.12.1's macros have no conditions.

pub mod eval;
pub mod parse;
pub mod run;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use mlua::{Lua, Value};

/// A macro run's state: the compiled cache, each `/castsequence`'s step, and the flow flags the
/// runner reads between lines.
#[derive(Default)]
pub struct MacroFlow {
    pub cache: RefCell<parse::Cache>,
    pub sequences: RefCell<HashMap<String, run::Sequence>>,
    /// `/stopmacro`: no further line of this run.
    pub stop: Cell<bool>,
    /// `/skipmacro`: no further line of the macro now running (a nested one ends alone).
    pub skip: Cell<bool>,
    /// `/firstaction`: the first `/cast` that acts ends the rest of this run's casts.
    pub first_action: Cell<bool>,
    pub first_done: Cell<bool>,
}

impl MacroFlow {
    /// A run begins: its flags clear.
    pub fn begin(&self) {
        self.stop.set(false);
        self.skip.set(false);
        self.first_action.set(false);
        self.first_done.set(false);
    }
}

fn flow(lua: &Lua) -> Rc<MacroFlow> {
    if let Some(f) = lua.app_data_ref::<Rc<MacroFlow>>() {
        return f.clone();
    }
    let f = Rc::new(MacroFlow::default());
    lua.set_app_data(f.clone());
    f
}

/// The commands the layer routes here, by their slash name.
pub const COMMANDS: &[&str] = &[
    "cast",
    "spell",
    "use",
    "castsequence",
    "castpet",
    "target",
    "startattack",
    "stopattack",
    "stopcasting",
    "stopchanneling",
    "unqueue",
    "cleartarget",
    "cancelaura",
    "unbuff",
    "unshift",
    "stopmacro",
    "skipmacro",
    "firstaction",
    "nofirstaction",
    "petattack",
    "petfollow",
    "petwait",
    "petstay",
    "petpassive",
    "petdefensive",
    "petaggressive",
    "equip",
    "equipmh",
    "equipoh",
    "equip11",
    "equip12",
    "equip13",
    "equip14",
    "applymain",
    "applyoff",
    "retarget",
];

/// The in-game globals [`install`] adds: the surface gates' exceptions.
#[cfg(test)]
pub(crate) const IN_GAME_GLOBALS: &[&str] = &[
    "BenillaSmartMacro_Run",
    "BenillaSmartMacro_Choose",
    "BenillaSmartMacro_Check",
    "BenillaSmartMacro_Begin",
    "BenillaSmartMacro_Rederive",
];

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    let g = lua.globals();
    // `BenillaSmartMacro_Run(cmd, msg[, button])`: run one command; true when it acted.
    g.set(
        "BenillaSmartMacro_Run",
        lua.create_function(
            |lua, (cmd, msg, button): (String, Option<String>, Option<u8>)| {
                let flow = flow(lua);
                if flow.stop.get() || flow.skip.get() {
                    return Ok(false);
                }
                Ok(run::run(
                    lua,
                    &flow,
                    &cmd,
                    msg.as_deref().unwrap_or("").trim(),
                    button,
                ))
            },
        )?,
    )?;
    // `BenillaSmartMacro_Choose(msg)`: the action and unit the argument picks now, for the
    // `#showtooltip` icon; nil when no clause passes.
    g.set(
        "BenillaSmartMacro_Choose",
        lua.create_function(|lua, msg: String| {
            let flow = flow(lua);
            let parsed = flow.cache.borrow_mut().get(msg.trim());
            Ok(match run::choose(lua, &parsed, None) {
                Some(c) => (Some(parse::clean_name(&c.action)), c.unit),
                None => (None, None),
            })
        })?,
    )?;
    // `BenillaSmartMacro_Check(msg)`: the condition names the grammar does not know.
    g.set(
        "BenillaSmartMacro_Check",
        lua.create_function(|lua, msg: String| {
            let parsed = flow(lua).cache.borrow_mut().get(msg.trim());
            Ok(parsed.unknown.join(", "))
        })?,
    )?;
    // `BenillaSmartMacro_Rederive()`: each smart macro's binding derived again against the
    // conditions now; every slot holding one that moved is told, as a slot write tells it.
    g.set(
        "BenillaSmartMacro_Rederive",
        lua.create_function(|lua, ()| {
            let moved = super::macros::derive_bindings(lua, |_, _, body| {
                super::macros::bind_is_smart(body)
            });
            if moved.is_empty() {
                return Ok(0);
            }
            let slots = slots_of(lua, &moved);
            let mut model = lua.app_data_mut::<super::Model>().expect("model app_data");
            for slot in &slots {
                model.raise(
                    "ACTIONBAR_SLOT_CHANGED",
                    vec![super::ScriptValue::Int(i64::from(*slot))],
                );
            }
            model.raise("ACTIONBAR_UPDATE_STATE", Vec::new());
            Ok(slots.len())
        })?,
    )?;
    // `BenillaSmartMacro_Begin()`: a macro run starts.
    g.set(
        "BenillaSmartMacro_Begin",
        lua.create_function(|lua, ()| {
            flow(lua).begin();
            Ok(())
        })?,
    )?;
    let _ = Value::Nil;
    Ok(())
}

/// The spell name a smart `/cast` argument picks now, `None` when no clause passes or it picks a
/// nested macro.
pub(crate) fn choose_name(lua: &Lua, arg: &str) -> Option<String> {
    let parsed = flow(lua).cache.borrow_mut().get(arg.trim());
    let c = run::choose(lua, &parsed, None)?;
    let name = parse::clean_name(&c.action);
    (!name.is_empty() && !name.starts_with('{')).then_some(name)
}

/// Each action slot holding one of `ids`, by its 1-based Lua action id.
fn slots_of(lua: &Lua, ids: &[u32]) -> Vec<u32> {
    let model = lua.app_data_ref::<super::Model>().expect("model app_data");
    model
        .actions
        .iter()
        .filter(|(_, s)| {
            super::action::macro_slot_id(s.kind, s.action).is_some_and(|m| ids.contains(&m))
        })
        .map(|(&slot, _)| slot)
        .collect()
}

impl super::UiScript {
    /// A macro run begins: the flow flags clear ([`MacroFlow::begin`]).
    pub fn begin_macro_run(&self) {
        flow(&self.lua).begin();
    }

    /// Whether the run's remaining lines are to be skipped (`/stopmacro`, `/skipmacro`).
    pub fn macro_run_stopped(&self) -> bool {
        let f = flow(&self.lua);
        f.stop.get() || f.skip.get()
    }
}

#[cfg(test)]
mod tests {
    use crate::script::UiScript;

    /// A VM whose game state is stand-ins: a hostile target at 40% health with Rake from the
    /// player, alt held, and `CastSpellByName` recording what it was asked.
    fn vm() -> UiScript {
        let s = UiScript::new().unwrap();
        s.run(
            r#"
            CAST = {}
            UnitExists = function(u) return u == "target" or u == "player" or u == "mouseover" end
            UnitCanAttack = function(a, b) return b == "target" end
            UnitCanAssist = function(a, b) return b == "mouseover" or b == "player" end
            UnitIsDeadOrGhost = function() return nil end
            UnitHealth = function(u) return u == "target" and 40 or 90 end
            UnitHealthMax = function() return 100 end
            UnitDebuff = function(u, i) if u == "target" and i == 1 then return "tex", 1, nil, 1822 end end
            UnitBuff = function() end
            SpellInfo = function(id) if id == 1822 then return "Rake", "Rank 1" end end
            IsAltKeyDown = function() return 1 end
            IsShiftKeyDown = function() return nil end
            IsControlKeyDown = function() return nil end
            GetSpellIdForName = function(n) return 1 end
            CastSpellByName = function(n, u) table.insert(CAST, n .. "@" .. tostring(u)) end
            "#,
        )
        .unwrap();
        s
    }

    fn casts(s: &UiScript) -> Vec<String> {
        s.eval::<Vec<String>>("return CAST").unwrap()
    }

    #[test]
    fn the_first_passing_clause_casts_at_its_unit() {
        let s = vm();
        s.run(r#"BenillaSmartMacro_Run("cast", "[mod:shift] Frostbolt; [mod:alt] Fire Blast; Blink")"#)
            .unwrap();
        s.run(r#"BenillaSmartMacro_Run("cast", "[@mouseover,help][] Rejuvenation")"#)
            .unwrap();
        s.run(r#"BenillaSmartMacro_Run("cast", "[harm,hp:<50,debuff:Rake] Ferocious Bite")"#)
            .unwrap();
        s.run(r#"BenillaSmartMacro_Run("cast", "[nodebuff:Rake] Rake")"#)
            .unwrap();
        assert_eq!(
            casts(&s),
            [
                "Fire Blast@nil",
                "Rejuvenation@mouseover",
                "Ferocious Bite@nil"
            ]
        );
    }

    #[test]
    fn stopmacro_and_firstaction_set_the_flow() {
        let s = vm();
        s.begin_macro_run();
        s.run(r#"BenillaSmartMacro_Run("firstaction", "")"#)
            .unwrap();
        s.run(r#"BenillaSmartMacro_Run("cast", "A")"#).unwrap();
        s.run(r#"BenillaSmartMacro_Run("cast", "B")"#).unwrap();
        assert_eq!(casts(&s), ["A@nil"]);
        assert!(!s.macro_run_stopped());
        s.run(r#"BenillaSmartMacro_Run("stopmacro", "[mod:alt]")"#)
            .unwrap();
        assert!(s.macro_run_stopped());
        s.begin_macro_run();
        assert!(!s.macro_run_stopped());
    }

    #[test]
    fn choose_and_check_answer_the_icon_and_the_checker() {
        let s = vm();
        let (name, unit): (Option<String>, Option<String>) = s
            .eval(r#"return BenillaSmartMacro_Choose("[@mouseover,help] Renew; Smite")"#)
            .unwrap();
        assert_eq!(
            (name.as_deref(), unit.as_deref()),
            (Some("Renew"), Some("mouseover"))
        );
        let bad: String = s
            .eval(r#"return BenillaSmartMacro_Check("[frobnicate,combat] X")"#)
            .unwrap();
        assert_eq!(bad, "frobnicate");
    }
}
