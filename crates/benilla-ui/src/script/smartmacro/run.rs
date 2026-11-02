//! Smart macro commands: each takes its argument's first clause whose groups pass ([`super::eval`])
//! and acts on that clause's action, at the passing group's `@unit`. A clause with no groups
//! always passes. The flow commands (`/stopmacro`, `/skipmacro`, `/firstaction`) set the flags the
//! host's macro runner reads between lines ([`super::MacroFlow`]).

use mlua::{IntoLuaMulti, Lua, Value};

use super::eval::Ctx;
use super::parse::{Clause, Parsed};
use super::MacroFlow;

/// A clause chosen to act on: its action and the unit its passing group names.
pub struct Chosen {
    pub action: String,
    pub unit: Option<String>,
    pub no_toggle: bool,
    pub cancel_aura: bool,
    /// The passing group carries `[cursor]`: a ground spell goes down at the mouse.
    pub at_cursor: bool,
}

/// The first clause of `parsed` that passes, with its unit.
pub fn choose(lua: &Lua, parsed: &Parsed, button: Option<u8>) -> Option<Chosen> {
    for clause in &parsed.clauses {
        let ctx = Ctx {
            lua,
            action: &clause.action,
            button,
        };
        if let Some((unit, at_cursor)) = passing_unit(&ctx, clause) {
            return Some(Chosen {
                action: clause.action.clone(),
                unit,
                no_toggle: clause.no_toggle,
                cancel_aura: clause.cancel_aura,
                at_cursor,
            });
        }
    }
    None
}

/// `Some((unit, at_cursor))` when the clause passes: the passing group's `@unit`, or the enemy
/// its `[multiscan:...]` picked, and whether that group carries `[cursor]`.
fn passing_unit(ctx: &Ctx, clause: &Clause) -> Option<(Option<String>, bool)> {
    if clause.groups.is_empty() {
        return Some((None, false));
    }
    for g in &clause.groups {
        let at_cursor = g.conds.iter().any(|c| c.name == "cursor" && !c.negated);
        if let Some(scan) = g.conds.iter().find(|c| c.name == "multiscan") {
            if let Some(unit) = multiscan(ctx, g, scan) {
                return Some((Some(unit), at_cursor));
            }
            continue;
        }
        if ctx.group_passes(g) {
            return Some((g.unit.clone(), at_cursor));
        }
    }
    None
}

/// `[multiscan:priority]`: the enemies in combat with the player (and the target, which needs
/// none), each tried as the group's unit, the first by priority that passes the group's other
/// conditions. Raid marks pick the marked unit.
fn multiscan(ctx: &Ctx, group: &super::parse::Group, scan: &super::parse::Cond) -> Option<String> {
    let priority = scan
        .args
        .first()
        .map(|a| a.name().to_ascii_lowercase())
        .unwrap_or_else(|| "nearest".into());
    let target = text(call(ctx.lua, "GetUnitGUID", "target"));
    let mut candidates: Vec<String> = ctx
        .enemies(45.0)
        .into_iter()
        .filter(|u| {
            Some(u) == target.as_ref() || truthy(call(ctx.lua, "UnitAffectingCombat", u.as_str()))
        })
        .collect();
    let hp = |u: &str| {
        let cur = num(call(ctx.lua, "UnitHealth", u));
        let max = num(call(ctx.lua, "UnitHealthMax", u)).max(1.0);
        (cur, cur / max)
    };
    const MARKS: [&str; 8] = [
        "star", "circle", "diamond", "triangle", "moon", "square", "cross", "skull",
    ];
    let mark_of = |u: &str| num(call(ctx.lua, "GetRaidTargetIndex", u)) as usize;
    match priority.as_str() {
        "farthest" => candidates.reverse(),
        "highesthp" => candidates.sort_by(|a, b| hp(b).1.total_cmp(&hp(a).1)),
        "lowesthp" => candidates.sort_by(|a, b| hp(a).1.total_cmp(&hp(b).1)),
        "highestrawhp" => candidates.sort_by(|a, b| hp(b).0.total_cmp(&hp(a).0)),
        "lowestrawhp" => candidates.sort_by(|a, b| hp(a).0.total_cmp(&hp(b).0)),
        "markorder" => {
            candidates.retain(|u| mark_of(u) != 0);
            candidates.sort_by_key(|u| std::cmp::Reverse(mark_of(u)));
        }
        mark => {
            if let Some(i) = MARKS.iter().position(|m| *m == mark) {
                candidates.retain(|u| mark_of(u) == i + 1);
            }
        }
    }
    candidates.into_iter().find(|u| {
        let mut probe = group.clone();
        probe.unit = Some(u.clone());
        probe.conds.retain(|c| c.name != "multiscan");
        ctx.group_passes(&probe)
    })
}

