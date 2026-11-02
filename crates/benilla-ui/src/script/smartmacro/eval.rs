//! Smart macro conditions, evaluated against the client's state through its own API, which the
//! engine answers off live state ([`super::parse`] compiles what is evaluated). A player
//! condition always reads the player; a unit condition reads the group's `@unit`, else the
//! target. What a condition cannot know (another unit's aura time left, an addon's threat
//! figure it does not have) fails rather than guessing.

use mlua::{FromLuaMulti, IntoLuaMulti, Lua, Value};

use super::parse::{Arg, Cmp, Cond, Group, Join};

/// One evaluation's context: the Lua state and the clause's action, the subject of a bare
/// condition (`[nobuff]` on `Mark of the Wild`).
pub struct Ctx<'a> {
    pub lua: &'a Lua,
    pub action: &'a str,
    /// The mouse button that invoked the action, 1 for a key.
    pub button: Option<u8>,
}

/// The player conditions, which ignore `@unit`.
const PLAYER_CONDS: &[&str] = &[
    "bg",
    "button",
    "cdgcd",
    "channeled",
    "checkcasting",
    "checkchanneled",
    "combo",
    "cooldown",
    "druidmana",
    "equipped",
    "equipset",
    "focus",
    "form",
    "gcd",
    "group",
    "hastarget",
    "inbag",
    "incominghit",
    "indoors",
    "keydown",
    "known",
    "lastswing",
    "locked",
    "mhenchant",
    "mhimbue",
    "mod",
    "mounted",
    "moving",
    "mybuff",
    "mybuffcapped",
    "mybuffcount",
    "mycc",
    "mydebuff",
    "mydebuffcapped",
    "myenergy",
    "myhp",
    "myhplost",
    "mylevel",
    "mymana",
    "mypower",
    "mypowerlost",
    "myrage",
    "myrawhp",
    "myrawpower",
    "myspellhaste",
    "notarget",
    "ohenchant",
    "ohimbue",
    "onswingpending",
    "outdoors",
    "pet",
    "pethappiness",
    "queuedspell",
    "reactive",
    "resting",
    "rooted",
    "selfcasting",
    "set",
    "spellcasttime",
    "standing",
    "stat",
    "stealth",
    "swimming",
    "totem",
    "usable",
    "zone",
];

/// The 1.12 `SpellMechanic.dbc` ids by the name `[cc:...]` and `[immune:...]` take.
const MECHANICS: &[(&str, u32)] = &[
    ("charm", 1),
    ("disorient", 2),
    ("disoriented", 2),
    ("disarm", 3),
    ("distract", 4),
    ("fear", 5),
    ("grip", 6),
    ("root", 7),
    ("pacify", 8),
    ("silence", 9),
    ("sleep", 10),
    ("snare", 11),
    ("stun", 12),
    ("freeze", 13),
    ("knockout", 14),
    ("incapacitate", 14),
    ("bleed", 15),
    ("bandage", 16),
    ("polymorph", 17),
    ("banish", 18),
    ("shield", 19),
    ("shackle", 20),
    ("mount", 21),
    ("persuade", 22),
    ("turn", 23),
    ("horror", 24),
    ("invulnerability", 25),
    ("interrupt", 26),
    ("daze", 27),
    ("sap", 30),
];

/// The mechanics bare `[cc]` counts: loss of control.
const CONTROL: &[u32] = &[1, 2, 5, 6, 7, 9, 10, 12, 13, 14, 17, 18, 20, 23, 24, 30];

impl Ctx<'_> {
    /// `name(args)`, `None` when the global is missing or raises.
    fn call<R: FromLuaMulti>(&self, name: &str, args: impl IntoLuaMulti) -> Option<R> {
        let f: mlua::Function = self.lua.globals().get(name).ok()?;
        f.call::<R>(args).ok()
    }

    fn truthy(&self, name: &str, args: impl IntoLuaMulti) -> bool {
        matches!(
            self.call::<Value>(name, args),
            Some(v) if !matches!(v, Value::Nil | Value::Boolean(false))
        )
    }

    fn num(&self, name: &str, args: impl IntoLuaMulti) -> f64 {
        match self.call::<Value>(name, args) {
            Some(Value::Integer(n)) => n as f64,
            Some(Value::Number(n)) => n,
            _ => 0.0,
        }
    }

    fn text(&self, name: &str, args: impl IntoLuaMulti) -> Option<String> {
        match self.call::<Value>(name, args)? {
            Value::String(s) => s.to_str().ok().map(|s| s.to_string()),
            _ => None,
        }
    }
}

/// An aura as the conditions read it.
struct Aura {
    name: String,
    rank: String,
    stacks: f64,
    /// Seconds left, `None` where the client cannot tell.
    left: Option<f64>,
    dispel: Option<String>,
    spell: u32,
}

