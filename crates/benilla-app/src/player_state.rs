//! The player's state as OctoWoW's client mods read it (SuperWoW's `IsMounted`, `IsSwimming`,
//! `isIndoors`, `GetSpeed`; nampower's `PlayerIsMoving`, `PlayerIsRooted`, `PlayerIsSwimming`;
//! ClassicAPI's `UnitStandState`), answered off the mover and the player's fields
//! ([`benilla_ui::script::PlayerStateQuery`]). Deviation: 1.12.1 exposes none of these.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use benilla_ui::script::live::LiveAsk;
use benilla_ui::script::{CursorWorldQuery, KeyDownQuery, PlayerState, PlayerStateQuery};

use crate::net::{ObjectStore, SelfPlayer};
use crate::player::Player;

#[derive(SystemParam)]
pub(crate) struct PlayerStateLive<'w, 's> {
    player: Option<Res<'w, Player>>,
    outdoors: Option<Res<'w, crate::player::PlayerOutdoors>>,
    store: Query<'w, 's, &'static ObjectStore, With<SelfPlayer>>,
    keys: Option<Res<'w, ButtonInput<KeyCode>>>,
    /// The world point under the mouse, SuperWoW's `CursorPosition`.
    under_mouse: Option<Res<'w, crate::target::PickOcclusion>>,
}

/// A WoW key name's keys: letters, digits, `F1`-`F12`, the named keys and either side of a
/// modifier.
fn key_codes(name: &str) -> Vec<KeyCode> {
    use KeyCode as K;
    let letter = |c: char| -> Option<KeyCode> {
        Some(match c {
            'A' => K::KeyA,
            'B' => K::KeyB,
            'C' => K::KeyC,
            'D' => K::KeyD,
            'E' => K::KeyE,
            'F' => K::KeyF,
            'G' => K::KeyG,
            'H' => K::KeyH,
            'I' => K::KeyI,
            'J' => K::KeyJ,
            'K' => K::KeyK,
            'L' => K::KeyL,
            'M' => K::KeyM,
            'N' => K::KeyN,
            'O' => K::KeyO,
            'P' => K::KeyP,
            'Q' => K::KeyQ,
            'R' => K::KeyR,
            'S' => K::KeyS,
            'T' => K::KeyT,
            'U' => K::KeyU,
            'V' => K::KeyV,
            'W' => K::KeyW,
            'X' => K::KeyX,
            'Y' => K::KeyY,
            'Z' => K::KeyZ,
            '0' => K::Digit0,
            '1' => K::Digit1,
            '2' => K::Digit2,
            '3' => K::Digit3,
            '4' => K::Digit4,
            '5' => K::Digit5,
            '6' => K::Digit6,
            '7' => K::Digit7,
            '8' => K::Digit8,
            '9' => K::Digit9,
            _ => return None,
        })
    };
    match name {
        "SPACE" => vec![K::Space],
        "ENTER" => vec![K::Enter],
        "TAB" => vec![K::Tab],
        "ESCAPE" => vec![K::Escape],
        "BACKSPACE" => vec![K::Backspace],
        "UP" => vec![K::ArrowUp],
        "DOWN" => vec![K::ArrowDown],
        "LEFT" => vec![K::ArrowLeft],
        "RIGHT" => vec![K::ArrowRight],
        "SHIFT" => vec![K::ShiftLeft, K::ShiftRight],
        "LSHIFT" => vec![K::ShiftLeft],
        "RSHIFT" => vec![K::ShiftRight],
        "CTRL" => vec![K::ControlLeft, K::ControlRight],
        "LCTRL" => vec![K::ControlLeft],
        "RCTRL" => vec![K::ControlRight],
        "ALT" => vec![K::AltLeft, K::AltRight],
        "LALT" => vec![K::AltLeft],
        "RALT" => vec![K::AltRight],
        f if f.starts_with('F') && f.len() > 1 => {
            let n: u8 = f[1..].parse().unwrap_or(0);
            [
                K::F1,
                K::F2,
                K::F3,
                K::F4,
                K::F5,
                K::F6,
                K::F7,
                K::F8,
                K::F9,
                K::F10,
                K::F11,
                K::F12,
            ]
            .get(usize::from(n.wrapping_sub(1)))
            .copied()
            .into_iter()
            .collect()
        }
        one if one.chars().count() == 1 => {
            one.chars().next().and_then(letter).into_iter().collect()
        }
        _ => Vec::new(),
    }
}

impl PlayerStateLive<'_, '_> {
    fn state(&self) -> Option<PlayerState> {
        let player = self.player.as_deref()?;
        let (rooted, swimming, speed) = player.motion_state();
        let fields = self.store.single().ok().map(|s| &s.0);
        Some(PlayerState {
            mounted: fields.is_some_and(|f| f.unit_mount_display_id() != 0),
            swimming,
            indoors: !self.outdoors.as_deref().is_some_and(|o| o.0),
            rooted,
            speed,
            stand_state: fields.map_or(0, |f| f.unit_stand_state()),
        })
    }
}

impl crate::script_calls::live::LiveFamily for PlayerStateLive<'_, '_> {
    fn answer(&mut self, ask: &mut LiveAsk<'_>) -> bool {
        ask.answer(|_: &PlayerStateQuery| Some(self.state()))
            || ask.answer(|q: &KeyDownQuery| {
                let keys = self.keys.as_deref()?;
                Some(key_codes(&q.0).iter().any(|k| keys.pressed(*k)))
            })
            || ask.answer(|_: &CursorWorldQuery| {
                let point = self.under_mouse.as_deref()?.point;
                Some(point.map(|p| benilla_assets::coords::bevy_to_wow(p)))
            })
    }
}
