//! perf_boost's render limits, which OctoWoW's client mods add for crowded cities and raids: other
//! players, pets, summons and totems, trash mobs and corpses past a distance are not drawn, each
//! with its own distance in combat and (for players) in a city, under the `PB_*` settings its
//! companion addon PerfBoostSettings reads and writes. The selection, the player's own pet and
//! summons, marked units and (by setting) PvP-flagged players always draw. -1 turns a limit off,
//! as it ships. Deviation: 1.12 draws every streamed unit.

use bevy::prelude::*;

use crate::net::{Guid, ObjectStore, SelfPlayer};

const FIELD_SUMMONED_BY: u16 = 12;
const FIELD_CREATED_BY: u16 = 14;
const FIELD_HEALTH: u16 = 22;
const FIELD_LEVEL: u16 = 34;
const FIELD_FLAGS: u16 = 46;
const FIELD_DYNAMIC_FLAGS: u16 = 143;
const FIELD_PLAYER_FLAGS: u16 = 190;
const UNIT_FLAG_PVP: u32 = 0x1000;
const UNIT_FLAG_IN_COMBAT: u32 = 0x8_0000;
const PLAYER_FLAG_RESTING: u32 = 0x20;
const DYNFLAG_LOOTABLE: u32 = 0x1;
/// A unit of this level or above is a boss, never trash.
const BOSS_LEVEL: u32 = 63;
/// How often the limits are re-applied, seconds.
const PERIOD: f32 = 0.25;

/// The `PB_*` settings, mirrored so the pass never reads the CVar table. A distance below 0 is no
/// limit.
#[derive(Resource, Clone, Debug, PartialEq)]
pub(crate) struct PerfBoostSettings {
    enabled: bool,
    show_all_players: bool,
    hide_all_players: bool,
    always_pvp: bool,
    always_marked: bool,
    player: f32,
    player_combat: f32,
    player_city: f32,
    trash: f32,
    trash_combat: f32,
    pet: f32,
    pet_combat: f32,
    summon: f32,
    summon_combat: f32,
    corpse: f32,
    always: Vec<String>,
    never: Vec<String>,
}

impl Default for PerfBoostSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            show_all_players: false,
            hide_all_players: false,
            always_pvp: false,
            always_marked: true,
            player: -1.0,
            player_combat: -1.0,
            player_city: -1.0,
            trash: -1.0,
            trash_combat: -1.0,
            pet: -1.0,
            pet_combat: -1.0,
            summon: -1.0,
            summon_combat: -1.0,
            corpse: -1.0,
            always: Vec::new(),
            never: Vec::new(),
        }
    }
}

fn sync_settings(cvars: Option<Res<crate::cvars::Cvars>>, mut settings: ResMut<PerfBoostSettings>) {
    let Some(cvars) = cvars.as_deref() else {
        return;
    };
    let on = |name: &str| cvars.get(name).is_some_and(|v| v.trim() == "1");
    let dist = |name: &str| {
        cvars
            .get(name)
            .and_then(|v| v.trim().parse::<f32>().ok())
            .unwrap_or(-1.0)
    };
    let names = |name: &str| -> Vec<String> {
        cvars
            .get(name)
            .unwrap_or("")
            .split(',')
            .map(|n| n.trim().to_lowercase())
            .filter(|n| !n.is_empty())
            .collect()
    };
    let now = PerfBoostSettings {
        enabled: on("PB_Enabled"),
        show_all_players: on("PB_ShowAllPlayers"),
        hide_all_players: on("PB_HideAllPlayers"),
        always_pvp: on("PB_AlwaysRenderPVP"),
        always_marked: on("PB_AlwaysRenderRaidMarks"),
        player: dist("PB_PlayerRenderDist"),
        player_combat: dist("PB_PlayerRenderDistInCombat"),
        player_city: dist("PB_PlayerRenderDistInCities"),
        trash: dist("PB_TrashUnitRenderDist"),
        trash_combat: dist("PB_TrashUnitRenderDistInCombat"),
        pet: dist("PB_PetRenderDist"),
        pet_combat: dist("PB_PetRenderDistInCombat"),
        summon: dist("PB_SummonRenderDist"),
        summon_combat: dist("PB_SummonRenderDistInCombat"),
        corpse: dist("PB_CorpseRenderDist"),
        always: names("PB_AlwaysRenderPlayers"),
        never: names("PB_NeverRenderPlayers"),
    };
    if *settings != now {
        *settings = now;
    }
}

/// What kind of unit a rule reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Player,
    Pet,
    Summon,
    Trash,
    Corpse,
    /// A boss or anything else no limit covers.
    Other,
}

fn kind_of(guid: u64, store: &ObjectStore, me: u64) -> Option<Kind> {
    let field = |i: u16| store.0.field(i).unwrap_or(0);
    let wide = |i: u16| u64::from(field(i)) | (u64::from(field(i + 1)) << 32);
    if !store.is_unit() {
        return None;
    }
    let is_player_guid = guid >> 48 == 0;
    if field(FIELD_HEALTH) == 0 {
        // A lootable corpse stays for its looter.
        return Some(if field(FIELD_DYNAMIC_FLAGS) & DYNFLAG_LOOTABLE != 0 {
            Kind::Other
        } else {
            Kind::Corpse
        });
    }
    if is_player_guid {
        return Some(Kind::Player);
    }
    let (summoner, creator) = (wide(FIELD_SUMMONED_BY), wide(FIELD_CREATED_BY));
    if summoner == me || creator == me {
        return Some(Kind::Other);
    }
    if creator != 0 {
        return Some(Kind::Summon);
    }
    if summoner != 0 {
        return Some(Kind::Pet);
    }
    if field(FIELD_LEVEL) < BOSS_LEVEL {
        return Some(Kind::Trash);
    }
    Some(Kind::Other)
}