impl Ctx<'_> {
    /// `unit`'s buffs or debuffs: the spell id SuperWoW's `UnitBuff`/`UnitDebuff` give, named
    /// through `SpellInfo`; the player's own time left through `GetPlayerBuffTimeLeft`.
    fn auras(&self, unit: &str, debuff: bool) -> Vec<Aura> {
        let mut out = Vec::new();
        let getter = if debuff { "UnitDebuff" } else { "UnitBuff" };
        let limit = if debuff { 16 } else { 32 };
        for i in 1..=limit {
            let Some(vals) = self.call::<mlua::MultiValue>(getter, (unit, i)) else {
                break;
            };
            let v: Vec<Value> = vals.into_iter().collect();
            if matches!(v.first(), None | Some(Value::Nil)) {
                break;
            }
            let num = |x: Option<&Value>| match x {
                Some(Value::Integer(n)) => *n as f64,
                Some(Value::Number(n)) => *n,
                _ => 0.0,
            };
            let stacks = num(v.get(1)).max(1.0);
            let (dispel, spell) = if debuff {
                let d = match v.get(2) {
                    Some(Value::String(s)) => s.to_str().ok().map(|s| s.to_string()),
                    _ => None,
                };
                (d, num(v.get(3)) as u32)
            } else {
                (None, num(v.get(2)) as u32)
            };
            let (name, rank) = self
                .call::<(Option<String>, Option<String>)>("SpellInfo", spell)
                .map(|(n, r)| (n.unwrap_or_default(), r.unwrap_or_default()))
                .unwrap_or_default();
            out.push(Aura {
                name,
                rank,
                stacks,
                left: None,
                dispel,
                spell,
            });
        }
        if !unit.eq_ignore_ascii_case("player") {
            // Another unit's auras: the time left the client keeps for the player's own casts.
            let guid = self
                .text("GetUnitGUID", unit)
                .and_then(|g| u64::from_str_radix(g.trim_start_matches("0x"), 16).ok());
            if let Some(guid) = guid {
                for aura in &mut out {
                    aura.left = crate::script::live::ask(
                        self.lua,
                        &crate::script::OwnAuraLeftQuery {
                            unit: guid,
                            spell: aura.spell,
                        },
                    )
                    .flatten();
                }
            }
        }
        if unit.eq_ignore_ascii_case("player") {
            let filter = if debuff { "HARMFUL" } else { "HELPFUL" };
            for (i, aura) in out.iter_mut().enumerate() {
                let index = self.num("GetPlayerBuff", (i as i64, filter));
                if index >= 0.0 {
                    let left = self.num("GetPlayerBuffTimeLeft", index as i64);
                    aura.left = (left > 0.0).then_some(left);
                }
            }
        }
        out
    }
}

/// Whether `aura` is the one `want` names: its name, case-insensitive, with an optional
/// `(Rank N)`.
fn aura_named(aura: &Aura, want: &str) -> bool {
    let (base, rank) = match want.find('(') {
        Some(i) => (
            want[..i].trim(),
            Some(want[i + 1..].trim_end_matches(')').trim()),
        ),
        None => (want.trim(), None),
    };
    aura.name.eq_ignore_ascii_case(base) && rank.is_none_or(|r| aura.rank.eq_ignore_ascii_case(r))
        || want.parse::<u32>().ok() == Some(aura.spell)
}

/// The item id in an item link (`item:19019:...`).
fn item_id_of(link: &str) -> Option<i64> {
    let rest = &link[link.find("item:")? + 5..];
    let end = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    rest[..end].parse().ok()
}

/// A school's bit in the lockout mask (`1 << School`).
fn school_bit(name: &str) -> Option<u32> {
    let school = match name.to_ascii_lowercase().as_str() {
        "physical" => 0,
        "holy" => 1,
        "fire" => 2,
        "nature" => 3,
        "frost" => 4,
        "shadow" => 5,
        "arcane" => 6,
        _ => return None,
    };
    Some(1 << school)
}

fn all_cmps(value: f64, cmps: &[Cmp]) -> bool {
    cmps.iter().all(|c| c.op.test(value, c.amount))
}

/// `a`'s time or stack comparisons against `aura`: an aura with no known time counts as having
/// none left (so `<` passes and `>` fails), as SuperCleveRoidMacros reads a missing one.
fn aura_cmps(aura: Option<&Aura>, cmps: &[Cmp]) -> bool {
    cmps.iter().all(|c| {
        let value = match aura {
            None => 0.0,
            Some(a) if c.stacks => a.stacks,
            Some(a) => a
                .left
                .unwrap_or(if aura.is_some() { f64::MAX } else { 0.0 }),
        };
        c.op.test(value, c.amount)
    })
}

/// A percentage, 0 for an empty pool.
fn pct(cur: f64, max: f64) -> f64 {
    if max > 0.0 {
        cur * 100.0 / max
    } else {
        0.0
    }
}