fn call(lua: &Lua, name: &str, args: impl IntoLuaMulti) -> Option<Value> {
    let f: mlua::Function = lua.globals().get(name).ok()?;
    f.call::<Value>(args).ok()
}

fn truthy(v: Option<Value>) -> bool {
    !matches!(v, None | Some(Value::Nil) | Some(Value::Boolean(false)))
}

fn num(v: Option<Value>) -> f64 {
    match v {
        Some(Value::Integer(n)) => n as f64,
        Some(Value::Number(n)) => n,
        _ => 0.0,
    }
}

fn text(v: Option<Value>) -> Option<String> {
    match v {
        Some(Value::String(s)) => s.to_str().ok().map(|s| s.to_string()),
        _ => None,
    }
}

fn clean(s: &str) -> String {
    super::parse::clean_name(s)
}

/// A `{MacroName}` action: the named macro's body, run line by line.
fn nested_macro(lua: &Lua, action: &str) -> bool {
    let Some(name) = action.strip_prefix('{').and_then(|a| a.strip_suffix('}')) else {
        return false;
    };
    let _ = call(lua, "BenillaSmartMacro_RunByName", name.trim());
    true
}

/// The player's buff slot named `name`, as `GetPlayerBuff` numbers it, `None` when not up.
fn own_buff(lua: &Lua, name: &str) -> Option<i64> {
    for i in 0..32 {
        let index = num(call(lua, "GetPlayerBuff", (i, "HELPFUL")));
        if index < 0.0 {
            break;
        }
        let spell = num(call(lua, "GetPlayerBuffID", index as i64)) as i64;
        let named = text(call(lua, "SpellInfo", spell)).unwrap_or_default();
        if named.eq_ignore_ascii_case(name) {
            return Some(index as i64);
        }
    }
    None
}

/// An item's place: `Worn(slot)` or `Bag(bag, slot)`, by name, id or inventory slot number.
enum Place {
    Worn(i64),
    Bag(i64, i64),
}

fn find_item(lua: &Lua, what: &str) -> Option<Place> {
    let what = clean(what);
    if let Ok(slot) = what.parse::<i64>() {
        if (1..=19).contains(&slot) {
            return Some(Place::Worn(slot));
        }
    }
    let id = what.parse::<u32>().ok();
    let matches = |link: Option<String>| {
        link.is_some_and(|l| match id {
            Some(id) => l.contains(&format!("item:{id}:")),
            None => l
                .to_ascii_lowercase()
                .contains(&format!("[{}]", what.to_ascii_lowercase())),
        })
    };
    for bag in 0..=4 {
        let slots = num(call(lua, "GetContainerNumSlots", bag)) as i64;
        for slot in 1..=slots {
            if matches(text(call(lua, "GetContainerItemLink", (bag, slot)))) {
                return Some(Place::Bag(bag, slot));
            }
        }
    }
    (1..=19)
        .find(|&slot| matches(text(call(lua, "GetInventoryItemLink", ("player", slot)))))
        .map(Place::Worn)
}

