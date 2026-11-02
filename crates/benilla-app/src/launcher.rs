//! The login screen's launcher actions ([`benilla_ui::script::LauncherAsk`]), carried out at the
//! frame after the click: emptying the saved template caches ([`crate::wdb::clear_saved`]) and
//! opening a config folder in the system's file browser. Deviation: 1.12.1 has neither.

use benilla_ui::script::{LauncherAsk, LauncherFolder, UiScript};
use bevy::prelude::*;

pub(crate) struct LauncherPlugin;

impl Plugin for LauncherPlugin {
    fn build(&self, app: &mut App) {
        // Last, after the frame's clicks and every other system: it fires no event.
        app.add_systems(Last, drain_launcher);
    }
}

fn drain_launcher(world: &mut World) {
    let asks = match world.get_non_send_resource_mut::<UiScript>() {
        Some(mut script) => script.take_launcher_asks(),
        None => return,
    };
    for ask in asks {
        match ask {
            LauncherAsk::ClearCache => crate::wdb::clear_saved(world),
            LauncherAsk::OpenFolder(folder) => open_folder(folder),
        }
    }
}

/// The folder under `benilla-config`, made if missing, opened in the file browser.
fn open_folder(folder: LauncherFolder) {
    let path = match folder {
        LauncherFolder::AddOns => crate::local_state::home().map(|h| h.join("AddOns")),
        LauncherFolder::Screenshots => crate::local_state::screenshots_dir(),
        LauncherFolder::Logs => crate::local_state::logs_dir(),
    };
    let Some(path) = path else {
        return;
    };
    if let Err(e) = std::fs::create_dir_all(&path) {
        warn!("launcher: cannot make {}: {e}", path.display());
        return;
    }
    let browser = if cfg!(target_os = "windows") {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    if let Err(e) = std::process::Command::new(browser).arg(&path).spawn() {
        warn!("launcher: cannot open {}: {e}", path.display());
    }
}
