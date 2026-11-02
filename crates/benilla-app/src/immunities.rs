//! Creature immunities, learned as SuperCleveRoidMacros learns them for `[immune]`: each of our
//! spells a creature answers with an immune miss marks that creature template immune to the
//! spell's school, and to its mechanic when it has one (a stun, a fear). Kept by creature entry in
//! `benilla-config/immunities.tsv`, so what one fight learned holds in the next. Deviation: the
//! reference learns nothing.

use std::collections::HashMap;

use benilla_protocol::{SessionEvent, SessionEventKind};
use bevy::prelude::*;

use benilla_ui::script::live::LiveAsk;
use benilla_ui::script::ImmuneQuery;

use crate::net::{NetHandlerApp, SelfGuid};
use crate::ui_action::Spells;

/// `SpellMissInfo`'s two immune answers.
const MISS_IMMUNE: [u8; 2] = [7, 8];
/// `Spell.dbc`'s `Mechanic` column.
const COL_MECHANIC: usize = 5;
/// A creature's guid carries its template entry in bits 24..48 under this high word.
const HIGH_CREATURE: u64 = 0xF130;

/// What a creature template is known immune to: schools as a `1 << School` mask, and mechanics.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Known {
    schools: u32,
    mechanics: Vec<u32>,
}

#[derive(Resource, Default)]
pub(crate) struct Immunities(HashMap<u32, Known>);

impl Immunities {
    fn from_tsv(text: &str) -> Self {
        let mut map = HashMap::new();
        for line in text.lines() {
            let mut cols = line.split('\t');
            let (Some(entry), Some(schools)) = (cols.next(), cols.next()) else {
                continue;
            };
            let (Ok(entry), Ok(schools)) = (entry.parse(), schools.parse()) else {
                continue;
            };
            let mechanics = cols
                .next()
                .unwrap_or("")
                .split(',')
                .filter_map(|m| m.parse().ok())
                .collect();
            map.insert(entry, Known { schools, mechanics });
        }
        Self(map)
    }

    fn to_tsv(&self) -> String {
        let mut entries: Vec<_> = self.0.iter().collect();
        entries.sort_unstable_by_key(|(e, _)| **e);
        entries
            .into_iter()
            .map(|(entry, k)| {
                let mechanics: Vec<String> = k.mechanics.iter().map(u32::to_string).collect();
                format!("{entry}\t{}\t{}\n", k.schools, mechanics.join(","))
            })
            .collect()
    }

    /// Learn that `entry` is immune to a spell of `school` and `mechanic` (0 for none); whether
    /// that was news.
    fn learn(&mut self, entry: u32, school: u32, mechanic: u32) -> bool {
        let known = self.0.entry(entry).or_default();
        let before = known.clone();
        if mechanic != 0 {
            if !known.mechanics.contains(&mechanic) {
                known.mechanics.push(mechanic);
            }
        } else if school < 32 {
            known.schools |= 1 << school;
        }
        *known != before
    }

    fn immune(&self, entry: u32, school: Option<u32>, mechanic: u32) -> bool {
        let Some(known) = self.0.get(&entry) else {
            return false;
        };
        (mechanic != 0 && known.mechanics.contains(&mechanic))
            || school.is_some_and(|s| s < 32 && known.schools & (1 << s) != 0)
    }
}

/// The creature template entry a guid carries, `None` for anything not a creature.
fn creature_entry(guid: u64) -> Option<u32> {
    (guid >> 48 == HIGH_CREATURE).then_some(((guid >> 24) & 0xFF_FFFF) as u32)
}

fn path() -> Option<std::path::PathBuf> {
    crate::local_state::home().map(|h| h.join("immunities.tsv"))
}

fn load(mut known: ResMut<Immunities>) {
    if let Some(text) = path().and_then(|p| std::fs::read_to_string(p).ok()) {
        *known = Immunities::from_tsv(&text);
    }
}

fn on_miss(
    In(ev): In<SessionEvent>,
    me: Res<SelfGuid>,
    spells: Option<Res<Spells>>,
    mut known: ResMut<Immunities>,
) {
    let SessionEvent::SpellLogMiss(m) = ev else {
        return;
    };
    if me.0 != Some(m.caster) {
        return;
    }
    let Some(spells) = spells.as_deref() else {
        return;
    };
    let school = spells.catalog.get(m.spell_id).map_or(0, |d| d.school);
    let mechanic = spells
        .catalog
        .row(m.spell_id)
        .and_then(|r| r.columns.get(COL_MECHANIC).copied())
        .unwrap_or(0);
    let mut learned = false;
    for &(target, info) in &m.misses {
        if let (true, Some(entry)) = (MISS_IMMUNE.contains(&info), creature_entry(target)) {
            learned |= known.learn(entry, school, mechanic);
        }
    }
    if learned {
        if let Some(p) = path() {
            if let Err(e) = crate::local_state::write_atomic(&p, &known.to_tsv()) {
                warn!("immunities: cannot write {}: {e}", p.display());
            }
        }
    }
}

/// What the `[immune]` read looks at.
#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct ImmuneLive<'w> {
    known: Res<'w, Immunities>,
    spells: Option<Res<'w, Spells>>,
}

impl crate::script_calls::live::LiveFamily for ImmuneLive<'_> {
    fn answer(&mut self, ask: &mut LiveAsk<'_>) -> bool {
        ask.answer(|q: &ImmuneQuery| {
            let Some(entry) = creature_entry(q.unit) else {
                return Some(false);
            };
            let (school, mechanic) = match q.spell {
                Some(spell) => {
                    let spells = self.spells.as_deref();
                    (
                        spells.and_then(|s| s.catalog.get(spell)).map(|d| d.school),
                        spells
                            .and_then(|s| s.catalog.row(spell))
                            .and_then(|r| r.columns.get(COL_MECHANIC).copied())
                            .unwrap_or(0),
                    )
                }
                None => (q.school, 0),
            };
            Some(self.known.immune(entry, school, mechanic))
        })
    }
}

pub(crate) struct ImmunitiesPlugin;

impl Plugin for ImmunitiesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Immunities>()
            .add_systems(Startup, load)
            .net_handler(SessionEventKind::SpellLogMiss, on_miss);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_immune_miss_marks_the_school_or_the_mechanic() {
        let mut k = Immunities::default();
        assert!(k.learn(1234, 2, 0), "fire");
        assert!(!k.learn(1234, 2, 0), "already known");
        assert!(k.learn(1234, 4, 12), "a stun");
        assert!(k.immune(1234, Some(2), 0));
        assert!(!k.immune(1234, Some(4), 0), "frost damage still lands");
        assert!(k.immune(1234, Some(0), 12), "any stun");
        assert!(!k.immune(99, Some(2), 0));
    }

    #[test]
    fn the_table_round_trips_and_creature_guids_carry_their_entry() {
        let mut k = Immunities::default();
        k.learn(1234, 2, 0);
        k.learn(1234, 0, 5);
        assert_eq!(Immunities::from_tsv(&k.to_tsv()).0, k.0);
        assert_eq!(creature_entry(0xF130_0004_D200_0042), Some(0x4D2));
        assert_eq!(creature_entry(0x0000_0000_0000_0042), None);
    }
}