/// Use an item where it is, on `unit` when it targets.
fn use_item(lua: &Lua, what: &str, unit: Option<&str>) -> bool {
    let Some(place) = find_item(lua, what) else {
        return false;
    };
    match place {
        Place::Worn(slot) => {
            call(lua, "UseInventoryItem", slot);
        }
        Place::Bag(bag, slot) => {
            call(lua, "UseContainerItem", (bag, slot));
        }
    }
    if let Some(u) = unit {
        if truthy(call(lua, "SpellIsTargeting", ())) {
            call(lua, "SpellTargetUnit", u);
        }
    }
    true
}

/// Put a bag item in an inventory slot.
fn equip_to(lua: &Lua, what: &str, slot: i64) -> bool {
    match find_item(lua, what) {
        Some(Place::Bag(bag, s)) => {
            call(lua, "PickupContainerItem", (bag, s));
            call(lua, "PickupInventoryItem", slot);
            true
        }
        Some(Place::Worn(_)) | None => false,
    }
}

/// `/cast`'s act: a nested macro, an aura cancel, a spell, else an item of that name.
fn cast(lua: &Lua, c: &Chosen) -> bool {
    if nested_macro(lua, &c.action) {
        return true;
    }
    let name = clean(&c.action);
    if c.cancel_aura {
        if let Some(index) = own_buff(lua, &name) {
            call(lua, "CancelPlayerBuff", index);
            return true;
        }
    }
    if c.no_toggle && own_buff(lua, &name).is_some() {
        return false;
    }
    let known = num(call(lua, "GetSpellIdForName", name.as_str())) != 0.0;
    if !known && find_item(lua, &name).is_some() {
        return use_item(lua, &name, c.unit.as_deref());
    }
    match &c.unit {
        Some(u) => call(lua, "CastSpellByName", (name.as_str(), u.as_str())),
        None => call(lua, "CastSpellByName", name.as_str()),
    };
    true
}

fn attacking(lua: &Lua) -> bool {
    crate::script::action::cast_view(lua).attacking
}

/// `/target` with conditions and no `@unit`: the current target if it passes, else each enemy
/// `TargetNearestEnemy` cycles to, until one passes or the cycle comes back; with none, the
/// original target is restored.
fn scan_target(lua: &Lua, parsed: &Parsed, button: Option<u8>) -> bool {
    if choose(lua, parsed, button).is_some() && truthy(call(lua, "UnitExists", "target")) {
        return true;
    }
    let original = text(call(lua, "GetUnitGUID", "target"));
    let mut seen = Vec::new();
    for _ in 0..24 {
        call(lua, "TargetNearestEnemy", ());
        let Some(guid) = text(call(lua, "GetUnitGUID", "target")) else {
            break;
        };
        if seen.contains(&guid) {
            break;
        }
        seen.push(guid);
        if choose(lua, parsed, button).is_some() {
            return true;
        }
    }
    match original {
        Some(g) => {
            call(lua, "TargetUnit", g);
        }
        None => {
            call(lua, "ClearTarget", ());
        }
    }
    false
}

/// The castsequence state of one sequence text: its next step and when it last stepped.
#[derive(Default, Clone)]
pub struct Sequence {
    pub step: usize,
    pub at: f64,
    pub target: Option<String>,
}

