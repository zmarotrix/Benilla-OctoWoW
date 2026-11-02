//! The conditional macro grammar SuperCleveRoidMacros popularised, compiled once per text: a
//! command's argument splits into `;` clauses, each a run of `[...]` groups (fallbacks: the first
//! that passes wins, `[]` always passes) and the action they share. A group holds an optional
//! `@unit` and conditions separated by commas or spaces; a condition is a name, `no`-negated or
//! not, with an optional `:` argument of `/`-ORed or `&`-ANDed values, each a plain value or a
//! `name` and comparisons (`>`, `<`, `>=`, `<=`, `=`, `~=`), a `#` comparing stacks. `_` reads as
//! a space and quotes group, so `"Mark of the Wild"` and `Mark_of_the_Wild` are one name. The
//! action may open with `!` (no toggle off), `~` (cancel the aura instead) or `?` (no tooltip).

use std::rc::Rc;

/// A comparison operator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Gt,
    Lt,
    Ge,
    Le,
    Eq,
    Ne,
}

impl Op {
    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            ">" => Self::Gt,
            "<" => Self::Lt,
            ">=" | "=>" => Self::Ge,
            "<=" | "=<" => Self::Le,
            "=" | "==" => Self::Eq,
            "~=" | "!=" => Self::Ne,
            _ => return None,
        })
    }

    /// `lhs op rhs`.
    pub fn test(self, lhs: f64, rhs: f64) -> bool {
        match self {
            Self::Gt => lhs > rhs,
            Self::Lt => lhs < rhs,
            Self::Ge => lhs >= rhs,
            Self::Le => lhs <= rhs,
            Self::Eq => (lhs - rhs).abs() < 1e-9,
            Self::Ne => (lhs - rhs).abs() >= 1e-9,
        }
    }
}

/// One comparison: `op amount`, against stacks with `#`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cmp {
    pub op: Op,
    pub amount: f64,
    pub stacks: bool,
}

/// One value of a condition's argument.
#[derive(Clone, Debug, PartialEq)]
pub enum Arg {
    /// A plain value: a name, a key, a unit.
    Text(String),
    /// A name (empty for the condition's own subject) and the comparisons it must all pass.
    Compare { name: String, cmps: Vec<Cmp> },
}

impl Arg {
    /// The name the value names, the text of a plain value.
    pub fn name(&self) -> &str {
        match self {
            Arg::Text(s) => s,
            Arg::Compare { name, .. } => name,
        }
    }
}

/// How a condition's values combine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Join {
    Any,
    All,
}

/// One condition: its canonical name, whether it is negated, and its argument's values (empty
/// for a bare condition, which takes the clause's action as its subject where it needs one).
#[derive(Clone, Debug, PartialEq)]
pub struct Cond {
    pub name: &'static str,
    pub negated: bool,
    pub join: Join,
    pub args: Vec<Arg>,
    /// The argument as written, after `:`, for the conditions that parse their own (`distance`'s
    /// `30:facing>1`).
    pub raw: String,
}

/// One `[...]` group: an optional `@unit` and conditions that must all pass.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Group {
    pub unit: Option<String>,
    pub conds: Vec<Cond>,
}

/// One `;` clause: its groups (none means it always passes) and its action.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Clause {
    pub groups: Vec<Group>,
    pub action: String,
    /// `!`: a toggle spell is not cast off again.
    pub no_toggle: bool,
    /// `~`: cancel the action's aura when it is up.
    pub cancel_aura: bool,
    /// `?`: the clause sets no `#showtooltip` icon.
    pub no_tooltip: bool,
}

/// A command argument, compiled.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Parsed {
    pub clauses: Vec<Clause>,
    /// Names the grammar did not know, for `/macrocheck`.
    pub unknown: Vec<String>,
}

