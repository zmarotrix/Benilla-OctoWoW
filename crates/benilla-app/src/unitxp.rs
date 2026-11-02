//! UnitXP SP3's reads, which OctoWoW's client mods add for pfUI, HealersMate and pfQuest: any
//! held unit's world position and facing, and a line-of-sight test between two points through the
//! world's collision ([`benilla_ui::script`]'s UnitXP queries). Deviation: 1.12.1 has neither.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use benilla_assets::coords::{bevy_to_wow, wow_to_bevy};
use benilla_ui::script::live::LiveAsk;
use benilla_ui::script::{LineOfSightQuery, NearbyUnitsQuery, UnitBody, UnitBodyQuery};

use crate::net::SelfPlayer;
use crate::player::Player;

/// The height over a unit's feet its sight is tested from, both ends: about a humanoid's eyes.
const EYE: f32 = 2.0;

/// What UnitXP's reads look at.
#[derive(SystemParam)]
pub(crate) struct UnitXpLive<'w, 's> {
    index: Option<Res<'w, crate::net::GuidIndex>>,
    me: Query<'w, 's, Entity, With<SelfPlayer>>,
    player: Option<Res<'w, Player>>,
    transforms: bevy::transform::helper::TransformHelper<'w, 's>,
    stores: Query<'w, 's, &'static crate::net::ObjectStore>,
    /// Absent where the world has no collision, which answers no sight test.
    collide: Option<benilla_world::collision::WorldCollision<'w, 's>>,
}

impl UnitXpLive<'_, '_> {
    /// `guid`'s position (raw WoW) and facing (radians, WoW's: 0 north, counter-clockwise).
    fn body(&self, guid: u64) -> Option<UnitBody> {
        let entity = self.index.as_deref()?.0.get(&guid).copied()?;
        if self.me.single().ok() == Some(entity) {
            let player = self.player.as_deref()?;
            return Some(UnitBody {
                position: bevy_to_wow(player.pos),
                facing: player.facing(),
            });
        }
        let at = self.transforms.compute_global_transform(entity).ok()?;
        let forward = at.rotation() * Vec3::NEG_Z;
        let [x, y, _] = bevy_to_wow(forward);
        Some(UnitBody {
            position: bevy_to_wow(at.translation()),
            facing: y.atan2(x),
        })
    }

    /// Every held unit within `radius` of the player, nearest first.
    fn nearby(&self, radius: f32) -> Vec<u64> {
        let Some(index) = self.index.as_deref() else {
            return Vec::new();
        };
        let Some(me) = self.player.as_deref().map(|p| p.pos) else {
            return Vec::new();
        };
        let me_entity = self.me.single().ok();
        let mut out: Vec<(f32, u64)> = index
            .0
            .iter()
            .filter(|(_, &e)| Some(e) != me_entity)
            .filter(|(_, &e)| self.stores.get(e).is_ok_and(|s| s.is_unit()))
            .filter_map(|(&guid, &e)| {
                let at = self
                    .transforms
                    .compute_global_transform(e)
                    .ok()?
                    .translation();
                let d = at.distance(me);
                (d <= radius).then_some((d, guid))
            })
            .collect();
        out.sort_by(|a, b| a.0.total_cmp(&b.0));
        out.into_iter().map(|(_, g)| g).collect()
    }

    /// Whether the world leaves the eye-height segment between two feet positions open.
    fn sight(&self, from: [f32; 3], to: [f32; 3]) -> Option<bool> {
        let collide = self.collide.as_ref()?;
        let a = wow_to_bevy(from) + Vec3::Y * EYE;
        let b = wow_to_bevy(to) + Vec3::Y * EYE;
        Some(collide.cast_camera(a, b - a, false).is_none())
    }
}

impl crate::script_calls::live::LiveFamily for UnitXpLive<'_, '_> {
    fn answer(&mut self, ask: &mut LiveAsk<'_>) -> bool {
        ask.answer(|q: &UnitBodyQuery| Some(self.body(q.0)))
            || ask.answer(|q: &LineOfSightQuery| Some(self.sight(q.from, q.to)))
            || ask.answer(|q: &NearbyUnitsQuery| Some(self.nearby(q.0)))
    }
}