/// `/castsequence [conds] reset=N/target/combat/shift/alt/ctrl a, b, c`.
fn cast_sequence(lua: &Lua, flow: &MacroFlow, key: &str, c: &Chosen) -> bool {
    let mut rest = c.action.trim();
    let mut reset_secs = None;
    let mut reset_on = Vec::new();
    if let Some(r) = rest.strip_prefix("reset=") {
        let end = r.find(' ').unwrap_or(r.len());
        for part in r[..end].split('/') {
            match part.parse::<f64>() {
                Ok(n) => reset_secs = Some(n),
                Err(_) => reset_on.push(part.to_ascii_lowercase()),
            }
        }
        rest = r[end..].trim();
    }
    let steps: Vec<String> = rest
        .split(',')
        .map(clean)
        .filter(|s| !s.is_empty())
        .collect();
    if steps.is_empty() {
        return false;
    }
    let now = num(call(lua, "GetTime", ()));
    let target = text(call(lua, "GetUnitGUID", "target"));
    let state = flow
        .sequences
        .borrow()
        .get(key)
        .cloned()
        .unwrap_or_default();
    let mut state = state;
    let modifier = |m: &str| match m {
        "shift" => truthy(call(lua, "IsShiftKeyDown", ())),
        "alt" => truthy(call(lua, "IsAltKeyDown", ())),
        "ctrl" => truthy(call(lua, "IsControlKeyDown", ())),
        _ => false,
    };
    let reset = reset_secs.is_some_and(|s| state.step > 0 && now - state.at > s)
        || (reset_on.iter().any(|r| r == "target") && state.target != target)
        || (reset_on.iter().any(|r| r == "combat")
            && !truthy(call(lua, "UnitAffectingCombat", "player")))
        || reset_on.iter().any(|r| modifier(r));
    if reset || state.step >= steps.len() {
        state.step = 0;
    }
    let spell = steps[state.step].clone();
    let cooling = num(call(lua, "GetSpellIdForName", spell.as_str())) != 0.0
        && call(lua, "IsSpellUsable", spell.as_str()).is_some_and(|v| num(Some(v)) != 1.0);
    match &c.unit {
        Some(u) => call(lua, "CastSpellByName", (spell.as_str(), u.as_str())),
        None => call(lua, "CastSpellByName", spell.as_str()),
    };
    if !cooling {
        state.step += 1;
        state.at = now;
        state.target = target;
    }
    flow.sequences.borrow_mut().insert(key.to_string(), state);
    true
}