/// The canonical name of each condition and alias SuperCleveRoidMacros accepts, a name past a
/// `no` prefix included; `None` for one it does not know. `ic`/`ooc` and `standing`/`sitting`
/// resolve with their own negation ([`resolve`]).
const NAMES: &[&str] = &[
    "bg",
    "button",
    "cdgcd",
    "channeled",
    "checkcasting",
    "checkchanneled",
    "combat",
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
    "nextslamclip",
    "rangedclip",
    "rangedtimer",
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
    "slamclip",
    "spellcasttime",
    "standing",
    "stat",
    "stealth",
    "swimming",
    "swingtimer",
    "totem",
    "usable",
    "zone",
    "alive",
    "behind",
    "buff",
    "buffcapped",
    "casting",
    "casttime",
    "cc",
    "channeltime",
    "class",
    "curse",
    "cursive",
    "dead",
    "debuff",
    "debuffcapped",
    "disease",
    "dispellable",
    "dispellablebuff",
    "distance",
    "energy",
    "exists",
    "facing",
    "harm",
    "help",
    "hp",
    "hplost",
    "immune",
    "inrange",
    "insight",
    "isnpc",
    "isplayer",
    "istank",
    "level",
    "magic",
    "magicbuff",
    "mana",
    "meleerange",
    "member",
    "multiscan",
    "mytag",
    "name",
    "othertag",
    "outrange",
    "party",
    "playercontrolled",
    "poison",
    "power",
    "powerlost",
    "powertype",
    "rage",
    "raid",
    "rawhp",
    "rawpower",
    "resisted",
    "tag",
    "targeting",
    "threat",
    "tte",
    "ttk",
    "type",
    "cursor",
    "target",
];

/// Short spellings ([`NAMES`]' aliases).
const ALIASES: &[(&str, &str)] = &[
    ("cd", "cooldown"),
    ("chan", "channeled"),
    ("chtime", "channeltime"),
    ("ct", "casttime"),
    ("dist", "distance"),
    ("eq", "equipped"),
    ("ihit", "incominghit"),
    ("los", "insight"),
    ("melee", "meleerange"),
    ("mv", "moving"),
    ("osp", "onswingpending"),
    ("queued", "queuedspell"),
    ("react", "reactive"),
    ("scast", "selfcasting"),
    ("sct", "spellcasttime"),
    ("stl", "stealth"),
    ("swim", "swimming"),
    ("tgt", "targeting"),
    ("stimer", "swingtimer"),
    ("rtimer", "rangedtimer"),
    ("stance", "form"),
    ("mouseuse", "cursor"),
    ("modifier", "mod"),
    ("harmful", "harm"),
    ("helpful", "help"),
];

/// A written name as `(canonical, negated)`, or `None` for an unknown one.
pub fn resolve(written: &str) -> Option<(&'static str, bool)> {
    let w = written.to_ascii_lowercase();
    match w.as_str() {
        "ic" => return Some(("combat", false)),
        "ooc" => return Some(("combat", true)),
        "sitting" => return Some(("standing", true)),
        // Semantic opposites read as negations of one condition.
        "outrange" => return Some(("outrange", false)),
        _ => {}
    }
    let canon = |n: &str| -> Option<&'static str> {
        NAMES
            .iter()
            .copied()
            .find(|c| *c == n)
            .or_else(|| ALIASES.iter().find(|(a, _)| *a == n).map(|(_, c)| *c))
    };
    if let Some(c) = canon(&w) {
        return Some((c, false));
    }
    // `no` + a known name; `notarget`, `nextslamclip` and the like are names of their own.
    w.strip_prefix("no").and_then(canon).map(|c| (c, true))
}

/// Split `s` on any of `seps` outside double quotes.
fn split_outside_quotes<'a>(s: &'a str, seps: &[char]) -> Vec<&'a str> {
    let mut out = Vec::new();
    let mut quoted = false;
    let mut start = 0;
    for (i, c) in s.char_indices() {
        if c == '"' {
            quoted = !quoted;
        } else if !quoted && seps.contains(&c) {
            out.push(&s[start..i]);
            start = i + c.len_utf8();
        }
    }
    out.push(&s[start..]);
    out
}

/// A name as written to the name it means: quotes dropped, `_` for a space, trimmed.
pub fn clean_name(s: &str) -> String {
    s.replace('"', "").replace('_', " ").trim().to_string()
}

/// `value` split into its name and comparisons: `"Rake<3"`, `">=4"`, `"Shield>#3"`,
/// `">20&<50"`. `None` when it holds no comparison.
fn compare(value: &str) -> Option<(String, Vec<Cmp>)> {
    let first = value.find(['>', '<', '=', '~', '!'])?;
    let name = clean_name(&value[..first]);
    let mut cmps = Vec::new();
    let mut rest = &value[first..];
    while !rest.is_empty() {
        rest = rest.trim_start_matches(|c: char| c == '&' || c == '/' || c.is_whitespace());
        let op_len = rest
            .find(|c: char| !matches!(c, '>' | '<' | '=' | '~' | '!'))
            .unwrap_or(rest.len());
        let op = Op::parse(&rest[..op_len])?;
        rest = &rest[op_len..];
        let stacks = rest.starts_with('#');
        if stacks {
            rest = &rest[1..];
        }
        let num_len = rest
            .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))
            .unwrap_or(rest.len());
        let amount: f64 = rest[..num_len].parse().ok()?;
        rest = &rest[num_len..];
        cmps.push(Cmp { op, amount, stacks });
    }
    (!cmps.is_empty()).then_some((name, cmps))
}