impl Ctx<'_> {
    /// The unit a condition reads.
    fn unit_for<'u>(&self, cond: &Cond, group: &'u Group) -> std::borrow::Cow<'u, str> {
        if PLAYER_CONDS.contains(&cond.name) {
            return "player".into();
        }
        match &group.unit {
            Some(u) => u.as_str().into(),
            None => "target".into(),
        }
    }

    /// The argument's values, or the action when the condition is bare.
    fn subjects<'c>(&'c self, cond: &'c Cond) -> Vec<Arg> {
        if cond.args.is_empty() {
            vec![Arg::Text(super::parse::clean_name(self.action))]
        } else {
            cond.args.clone()
        }
    }

    /// Combine per-value results by the condition's join.
    fn each(&self, cond: &Cond, mut test: impl FnMut(&Arg) -> bool) -> bool {
        let subjects = self.subjects(cond);
        match cond.join {
            Join::Any => subjects.iter().any(&mut test),
            Join::All => subjects.iter().all(&mut test),
        }
    }

    /// A numeric condition: every comparison of every value against `value`.
    fn numeric(&self, cond: &Cond, value: f64) -> bool {
        if cond.args.is_empty() {
            return value > 0.0;
        }
        self.each(cond, |a| match a {
            Arg::Compare { cmps, .. } => all_cmps(value, cmps),
            Arg::Text(t) => t.parse::<f64>().is_ok_and(|n| (n - value).abs() < 1e-9),
        })
    }

    fn has_aura(&self, unit: &str, debuff: bool, cond: &Cond) -> bool {
        let auras = self.auras(unit, debuff);
        self.each(cond, |a| {
            let found = auras.iter().find(|x| aura_named(x, a.name()));
            match a {
                Arg::Compare { cmps, .. } => aura_cmps(found, cmps),
                Arg::Text(_) => found.is_some(),
            }
        })
    }

    fn mechanics_on(&self, unit: &str) -> Vec<u32> {
        self.auras(unit, true)
            .iter()
            .map(|a| self.num("GetSpellRecField", (a.spell, "mechanic")) as u32)
            .filter(|&m| m != 0)
            .collect()
    }

    fn cc(&self, unit: &str, cond: &Cond) -> bool {
        let on = self.mechanics_on(unit);
        if cond.args.is_empty() {
            return on.iter().any(|m| CONTROL.contains(m));
        }
        self.each(cond, |a| {
            MECHANICS
                .iter()
                .find(|(n, _)| n.eq_ignore_ascii_case(a.name()))
                .is_some_and(|(_, id)| on.contains(id))
        })
    }

    fn dispel(&self, unit: &str, kind: &str) -> bool {
        self.auras(unit, true).iter().any(|a| {
            a.dispel
                .as_deref()
                .is_some_and(|d| d.eq_ignore_ascii_case(kind))
        })
    }

    /// A spell's id by name or number, 0 for neither.
    fn spell_id(&self, name: &str) -> u32 {
        name.parse::<u32>()
            .ok()
            .unwrap_or_else(|| self.num("GetSpellIdForName", name) as u32)
    }

    /// A spell's or item's cooldown left in seconds, with the global cooldown or without.
    fn cooldown_left(&self, name: &str, with_gcd: bool) -> f64 {
        let spell = self.spell_id(name);
        let table: Option<mlua::Table> = if spell != 0 {
            self.call("GetSpellIdCooldown", spell)
        } else {
            let item = self.item_id(name);
            (item != 0)
                .then(|| self.call("GetItemIdCooldown", item))
                .flatten()
        };
        let Some(t) = table else {
            return 0.0;
        };
        let ms = |k: &str| t.get::<f64>(k).unwrap_or(0.0);
        let left = if with_gcd {
            ms("cooldownRemainingMs")
        } else {
            ms("individualRemainingMs").max(ms("categoryRemainingMs"))
        };
        left / 1000.0
    }

    /// An item's id by number, link or the name of one in the bags or worn.
    fn item_id(&self, name: &str) -> u32 {
        if let Ok(n) = name.parse::<u32>() {
            return n;
        }
        let from_link = |link: Option<String>| -> Option<u32> {
            let link = link?;
            let i = link.find("item:")? + 5;
            link[i..].split(':').next()?.parse().ok()
        };
        let named = |link: &Option<String>| {
            link.as_deref().is_some_and(|l| {
                l.find('[')
                    .zip(l.find(']'))
                    .is_some_and(|(a, b)| a < b && l[a + 1..b].eq_ignore_ascii_case(name))
            })
        };
        for slot in 0..=19 {
            let link = self.text("GetInventoryItemLink", ("player", slot));
            if named(&link) {
                return from_link(link).unwrap_or(0);
            }
        }
        for bag in 0..=4 {
            let slots = self.num("GetContainerNumSlots", bag) as i64;
            for slot in 1..=slots {
                let link = self.text("GetContainerItemLink", (bag, slot));
                if named(&link) {
                    return from_link(link).unwrap_or(0);
                }
            }
        }
        0
    }

    /// How many of `name` the bags hold.
    fn bag_count(&self, name: &str) -> f64 {
        let id = self.item_id(name);
        let mut n = 0.0;
        for bag in 0..=4 {
            let slots = self.num("GetContainerNumSlots", bag) as i64;
            for slot in 1..=slots {
                let link = self.text("GetContainerItemLink", (bag, slot));
                let same = link.as_deref().is_some_and(|l| {
                    l.contains(&format!("item:{id}:"))
                        || l.to_ascii_lowercase()
                            .contains(&format!("[{}]", name.to_ascii_lowercase()))
                });
                if same {
                    let (_, count) = self
                        .call::<(Value, Option<f64>)>("GetContainerItemInfo", (bag, slot))
                        .unwrap_or((Value::Nil, None));
                    n += count.unwrap_or(1.0);
                }
            }
        }
        n
    }

    fn modifier(&self, which: &str) -> bool {
        match which.to_ascii_lowercase().as_str() {
            "shift" | "lshift" | "rshift" => self.truthy("IsShiftKeyDown", ()),
            "ctrl" | "lctrl" | "rctrl" => self.truthy("IsControlKeyDown", ()),
            "alt" | "lalt" | "ralt" => self.truthy("IsAltKeyDown", ()),
            _ => false,
        }
    }

    /// The active form's index, 0 for none.
    fn form(&self) -> f64 {
        let n = self.num("GetNumShapeshiftForms", ()) as i64;
        for i in 1..=n {
            if let Some((_, _, active)) =
                self.call::<(Value, Value, Option<Value>)>("GetShapeshiftFormInfo", i)
            {
                if active.is_some_and(|v| !matches!(v, Value::Nil | Value::Boolean(false))) {
                    return i as f64;
                }
            }
        }
        0.0
    }

    /// The distance UnitXP measures from the player, `None` where it cannot.
    fn distance(&self, unit: &str, meter: &str) -> Option<f64> {
        match self.call::<Value>("UnitXP", ("distanceBetween", "player", unit, meter))? {
            Value::Number(n) => Some(n),
            Value::Integer(n) => Some(n as f64),
            _ => None,
        }
    }

    fn cast_info(&self) -> Option<mlua::Table> {
        self.call::<Option<mlua::Table>>("GetCastInfo", ())
            .flatten()
    }

    fn equipped(&self, cond: &Cond) -> bool {
        self.each(cond, |a| {
            let want = a.name().to_ascii_lowercase();
            (0..=19).any(|slot| {
                let Some(link) = self.text("GetInventoryItemLink", ("player", slot)) else {
                    return false;
                };
                if link.to_ascii_lowercase().contains(&format!("[{want}]")) {
                    return true;
                }
                // A weapon or armour type: `GetItemInfo`'s class and subclass.
                let id = link
                    .find("item:")
                    .and_then(|i| link[i + 5..].split(':').next()?.parse::<u32>().ok());
                id.and_then(|id| {
                    self.call::<(Value, Value, Value, Value, Value, Option<String>)>(
                        "GetItemInfo",
                        id,
                    )
                })
                .and_then(|t| t.5)
                .is_some_and(|sub| {
                    let sub = sub.to_ascii_lowercase();
                    sub == want
                        || sub.trim_start_matches("one-handed ") == want.trim_end_matches('s')
                        || sub.starts_with(want.trim_end_matches('s'))
                })
            })
        })
    }
}