/// Run `/command msg`. Returns whether it acted.
pub fn run(lua: &Lua, flow: &MacroFlow, cmd: &str, msg: &str, button: Option<u8>) -> bool {
    let parsed = flow.cache.borrow_mut().get(msg);
    let cmd = cmd.to_ascii_lowercase();
    // The flow commands act on their own conditions.
    let passes = || parsed.clauses.is_empty() || choose(lua, &parsed, button).is_some();
    match cmd.as_str() {
        "stopmacro" => {
            if passes() {
                flow.stop.set(true);
            }
            return true;
        }
        "skipmacro" => {
            if passes() {
                flow.skip.set(true);
            }
            return true;
        }
        "firstaction" => {
            if passes() {
                flow.first_action.set(true);
            }
            return true;
        }
        "nofirstaction" => {
            flow.first_action.set(false);
            return true;
        }
        _ => {}
    }
    if cmd == "target" && !parsed.clauses.is_empty() {
        let scan = parsed.clauses.iter().any(|c| {
            c.groups
                .iter()
                .any(|g| g.unit.is_none() && !g.conds.is_empty())
        });
        if scan {
            return scan_target(lua, &parsed, button);
        }
    }
    let chosen = match parsed.clauses.is_empty() {
        true => Some(Chosen {
            action: String::new(),
            unit: None,
            no_toggle: false,
            cancel_aura: false,
            at_cursor: false,
        }),
        false => choose(lua, &parsed, button),
    };
    let Some(c) = chosen else {
        return false;
    };
    let unit = c.unit.clone();
    let action = clean(&c.action);
    let done = match cmd.as_str() {
        "cast" | "spell" => {
            if flow.first_action.get() && flow.first_done.get() {
                return false;
            }
            let done = cast(lua, &c);
            if c.at_cursor {
                call(lua, "BenillaTargetAtCursor", ());
            }
            if done && flow.first_action.get() {
                flow.first_done.set(true);
            }
            done
        }
        "use" => {
            let done = nested_macro(lua, &c.action) || use_item(lua, &action, unit.as_deref());
            if c.at_cursor {
                call(lua, "BenillaTargetAtCursor", ());
            }
            done
        }
        "castsequence" => cast_sequence(lua, flow, msg, &c),
        "castpet" => {
            for i in 1..=10 {
                let name = text(call(lua, "GetPetActionInfo", i)).unwrap_or_default();
                if name.eq_ignore_ascii_case(&action) {
                    call(lua, "CastPetAction", i);
                    return true;
                }
            }
            false
        }
        "target" => {
            match (&unit, action.is_empty()) {
                (Some(u), _) => call(lua, "TargetUnit", u.as_str()),
                (None, false) => call(lua, "TargetByName", action.as_str()),
                (None, true) => None,
            };
            true
        }
        "startattack" => {
            if let Some(u) = &unit {
                call(lua, "TargetUnit", u.as_str());
            }
            if !attacking(lua) && truthy(call(lua, "UnitExists", "target")) {
                call(lua, "AttackTarget", ());
            }
            true
        }
        "stopattack" => {
            if attacking(lua) {
                call(lua, "AttackTarget", ());
            }
            true
        }
        "stopcasting" => {
            call(lua, "SpellStopCasting", ());
            true
        }
        "stopchanneling" => {
            call(lua, "ChannelStopCastingNextTick", ());
            true
        }
        "unqueue" => {
            call(lua, "BenillaSpellQueue", "clear");
            true
        }
        "cleartarget" => {
            call(lua, "ClearTarget", ());
            true
        }
        "cancelaura" | "unbuff" => match own_buff(lua, &action) {
            Some(index) => {
                call(lua, "CancelPlayerBuff", index);
                true
            }
            None => false,
        },
        "unshift" => {
            let n = num(call(lua, "GetNumShapeshiftForms", ())) as i64;
            for i in 1..=n {
                let f: mlua::Function = match lua.globals().get("GetShapeshiftFormInfo") {
                    Ok(f) => f,
                    Err(_) => break,
                };
                let active = f
                    .call::<(Value, Value, Value)>(i)
                    .map(|(_, _, a)| truthy(Some(a)))
                    .unwrap_or(false);
                if active {
                    call(lua, "CastShapeshiftForm", i);
                    return true;
                }
            }
            false
        }
        "petattack" => {
            call(lua, "PetAttack", ());
            true
        }
        "petfollow" => {
            call(lua, "PetFollow", ());
            true
        }
        "petwait" | "petstay" => {
            call(lua, "PetWait", ());
            true
        }
        "petpassive" => {
            call(lua, "PetPassiveMode", ());
            true
        }
        "petdefensive" => {
            call(lua, "PetDefensiveMode", ());
            true
        }
        "petaggressive" => {
            call(lua, "PetAggressiveMode", ());
            true
        }
        "equip" => match find_item(lua, &action) {
            Some(Place::Bag(bag, slot)) => {
                call(lua, "UseContainerItem", (bag, slot));
                true
            }
            _ => false,
        },
        "equipmh" => equip_to(lua, &action, 16),
        "equipoh" => equip_to(lua, &action, 17),
        "equip11" => equip_to(lua, &action, 11),
        "equip12" => equip_to(lua, &action, 12),
        "equip13" => equip_to(lua, &action, 13),
        "equip14" => equip_to(lua, &action, 14),
        "applymain" | "applyoff" => {
            if !use_item(lua, &action, None) {
                return false;
            }
            let slot = if cmd == "applymain" { 16 } else { 17 };
            call(lua, "PickupInventoryItem", slot);
            true
        }
        "retarget" => {
            let dead = truthy(call(lua, "UnitIsDeadOrGhost", "target"));
            let hostile = truthy(call(lua, "UnitCanAttack", ("player", "target")));
            if !truthy(call(lua, "UnitExists", "target")) || dead || !hostile {
                call(lua, "ClearTarget", ());
                call(lua, "TargetNearestEnemy", ());
            }
            true
        }
        _ => false,
    };
    done
}