/// A condition's argument as values and their join: `&` joins with AND unless it only chains
/// comparisons (`>20&<50`), `/` with OR.
fn args(raw: &str) -> (Join, Vec<Arg>) {
    let chained = raw.contains('&') && {
        // `>0&<10`: an operator and number on both sides of every `&`.
        raw.split('&')
            .skip(1)
            .all(|part| part.trim_start().starts_with(['>', '<', '=', '~', '!']))
    };
    let (join, sep) = if raw.contains('&') && !raw.contains('/') && !chained {
        (Join::All, '&')
    } else {
        (Join::Any, '/')
    };
    // A comparison chain (`>20&<50`, `>1800/<2200` on one name) stays one value.
    let one_chain = |s: &str| {
        let s = s.trim_start();
        s.starts_with(['>', '<', '=', '~', '!'])
    };
    let mut values: Vec<String> = Vec::new();
    for part in split_outside_quotes(raw, &[sep]) {
        if chained || (one_chain(part) && values.last().is_some_and(|v| compare(v).is_some())) {
            if let Some(last) = values.last_mut() {
                if chained || one_chain(part) {
                    last.push(if chained { '&' } else { sep });
                    last.push_str(part);
                    continue;
                }
            }
        }
        values.push(part.to_string());
    }
    let args = values
        .into_iter()
        .map(|v| {
            let v = v.trim().replace('"', "");
            // `name#3` and `#3` are `=#3`.
            let v = match v.find('#') {
                Some(i) if !v[..i].ends_with(['>', '<', '=']) => {
                    format!("{}=#{}", &v[..i], &v[i + 1..])
                }
                _ => v,
            };
            match compare(&v) {
                Some((name, cmps)) => Arg::Compare { name, cmps },
                None => Arg::Text(clean_name(&v)),
            }
        })
        .collect();
    (join, args)
}

/// One `[...]` group's inside.
fn group(inside: &str, unknown: &mut Vec<String>) -> Group {
    let mut g = Group::default();
    for token in split_outside_quotes(inside, &[',', ' ']) {
        let token = token.trim();
        if token.is_empty() {
            continue;
        }
        if let Some(unit) = token.strip_prefix('@') {
            g.unit = Some(unit.to_string());
            continue;
        }
        let (written, raw) = match token.find(':') {
            Some(i) => (&token[..i], &token[i + 1..]),
            None => (token, ""),
        };
        let Some((name, negated)) = resolve(written) else {
            unknown.push(written.to_string());
            continue;
        };
        let (join, args) = if raw.is_empty() {
            (Join::Any, Vec::new())
        } else {
            args(raw)
        };
        g.conds.push(Cond {
            name,
            negated,
            join,
            args,
            raw: raw.to_string(),
        });
    }
    g
}

/// One `;` clause.
fn clause(text: &str, unknown: &mut Vec<String>) -> Clause {
    let mut c = Clause::default();
    let mut rest = text.trim();
    if let Some(r) = rest.strip_prefix('?') {
        c.no_tooltip = true;
        rest = r.trim_start();
    }
    while let Some(r) = rest.strip_prefix('[') {
        let Some(end) = r.find(']') else {
            break;
        };
        c.groups.push(group(&r[..end], unknown));
        rest = r[end + 1..].trim_start();
    }
    loop {
        if let Some(r) = rest.strip_prefix('!') {
            c.no_toggle = true;
            rest = r;
        } else if let Some(r) = rest.strip_prefix('~') {
            c.cancel_aura = true;
            rest = r;
        } else if let Some(r) = rest.strip_prefix('?') {
            c.no_tooltip = true;
            rest = r;
        } else {
            break;
        }
    }
    c.action = rest.trim().to_string();
    c
}

/// Compile a command's argument.
pub fn parse(text: &str) -> Parsed {
    let mut unknown = Vec::new();
    let clauses = split_outside_quotes(text, &[';'])
        .into_iter()
        .filter(|c| !c.trim().is_empty())
        .map(|c| clause(c, &mut unknown))
        .collect();
    Parsed { clauses, unknown }
}

