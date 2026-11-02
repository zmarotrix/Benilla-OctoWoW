//! The player's choice of the world's visual time of day: the server's clock (the reference's
//! only source), the computer's local clock, or a fixed time. It feeds only the lighting
//! ([`benilla_world::lighting::WorldTime`], through [`crate::net::game_time::publish_world_time`]):
//! the sky, sun, moons, fog and light colours. The in-game clock (`GetGameTime`), and everything
//! the server sees, keep the server's time.
//!
//! Deviation: 1.12 always draws the server's time; SuperWoW 3.0's `ForceTimeOfDay` and modern
//! clients' local-time option are the precedent.

use bevy::prelude::*;

/// `timeOfDay`: `0` server, `1` local, `2` static.
pub(crate) const CVAR_MODE: &str = "timeOfDay";
/// `timeOfDayStatic`: the static mode's minute of the day, `0..1440`.
pub(crate) const CVAR_STATIC: &str = "timeOfDayStatic";
/// The static minute's default, noon.
pub(crate) const DEFAULT_STATIC_MINUTE: u32 = 720;

/// Where the world's visual time of day comes from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum TimeOfDayMode {
    /// The server's game clock, as 1.12 draws it.
    #[default]
    Server,
    /// The computer's local wall clock.
    Local,
    /// A fixed minute of the day ([`TimeOfDaySetting::static_minute`]).
    Static,
}

impl TimeOfDayMode {
    /// The CVar's value, an unknown one reading as the server's.
    fn from_cvar(value: &str) -> Self {
        match value.trim() {
            "1" => Self::Local,
            "2" => Self::Static,
            _ => Self::Server,
        }
    }
}

/// The two settings, as their CVars hold them.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TimeOfDaySetting {
    pub(crate) mode: TimeOfDayMode,
    pub(crate) static_minute: u32,
}

impl Default for TimeOfDaySetting {
    fn default() -> Self {
        Self {
            mode: TimeOfDayMode::Server,
            static_minute: DEFAULT_STATIC_MINUTE,
        }
    }
}

impl TimeOfDaySetting {
    /// The minute of the day (fractional, `0..1440`) the lighting draws in place of the server's,
    /// `None` in server mode. `local` reads the computer's clock, called only in local mode.
    pub(crate) fn override_minute(&self, local: impl FnOnce() -> f32) -> Option<f32> {
        match self.mode {
            TimeOfDayMode::Server => None,
            TimeOfDayMode::Local => Some(local().rem_euclid(1440.0)),
            TimeOfDayMode::Static => Some(self.static_minute.min(1439) as f32),
        }
    }
}

/// The computer's local minute of the day, seconds included.
pub(crate) fn local_minute() -> f32 {
    let (civil, _) = benilla_ui::civil::local(benilla_ui::civil::unix_seconds());
    (civil.hour * 60 + civil.min) as f32 + civil.sec as f32 / 60.0
}

/// The two CVars' change callback.
pub(crate) fn on_cvar(ev: &crate::cvars::CvarChanged, mut setting: Mut<'_, TimeOfDaySetting>) {
    if ev.is(CVAR_MODE) {
        setting.mode = TimeOfDayMode::from_cvar(&ev.new);
    } else if ev.is(CVAR_STATIC) {
        let minute = ev
            .new
            .trim()
            .parse::<f32>()
            .unwrap_or(DEFAULT_STATIC_MINUTE as f32);
        setting.static_minute = minute.clamp(0.0, 1439.0) as u32;
    }
}

pub(crate) struct TimeOfDayPlugin;

impl Plugin for TimeOfDayPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TimeOfDaySetting>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_mode_picks_its_minute() {
        let server = TimeOfDaySetting::default();
        assert_eq!(server.override_minute(|| 100.0), None);
        let local = TimeOfDaySetting {
            mode: TimeOfDayMode::Local,
            ..Default::default()
        };
        assert_eq!(local.override_minute(|| 100.5), Some(100.5));
        assert_eq!(local.override_minute(|| 1500.0), Some(60.0));
        let fixed = TimeOfDaySetting {
            mode: TimeOfDayMode::Static,
            static_minute: 1320,
        };
        assert_eq!(fixed.override_minute(|| 100.0), Some(1320.0));
    }

    #[test]
    fn an_unknown_mode_is_the_servers() {
        assert_eq!(TimeOfDayMode::from_cvar("1"), TimeOfDayMode::Local);
        assert_eq!(TimeOfDayMode::from_cvar(" 2 "), TimeOfDayMode::Static);
        assert_eq!(TimeOfDayMode::from_cvar("0"), TimeOfDayMode::Server);
        assert_eq!(TimeOfDayMode::from_cvar("x"), TimeOfDayMode::Server);
    }
}