/// The conditions that count enemies when given a count comparison.
const COUNTABLE: &[&str] = &[
    "meleerange",
    "behind",
    "facing",
    "insight",
    "inrange",
    "outrange",
    "distance",
];

/// How far count mode and multiscan look, past any spell's range.
const SCAN_RADIUS: f32 = 45.0;

impl Ctx<'_> {
    /// The hostile, living units around the player, as guid tokens SuperWoW's unit functions take.
    pub fn enemies(&self, radius: f32) -> Vec<String> {
        let guids = crate::script::live::ask(self.lua, &crate::script::NearbyUnitsQuery(radius))
            .unwrap_or_default();
        guids
            .into_iter()
            .map(crate::script::guid_text)
            .filter(|u| {
                self.truthy("UnitCanAttack", ("player", u.as_str()))
                    && !self.truthy("UnitIsDeadOrGhost", u.as_str())
            })
            .collect()
    }

    /// One enemy against a count check's primary test or filter.
    fn passes_check(&self, check: &str, unit: &str, spell: &str) -> bool {
        match check {
            "meleerange" => self
                .distance(unit, "meleeAutoAttack")
                .is_some_and(|d| d <= 0.0),
            "behind" => self.truthy("UnitXP", ("behind", "player", unit)),
            "facing" => !self.truthy("UnitXP", ("behind", unit, "player")),
            "insight" => self.truthy("UnitXP", ("inSight", "player", unit)),
            "inrange" => self.num("IsSpellInRange", (spell, unit)) == 1.0,
            "outrange" => self.num("IsSpellInRange", (spell, unit)) == 0.0,
            radius => radius
                .parse::<f64>()
                .ok()
                .zip(self.distance(unit, "ranged"))
                .is_some_and(|(r, d)| d <= r),
        }
    }

    /// Count mode (`[meleerange:facing>1]`, `[distance:30:facing>2]`, `[inrange:Multi-Shot>1]`):
    /// `None` when the condition is not counting.
    fn count_mode(&self, cond: &Cond) -> Option<bool> {
        if !COUNTABLE.contains(&cond.name) || cond.args.len() != 1 {
            return None;
        }
        let Arg::Compare { name, cmps } = &cond.args[0] else {
            return None;
        };
        // `[distance:<40]` compares the target's distance; `[distance:30>2]` counts.
        if cond.name == "distance" && name.is_empty() {
            return None;
        }
        let parts: Vec<&str> = name.split(':').map(str::trim).collect();
        let (primary, filter, spell) = match cond.name {
            "distance" => (parts[0], parts.get(1).copied(), ""),
            "inrange" | "outrange" => (cond.name, None, parts[0]),
            n => (n, parts.first().copied().filter(|p| !p.is_empty()), ""),
        };
        let spell = if spell.is_empty() {
            super::parse::clean_name(self.action)
        } else {
            spell.to_string()
        };
        let count = self
            .enemies(SCAN_RADIUS)
            .iter()
            .filter(|u| {
                self.passes_check(primary, u, &spell) != cond.negated
                    && filter.is_none_or(|f| self.passes_check(f, u, &spell))
            })
            .count() as f64;
        Some(all_cmps(count, cmps))
    }

    /// Whether `cond` holds in `group`, its negation applied.
    pub fn test(&self, cond: &Cond, group: &Group) -> bool {
        if let Some(counted) = self.count_mode(cond) {
            return counted;
        }
        let unit = self.unit_for(cond, group);
        let unit = unit.as_ref();
        let held = self.holds(cond, group, unit);
        held != cond.negated
    }

    /// Every condition of `group`.
    pub fn group_passes(&self, group: &Group) -> bool {
        group.conds.iter().all(|c| self.test(c, group))
    }

    fn holds(&self, cond: &Cond, group: &Group, unit: &str) -> bool {
        let exists = || self.truthy("UnitExists", unit);
        let hp = |u: &str| (self.num("UnitHealth", u), self.num("UnitHealthMax", u));
        let power = |u: &str| (self.num("UnitMana", u), self.num("UnitManaMax", u));
        match cond.name {
            // ── Units ───────────────────────────────────────────────────────────────────────
            "exists" => exists(),
            "target" => true,
            "alive" => exists() && !self.truthy("UnitIsDeadOrGhost", unit),
            "dead" => exists() && self.truthy("UnitIsDeadOrGhost", unit),
            "help" => exists() && self.truthy("UnitCanAssist", ("player", unit)),
            "harm" => exists() && self.truthy("UnitCanAttack", ("player", unit)),
            "isplayer" => self.truthy("UnitIsPlayer", unit),
            "isnpc" => exists() && !self.truthy("UnitIsPlayer", unit),
            "playercontrolled" => self.truthy("UnitPlayerControlled", unit),
            "member" => self.truthy("UnitInParty", unit) || self.truthy("UnitInRaid", unit),
            "party" => {
                let u = cond.args.first().map_or(unit, |a| a.name());
                self.truthy("UnitInParty", u)
            }
            "raid" => {
                let u = cond.args.first().map_or(unit, |a| a.name());
                self.truthy("UnitInRaid", u)
            }
            "tag" => self.truthy("UnitIsTapped", unit),
            "mytag" => self.truthy("UnitIsTappedByPlayer", unit),
            "othertag" => {
                self.truthy("UnitIsTapped", unit) && !self.truthy("UnitIsTappedByPlayer", unit)
            }
            "name" => {
                let name = self.text("UnitName", unit).unwrap_or_default();
                self.each(cond, |a| a.name().eq_ignore_ascii_case(&name))
            }
            "class" => {
                let class = self.text("UnitClass", unit).unwrap_or_default();
                self.truthy("UnitIsPlayer", unit)
                    && self.each(cond, |a| a.name().eq_ignore_ascii_case(&class))
            }
            "type" => {
                let kind = self.text("UnitCreatureType", unit).unwrap_or_default();
                self.each(cond, |a| a.name().eq_ignore_ascii_case(&kind))
            }
            "level" => {
                let mut level = self.num("UnitLevel", unit);
                if level < 0.0 {
                    level = 63.0;
                }
                self.numeric(cond, level)
            }
            "hp" => {
                let (c, m) = hp(unit);
                self.numeric(cond, pct(c, m))
            }
            "rawhp" => self.numeric(cond, hp(unit).0),
            "hplost" => {
                let (c, m) = hp(unit);
                self.numeric(cond, m - c)
            }
            "power" | "mana" | "rage" | "energy" => {
                let (c, m) = power(unit);
                let kind = self.num("UnitPowerType", unit) as i64;
                let want = match cond.name {
                    "mana" => Some(0),
                    "rage" => Some(1),
                    "energy" => Some(3),
                    _ => None,
                };
                if want.is_some_and(|w| w != kind) {
                    return false;
                }
                // Rage reads raw (0..100 is its percent), the others as a share.
                self.numeric(cond, if kind == 1 { c } else { pct(c, m) })
            }
            "rawpower" => self.numeric(cond, power(unit).0),
            "powerlost" => {
                let (c, m) = power(unit);
                self.numeric(cond, m - c)
            }
            "powertype" => {
                let kind = self.num("UnitPowerType", unit) as i64;
                let name = ["mana", "rage", "focus", "energy", "happiness"]
                    .get(kind as usize)
                    .copied()
                    .unwrap_or("");
                self.each(cond, |a| a.name().eq_ignore_ascii_case(name))
            }
            "combat" => {
                let u = cond
                    .args
                    .first()
                    .map_or(if group.unit.is_some() { unit } else { "player" }, |a| {
                        a.name()
                    });
                self.truthy("UnitAffectingCombat", u)
            }
            "buff" => self.has_aura(unit, false, cond),
            "debuff" => self.has_aura(unit, true, cond),
            "cursive" => self.has_aura(unit, true, cond),
            "buffcapped" => self.auras(unit, false).len() >= 32,
            "debuffcapped" => self.auras(unit, true).len() >= 16,
            "cc" => self.cc(unit, cond),
            "magic" | "curse" | "disease" | "poison" => self.dispel(unit, cond.name),
            "dispellable" => ["magic", "curse", "disease", "poison"]
                .iter()
                .any(|k| self.dispel(unit, k)),
            "dispellablebuff" | "magicbuff" => self
                .auras(unit, false)
                .iter()
                .any(|a| self.num("GetSpellRecField", (a.spell, "dispel")) == 1.0),
            "casting" | "casttime" | "channeltime" => {
                // The player's own off the cast record; another unit's off its packets.
                let (spell, left, channel) = if unit.eq_ignore_ascii_case("player") {
                    let Some(info) = self.cast_info() else {
                        return false;
                    };
                    (
                        info.get::<u32>("spellId").unwrap_or(0),
                        info.get::<f64>("castRemainingMs").unwrap_or(0.0) / 1000.0,
                        info.get::<i64>("castType").unwrap_or(0) == 3,
                    )
                } else {
                    match self
                        .call::<(Option<u32>, Option<f64>, Option<bool>)>("BenillaUnitCast", unit)
                    {
                        Some((Some(spell), Some(left), Some(channel))) => (spell, left, channel),
                        _ => return false,
                    }
                };
                match cond.name {
                    "casttime" => !channel && self.numeric(cond, left),
                    "channeltime" => channel && self.numeric(cond, left),
                    _ => {
                        !channel
                            && (cond.args.is_empty()
                                || self.each(cond, |a| self.spell_id(a.name()) == spell))
                    }
                }
            }
            "distance" => {
                let Some(d) = self.distance(unit, "ranged") else {
                    return false;
                };
                if cond.args.is_empty() {
                    return true;
                }
                self.numeric(cond, d)
            }
            "meleerange" => self
                .distance(unit, "meleeAutoAttack")
                .is_some_and(|d| d <= 0.0),
            "inrange" | "outrange" => {
                let spell = cond.args.first().map_or_else(
                    || super::parse::clean_name(self.action),
                    |a| a.name().to_string(),
                );
                let r = self.num("IsSpellInRange", (spell, unit));
                if cond.name == "inrange" {
                    r == 1.0
                } else {
                    r == 0.0
                }
            }
            "insight" => self.truthy("UnitXP", ("inSight", "player", unit)),
            "behind" => self.truthy("UnitXP", ("behind", "player", unit)),
            "facing" => exists() && !self.truthy("UnitXP", ("behind", unit, "player")),
            "targeting" => {
                let target = format!("{unit}target");
                self.each(cond, |a| match a.name().to_ascii_lowercase().as_str() {
                    "player" | "" => self.truthy("UnitIsUnit", (target.as_str(), "player")),
                    other => self.truthy("UnitIsUnit", (target.as_str(), other)),
                })
            }
            // Seconds until the unit dies, or reaches 20%, at the rate its health falls.
            "ttk" | "tte" => {
                let Some((Some(ttk), Some(tte))) =
                    self.call::<(Option<f64>, Option<f64>)>("BenillaUnitTrend", unit)
                else {
                    return false;
                };
                self.numeric(cond, if cond.name == "ttk" { ttk } else { tte })
            }
            // Our last spell on the unit resisted: any, or "full" / "partial".
            "resisted" => {
                let Some(kind) = self
                    .call::<Option<String>>("BenillaUnitResisted", unit)
                    .flatten()
                else {
                    return false;
                };
                cond.args.is_empty() || self.each(cond, |a| a.name().eq_ignore_ascii_case(&kind))
            }
            // Known immune, as our spells' immune misses taught: a school by name, else a spell
            // (the action when bare).
            "immune" => self.each(cond, |a| {
                let name = a.name();
                let school = [
                    "physical", "holy", "fire", "nature", "frost", "shadow", "arcane",
                ]
                .iter()
                .any(|k| k.eq_ignore_ascii_case(name));
                if school {
                    self.truthy("BenillaUnitImmune", (unit, name.to_ascii_lowercase()))
                } else {
                    let id = self.spell_id(name);
                    id != 0 && self.truthy("BenillaUnitImmune", (unit, id))
                }
            }),
            "istank" | "threat" | "multiscan" => {
                // Read through the addons that track them, where loaded.
                self.addon_fact(cond, unit)
            }

            // ── The player ─────────────────────────────────────────────────────────────────
            "mod" => {
                if cond.args.is_empty() {
                    return self.modifier("shift") || self.modifier("ctrl") || self.modifier("alt");
                }
                self.each(cond, |a| self.modifier(a.name()))
            }
            "button" => {
                let b = self.button.unwrap_or(1) as f64;
                if cond.args.is_empty() {
                    return self.button.is_some();
                }
                self.each(cond, |a| a.name().parse::<f64>().ok() == Some(b))
            }
            "keydown" => self.each(cond, |a| self.truthy("IsKeyDown", a.name().to_uppercase())),
            "hastarget" => self.truthy("UnitExists", "target"),
            "notarget" => !self.truthy("UnitExists", "target"),
            "focus" => self.truthy("UnitExists", "focus"),
            "group" => {
                let party = self.num("GetNumPartyMembers", ()) > 0.0;
                let raid = self.num("GetNumRaidMembers", ()) > 0.0;
                if cond.args.is_empty() {
                    return party || raid;
                }
                self.each(cond, |a| match a.name().to_ascii_lowercase().as_str() {
                    "party" => party,
                    "raid" => raid,
                    _ => false,
                })
            }
            "bg" => {
                let zone = self.text("GetRealZoneText", ()).unwrap_or_default();
                ["Warsong Gulch", "Arathi Basin", "Alterac Valley"]
                    .iter()
                    .any(|z| z.eq_ignore_ascii_case(&zone))
            }
            "zone" => {
                let zone = self.text("GetRealZoneText", ()).unwrap_or_default();
                let sub = self.text("GetSubZoneText", ()).unwrap_or_default();
                self.each(cond, |a| {
                    a.name().eq_ignore_ascii_case(&zone) || a.name().eq_ignore_ascii_case(&sub)
                })
            }
            "resting" => self.truthy("IsResting", ()),
            "combo" => self.numeric(cond, self.num("GetComboPoints", ())),
            "form" => {
                let f = self.form();
                if cond.args.is_empty() {
                    return f > 0.0;
                }
                self.each(cond, |a| match a {
                    Arg::Compare { cmps, .. } => all_cmps(f, cmps),
                    Arg::Text(t) => t.parse::<f64>().ok() == Some(f),
                })
            }
            "stealth" => self.auras("player", false).iter().any(|a| {
                ["Stealth", "Prowl", "Shadowmeld", "Vanish"]
                    .iter()
                    .any(|s| a.name.eq_ignore_ascii_case(s))
            }),
            "mounted" => self.truthy("IsMounted", ()),
            "swimming" => self.truthy("IsSwimming", ()),
            "indoors" => self.truthy("IsIndoors", ()),
            "outdoors" => self.truthy("IsOutdoors", ()),
            "moving" => {
                let (_, pct) = self
                    .call::<(f64, f64)>("GetSpeed", ())
                    .unwrap_or((0.0, 0.0));
                if cond.args.is_empty() {
                    pct > 0.0
                } else {
                    self.numeric(cond, pct)
                }
            }
            "rooted" => self.truthy("PlayerIsRooted", ()),
            "standing" => self.num("UnitStandState", "player") == 0.0,
            "myhp" => {
                let (c, m) = hp("player");
                self.numeric(cond, pct(c, m))
            }
            "myrawhp" => self.numeric(cond, hp("player").0),
            "myhplost" => {
                let (c, m) = hp("player");
                self.numeric(cond, m - c)
            }
            "mylevel" => self.numeric(cond, self.num("UnitLevel", "player")),
            "mypower" | "mymana" | "myrage" | "myenergy" => {
                let (c, m) = power("player");
                let kind = self.num("UnitPowerType", "player") as i64;
                let want = match cond.name {
                    "mymana" => Some(0),
                    "myrage" => Some(1),
                    "myenergy" => Some(3),
                    _ => None,
                };
                if want.is_some_and(|w| w != kind) {
                    return false;
                }
                self.numeric(cond, if kind == 1 { c } else { pct(c, m) })
            }
            "myrawpower" => self.numeric(cond, power("player").0),
            "mypowerlost" => {
                let (c, m) = power("player");
                self.numeric(cond, m - c)
            }
            "druidmana" => self.numeric(cond, self.num("UnitMana", "player")),
            "mybuff" => self.has_aura("player", false, cond),
            "mydebuff" => self.has_aura("player", true, cond),
            "mybuffcapped" => self.auras("player", false).len() >= 32,
            "mydebuffcapped" => self.auras("player", true).len() >= 16,
            "mybuffcount" => self.numeric(cond, self.auras("player", false).len() as f64),
            "mycc" => self.cc("player", cond),
            "cooldown" | "cdgcd" => self.each(cond, |a| {
                let name = if a.name().is_empty() {
                    super::parse::clean_name(self.action)
                } else {
                    a.name().to_string()
                };
                let left = self.cooldown_left(&name, cond.name == "cdgcd");
                match a {
                    Arg::Compare { cmps, .. } => all_cmps(left, cmps),
                    Arg::Text(_) => left > 0.0,
                }
            }),
            "gcd" => {
                let left = self
                    .cast_info()
                    .and_then(|t| t.get::<f64>("gcdRemainingMs").ok())
                    .unwrap_or(0.0)
                    / 1000.0;
                if cond.args.is_empty() {
                    left > 0.0
                } else {
                    self.numeric(cond, left)
                }
            }
            "usable" | "reactive" => self.each(cond, |a| {
                self.num("IsSpellUsable", a.name()) == 1.0
                    || (self.spell_id(a.name()) == 0 && self.item_id(a.name()) != 0)
            }),
            "known" => self.each(cond, |a| self.spell_id(a.name()) != 0),
            "inbag" => self.each(cond, |a| {
                let n = self.bag_count(a.name());
                match a {
                    Arg::Compare { cmps, .. } => all_cmps(n, cmps),
                    Arg::Text(_) => n > 0.0,
                }
            }),
            "equipped" => self.equipped(cond),
            "selfcasting" | "channeled" | "checkcasting" | "checkchanneled" => {
                let info = self.cast_info();
                let channel = info
                    .as_ref()
                    .is_some_and(|t| t.get::<i64>("castType").unwrap_or(0) == 3);
                let spell = info
                    .as_ref()
                    .and_then(|t| t.get::<u32>("spellId").ok())
                    .unwrap_or(0);
                let matches = |want: bool| {
                    want && (cond.args.is_empty()
                        || self.each(cond, |a| self.spell_id(a.name()) == spell))
                };
                match cond.name {
                    "selfcasting" => matches(info.is_some()),
                    "channeled" => matches(channel),
                    "checkcasting" => !matches(info.is_some() && !channel),
                    _ => !matches(channel),
                }
            }
            "pet" => {
                if !self.truthy("UnitExists", "pet") {
                    return false;
                }
                if cond.args.is_empty() {
                    return true;
                }
                let family = self.text("UnitCreatureFamily", "pet").unwrap_or_default();
                self.each(cond, |a| a.name().eq_ignore_ascii_case(&family))
            }
            "pethappiness" => {
                let h = self.num("GetPetHappiness", ());
                self.each(cond, |a| match a.name().to_ascii_lowercase().as_str() {
                    "unhappy" => h == 1.0,
                    "content" => h == 2.0,
                    "happy" => h == 3.0,
                    n => n.parse::<f64>().ok() == Some(h),
                })
            }
            "spellcasttime" => {
                let name = cond
                    .args
                    .first()
                    .filter(|a| !a.name().is_empty())
                    .map_or_else(
                        || super::parse::clean_name(self.action),
                        |a| a.name().into(),
                    );
                let id = self.spell_id(&name);
                // The table's time, scaled by the cast speed haste and slows put on it.
                let base = crate::script::live::ask(self.lua, &crate::script::CastTimeQuery(id))
                    .flatten()
                    .unwrap_or(0);
                let speed = self
                    .call::<f64>("GetUnitField", ("player", "modCastSpeed"))
                    .filter(|s| *s > 0.0)
                    .unwrap_or(1.0);
                self.numeric(cond, f64::from(base) / 1000.0 * speed)
            }
            "mhimbue" | "ohimbue" | "mhenchant" | "ohenchant" => {
                let (has_mh, _, _, has_oh) = self
                    .call::<(Value, Value, Value, Value)>("GetWeaponEnchantInfo", ())
                    .unwrap_or((Value::Nil, Value::Nil, Value::Nil, Value::Nil));
                let on = |v: &Value| !matches!(v, Value::Nil | Value::Boolean(false));
                if cond.name.starts_with("mh") {
                    on(&has_mh)
                } else {
                    on(&has_oh)
                }
            }
            "stat" => self.each(cond, |a| {
                let Arg::Compare { name, cmps } = a else {
                    return false;
                };
                let value = match name.to_ascii_lowercase().as_str() {
                    "str" | "strength" => self.stat(1),
                    "agi" | "agility" => self.stat(2),
                    "sta" | "stamina" => self.stat(3),
                    "int" | "intellect" => self.stat(4),
                    "spi" | "spirit" => self.stat(5),
                    "ap" | "attackpower" => self.attack_power("UnitAttackPower"),
                    "rap" | "rangedattackpower" => self.attack_power("UnitRangedAttackPower"),
                    "armor" => self
                        .call::<(f64, f64)>("UnitArmor", "player")
                        .map_or(0.0, |(_, effective)| effective),
                    _ => return false,
                };
                all_cmps(value, cmps)
            }),
            "swingtimer" | "rangedtimer" => {
                let t = self.timers();
                let (since, field) = if cond.name == "swingtimer" {
                    (t.since_main, 126)
                } else {
                    (t.since_ranged, 128)
                };
                let speed = self.attack_time(field);
                let Some(since) = since.filter(|_| speed > 0.0) else {
                    return false;
                };
                let pct = (since / speed * 100.0).min(100.0);
                if cond.args.is_empty() {
                    pct < 100.0
                } else {
                    self.numeric(cond, pct)
                }
            }
            "lastswing" | "incominghit" => {
                let t = self.timers();
                let last = if cond.name == "lastswing" {
                    t.last_out
                } else {
                    t.last_in
                };
                let Some((kind, ago)) = last else {
                    return false;
                };
                if cond.args.is_empty() {
                    return true;
                }
                self.each(cond, |a| match a {
                    Arg::Compare { name, cmps } => {
                        (name.is_empty() || name.eq_ignore_ascii_case(kind.name()))
                            && all_cmps(ago, cmps)
                    }
                    Arg::Text(t) => t.eq_ignore_ascii_case(kind.name()),
                })
            }
            "slamclip" | "nextslamclip" | "rangedclip" => {
                let t = self.timers();
                let (since, field) = if cond.name == "rangedclip" {
                    (t.since_ranged, 128)
                } else {
                    (t.since_main, 126)
                };
                let speed = self.attack_time(field);
                let Some(left) = since.map(|s| speed - s).filter(|l| *l > 0.0) else {
                    return false;
                };
                // A cast clips the swing when it ends after the swing is due: Slam's 1.5 s,
                // after a global cooldown for the next one; a ranged cast its own time.
                let cast = match cond.name {
                    "slamclip" => 1.5,
                    "nextslamclip" => 3.0,
                    _ => 1.5,
                };
                left < cast
            }
            "onswingpending" => {
                let view = crate::script::action::cast_view(self.lua);
                view.slot.is_some_and(|spell| {
                    let attrs = self.num("GetSpellRecField", (spell, "attributes")) as u32;
                    attrs & (0x4 | 0x400) != 0
                })
            }
            // The client's queue: any queued spell, or the named one.
            "queuedspell" => {
                let queued = self.num("BenillaSpellQueue", ()) as u32;
                queued != 0
                    && (cond.args.is_empty()
                        || self.each(cond, |a| match a {
                            Arg::Text(name) | Arg::Compare { name, .. } => {
                                name.is_empty()
                                    || self.num("GetSpellIdForName", name.as_str()) as u32 == queued
                            }
                        }))
            }
            // The spell-school interrupt lockout: any school, or the named ones.
            "locked" => {
                let mask = self.num("BenillaSchoolLockout", ()) as u32;
                if cond.args.is_empty() {
                    return mask != 0;
                }
                self.each(cond, |a| match a {
                    Arg::Text(name) | Arg::Compare { name, .. } => {
                        school_bit(name).is_some_and(|bit| mask & bit != 0)
                    }
                })
            }
            // A totem slot (fire, earth, water, air, 1-4) or a totem by name, the action's when
            // bare; comparisons read its seconds left, `#` ones whether it is down.
            "totem" => self.each(cond, |a| {
                let (name, cmps) = match a {
                    Arg::Text(name) => (name.as_str(), &[][..]),
                    Arg::Compare { name, cmps } => (name.as_str(), cmps.as_slice()),
                };
                let name = if name.is_empty() {
                    super::parse::clean_name(self.action)
                } else {
                    name.to_string()
                };
                let Some(slot) = self.totem_slot(&name) else {
                    return cmps.iter().all(|c| c.op.test(-1.0, c.amount));
                };
                let left: f64 = self.num("GetTotemTimeLeft", slot);
                cmps.iter().all(|c| {
                    let value = if c.stacks { 1.0 } else { left };
                    c.op.test(value, c.amount)
                })
            }),
            // Spell haste in percent off the cast speed factor: positive hasted, negative slowed.
            "myspellhaste" => {
                let speed = self
                    .call::<f64>("GetUnitField", ("player", "modCastSpeed"))
                    .filter(|s| *s > 0.0)
                    .unwrap_or(1.0);
                self.numeric(cond, (1.0 / speed - 1.0) * 100.0)
            }
            // Pieces of an item set equipped, by the set's name: any, or compared.
            "set" => self.each(cond, |a| {
                let (name, cmps) = match a {
                    Arg::Text(name) => (name.as_str(), &[][..]),
                    Arg::Compare { name, cmps } => (name.as_str(), cmps.as_slice()),
                };
                let count = f64::from(self.set_pieces(name));
                if cmps.is_empty() {
                    count > 0.0
                } else {
                    all_cmps(count, cmps)
                }
            }),
            // A modifier: the clause's ground spell goes down at the cursor ([`super::run`]).
            "cursor" => true,
            "equipset" => self.addon_fact(cond, unit),
            _ => false,
        }
    }

    fn timers(&self) -> crate::script::CombatTimers {
        crate::script::live::ask(self.lua, &crate::script::CombatTimersQuery).unwrap_or_default()
    }

    /// The player's attack time in seconds off unit field `field` (126 main, 128 ranged).
    fn attack_time(&self, field: i64) -> f64 {
        let name = match field {
            126 => "baseAttackTime",
            127 => "offhandAttackTime",
            _ => "rangedAttackTime",
        };
        self.num("GetUnitField", ("player", name)) / 1000.0
    }

    /// `UnitStat("player", index)`'s effective value.
    fn stat(&self, index: i64) -> f64 {
        self.call::<(f64, f64)>("UnitStat", ("player", index))
            .map_or(0.0, |(_, effective)| effective)
    }

    /// An attack power getter's base plus its buffs less its debuffs.
    fn attack_power(&self, getter: &str) -> f64 {
        self.call::<(f64, f64, f64)>(getter, "player")
            .map_or(0.0, |(base, pos, neg)| base + pos + neg)
    }

    /// A fact the client keeps no copy of, read from `BenillaSmartMacro_Fact(name, unit, raw)`
    /// when the layer or an addon defines it; false otherwise.
    /// The totem-bar slot `name` names: a slot word or number, else the slot holding a totem of
    /// that name.
    fn totem_slot(&self, name: &str) -> Option<i64> {
        let lower = name.to_ascii_lowercase();
        let slot = match lower.as_str() {
            "fire" | "1" => Some(1),
            "earth" | "2" => Some(2),
            "water" | "3" => Some(3),
            "air" | "4" => Some(4),
            _ => None,
        };
        if slot.is_some() {
            return slot.filter(|&s| {
                self.call::<(bool, String)>("GetTotemInfo", s)
                    .is_some_and(|(_, n)| !n.is_empty())
            });
        }
        (1..=4).find(|&s| {
            self.call::<(bool, String)>("GetTotemInfo", s)
                .is_some_and(|(_, n)| !n.is_empty() && n.eq_ignore_ascii_case(&lower))
        })
    }

    /// How many equipped items belong to the set named `name`.
    fn set_pieces(&self, name: &str) -> u32 {
        let mut count = 0;
        for slot in 1..=19 {
            let Some(link) = self.call::<String>("GetInventoryItemLink", ("player", slot)) else {
                continue;
            };
            let Some(id) = item_id_of(&link) else {
                continue;
            };
            let set = self
                .call::<(Option<i64>, Option<String>)>("BenillaItemSet", id)
                .and_then(|(_, n)| n);
            if set.is_some_and(|n| n.eq_ignore_ascii_case(name)) {
                count += 1;
            }
        }
        count
    }

    fn addon_fact(&self, cond: &Cond, unit: &str) -> bool {
        self.truthy(
            "BenillaSmartMacro_Fact",
            (cond.name, unit, cond.raw.as_str(), self.action),
        )
    }
}