/// The compiled cache: a macro's text is parsed once for every click and icon refresh after.
#[derive(Default)]
pub struct Cache {
    map: std::collections::HashMap<String, Rc<Parsed>>,
}

impl Cache {
    /// The compiled form of `text`, parsed on its first use.
    pub fn get(&mut self, text: &str) -> Rc<Parsed> {
        if let Some(p) = self.map.get(text) {
            return p.clone();
        }
        // A bound on a session's distinct texts: an addon building strings each frame cannot
        // grow it without end.
        if self.map.len() > 4096 {
            self.map.clear();
        }
        let p = Rc::new(parse(text));
        self.map.insert(text.to_string(), p.clone());
        p
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmp(op: Op, amount: f64, stacks: bool) -> Cmp {
        Cmp { op, amount, stacks }
    }

    #[test]
    fn clauses_groups_units_and_actions() {
        let p = parse("[mod:alt] Frostbolt; [@mouseover,help][] Rejuvenation; Blink");
        assert_eq!(p.clauses.len(), 3);
        assert_eq!(p.clauses[0].action, "Frostbolt");
        assert_eq!(p.clauses[0].groups[0].conds[0].name, "mod");
        assert_eq!(
            p.clauses[0].groups[0].conds[0].args,
            [Arg::Text("alt".into())]
        );
        assert_eq!(p.clauses[1].groups.len(), 2);
        assert_eq!(p.clauses[1].groups[0].unit.as_deref(), Some("mouseover"));
        assert!(p.clauses[1].groups[1].conds.is_empty(), "[] always passes");
        assert!(p.clauses[2].groups.is_empty());
        assert!(p.unknown.is_empty());
    }

    #[test]
    fn negation_aliases_and_opposites() {
        let g =
            &parse("[nobuff:Renew/Rejuvenation ooc ic nocd:Sprint sitting] X").clauses[0].groups[0];
        let names: Vec<_> = g.conds.iter().map(|c| (c.name, c.negated)).collect();
        assert_eq!(
            names,
            [
                ("buff", true),
                ("combat", true),
                ("combat", false),
                ("cooldown", true),
                ("standing", true)
            ]
        );
        assert_eq!(g.conds[0].join, Join::Any);
        assert_eq!(g.conds[0].args.len(), 2);
        assert_eq!(resolve("notarget"), Some(("notarget", false)));
    }

    #[test]
    fn comparisons_stacks_chains_and_and() {
        let g = &parse(
            "[hp:>20&<50 debuff:Rake<3 buff:\"Power Word: Shield\">#2 buff:X&Y combo:>=4] A",
        )
        .clauses[0]
            .groups[0];
        assert_eq!(
            g.conds[0].args,
            [Arg::Compare {
                name: String::new(),
                cmps: vec![cmp(Op::Gt, 20.0, false), cmp(Op::Lt, 50.0, false)]
            }]
        );
        assert_eq!(
            g.conds[1].args,
            [Arg::Compare {
                name: "Rake".into(),
                cmps: vec![cmp(Op::Lt, 3.0, false)]
            }]
        );
        assert_eq!(
            g.conds[2].args,
            [Arg::Compare {
                name: "Power Word: Shield".into(),
                cmps: vec![cmp(Op::Gt, 2.0, true)]
            }]
        );
        assert_eq!(g.conds[3].join, Join::All);
        assert_eq!(g.conds[3].args.len(), 2);
        assert_eq!(
            g.conds[4].args,
            [Arg::Compare {
                name: String::new(),
                cmps: vec![cmp(Op::Ge, 4.0, false)]
            }]
        );
    }

    #[test]
    fn underscores_quotes_prefixes_and_unknown_names() {
        let p = parse("[buff:Mark_of_the_Wild] !~?Prowl; [frobnicate] X");
        assert_eq!(
            p.clauses[0].groups[0].conds[0].args,
            [Arg::Text("Mark of the Wild".into())]
        );
        let c = &p.clauses[0];
        assert!(c.no_toggle && c.cancel_aura && c.no_tooltip);
        assert_eq!(c.action, "Prowl");
        assert_eq!(p.unknown, ["frobnicate"]);
    }

    #[test]
    fn the_cache_hands_back_one_compiled_form() {
        let mut cache = Cache::default();
        let a = cache.get("[combat] Charge");
        let b = cache.get("[combat] Charge");
        assert!(Rc::ptr_eq(&a, &b));
    }
}