impl PerfBoostSettings {
    /// The distance limit for `kind`, `None` for none.
    fn limit(&self, kind: Kind, combat: bool, city: bool) -> Option<f32> {
        let pick = |normal: f32, fight: f32| {
            if combat && fight >= 0.0 {
                fight
            } else {
                normal
            }
        };
        let d = match kind {
            Kind::Player if combat && self.player_combat >= 0.0 => self.player_combat,
            Kind::Player if city && self.player_city >= 0.0 => self.player_city,
            Kind::Player => self.player,
            Kind::Pet => pick(self.pet, self.pet_combat),
            Kind::Summon => pick(self.summon, self.summon_combat),
            Kind::Trash => pick(self.trash, self.trash_combat),
            Kind::Corpse => self.corpse,
            Kind::Other => -1.0,
        };
        (d >= 0.0).then_some(d)
    }
}

/// A unit perf_boost holds back from drawing.
#[derive(Component)]
pub(crate) struct PerfHidden;

/// What the pass reads of each other unit, and the visibility it sets.
type UnitRow = (
    Entity,
    &'static Guid,
    &'static ObjectStore,
    &'static GlobalTransform,
    &'static mut Visibility,
    Has<PerfHidden>,
);

/// Re-apply the limits a few times a second: each unit's kind and distance decide, the exempt
/// ones aside, and a unit that stops qualifying draws again.
#[allow(clippy::too_many_arguments)]
fn apply_limits(
    time: Res<Time>,
    mut wait: Local<f32>,
    settings: Res<PerfBoostSettings>,
    selection: Option<Res<crate::target::Selection>>,
    me: Query<(&Guid, &ObjectStore, &GlobalTransform), With<SelfPlayer>>,
    names: Option<Res<crate::names::NameCache>>,
    marks: Option<Res<crate::raid_marks::RaidMarks>>,
    mut units: Query<UnitRow, Without<SelfPlayer>>,
    mut commands: Commands,
) {
    *wait -= time.delta_secs();
    if *wait > 0.0 {
        return;
    }
    *wait = PERIOD;
    let Ok((my_guid, my_store, my_pos)) = me.single() else {
        return;
    };
    let my_guid = my_guid.0;
    let my_flags = my_store.0.field(FIELD_FLAGS).unwrap_or(0);
    let combat = my_flags & UNIT_FLAG_IN_COMBAT != 0;
    let city = my_store.0.field(FIELD_PLAYER_FLAGS).unwrap_or(0) & PLAYER_FLAG_RESTING != 0;
    let target = selection.as_deref().and_then(|s| s.guid);
    let my_pos = my_pos.translation();
    for (entity, guid, store, pos, mut visibility, hidden) in &mut units {
        let hide = settings.enabled
            && Some(guid.0) != target
            && !(settings.always_marked && marks.as_deref().is_some_and(|m| m.marks(entity)))
            && kind_of(guid.0, store, my_guid).is_some_and(|kind| {
                if kind == Kind::Player {
                    let name = names
                        .as_deref()
                        .and_then(|n| n.resolve_player(guid.0, crate::query_cache::Ask::Read))
                        .map(str::to_lowercase);
                    let listed = |list: &[String]| name.as_ref().is_some_and(|n| list.contains(n));
                    if settings.show_all_players || listed(&settings.always) {
                        return false;
                    }
                    if settings.hide_all_players || listed(&settings.never) {
                        return true;
                    }
                    if settings.always_pvp
                        && store.0.field(FIELD_FLAGS).unwrap_or(0) & UNIT_FLAG_PVP != 0
                    {
                        return false;
                    }
                }
                settings
                    .limit(kind, combat, city)
                    .is_some_and(|d| pos.translation().distance(my_pos) > d)
            });
        if hide && !hidden {
            *visibility = Visibility::Hidden;
            commands.entity(entity).insert(PerfHidden);
        } else if !hide && hidden {
            *visibility = Visibility::Inherited;
            commands.entity(entity).remove::<PerfHidden>();
        }
    }
}

pub(crate) struct PerfBoostPlugin;

impl Plugin for PerfBoostPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PerfBoostSettings>()
            .add_systems(PreUpdate, sync_settings)
            .add_systems(PostUpdate, apply_limits.before(TransformSystems::Propagate));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combat_and_city_distances_take_priority_over_the_default() {
        let s = PerfBoostSettings {
            player: 60.0,
            player_combat: 20.0,
            player_city: 30.0,
            pet: 40.0,
            ..PerfBoostSettings::default()
        };
        assert_eq!(s.limit(Kind::Player, false, false), Some(60.0));
        assert_eq!(s.limit(Kind::Player, true, true), Some(20.0));
        assert_eq!(s.limit(Kind::Player, false, true), Some(30.0));
        assert_eq!(
            s.limit(Kind::Pet, true, false),
            Some(40.0),
            "no combat limit set"
        );
        assert_eq!(s.limit(Kind::Trash, false, false), None, "-1 is no limit");
        assert_eq!(s.limit(Kind::Other, true, false), None);
    }
}
