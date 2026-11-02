//! A normal client's `WTF` folder, dropped into `benilla-config/WTF`, taken in at launch: its
//! `Config.wtf` into `config.toml`, each account's and character's account-data files (key
//! bindings, macros, layout, chat, config) through the same reader the server's copies go through
//! ([`crate::account_data::sections::write`]), the addon enable lists, the camera poses and every
//! addon's saved variables, and the stock UI's `SavedVariables.lua`. What it replaces is copied to `benilla-config/import-backup-<time>/`
//! first, the folder is renamed `WTF-imported-<time>` so it runs once, and `wtf-import.log` says
//! what moved. Deviation: the reference reads its own `WTF` in place and has no importer.
//!
//! The `benilla-transfer` tool runs the same steps on a normal client's own `WTF`, read in place
//! ([`crate::transfer`]).

use std::path::{Path, PathBuf};

use bevy::log::{info, warn};

use crate::account_data::sections;
use crate::local_state;

/// The folders of `benilla-config` an import writes into: the backup's set, with [`TOUCHED_FILES`].
pub(crate) const TOUCHED: [&str; 8] = [
    "saved",
    "addon-state",
    "macros",
    "bindings",
    "chat",
    "layout",
    "camera",
    "config-cache",
];

/// The files of `benilla-config` an import writes.
const TOUCHED_FILES: [&str; 3] = ["config.toml", "saved-variables.lua", "realm-names.txt"];

/// The account-data files and their sections: account-wide in the account folder, per character
/// in the character's (`sections::ACCOUNT_WIDE`).
pub(crate) const ACCOUNT_FILES: [(&str, usize); 3] = [
    ("config-cache.wtf", 0),
    ("bindings-cache.wtf", 2),
    ("macros-cache.txt", 4),
];
pub(crate) const CHARACTER_FILES: [(&str, usize); 5] = [
    ("config-cache.wtf", 1),
    ("bindings-cache.wtf", 3),
    ("macros-cache.txt", 5),
    ("layout-cache.txt", sections::LAYOUT),
    ("chat-cache.txt", sections::CHAT),
];

/// The settings an import leaves out: the renderer API names the old client's DirectX or OpenGL,
/// which benilla does not use.
pub(crate) const SKIPPED_CVARS: [&str; 1] = ["gxApi"];

/// Take in `benilla-config/WTF` if it is there. Run once at launch, before the CVars load.
pub(crate) fn run() {
    let Some(home) = local_state::home() else {
        return;
    };
    let wtf = child(&home, "WTF");
    if !wtf.is_dir() {
        return;
    }
    let stamp = time_stamp();
    let mut log = Vec::new();
    backup(
        &home,
        &home.join(format!("import-backup-{stamp}")),
        &mut log,
    );
    import(&wtf, &mut log);
    let done = home.join(format!("WTF-imported-{stamp}"));
    match std::fs::rename(&wtf, &done) {
        Ok(()) => log.push(format!("renamed WTF to {}", file_name(&done))),
        Err(e) => log.push(format!("could not rename WTF ({e}); it will import again")),
    }
    let text = log.join("\n") + "\n";
    if let Err(e) = local_state::write_atomic(&home.join("wtf-import.log"), &text) {
        warn!("wtf import: cannot write its log: {e}");
    }
    info!("wtf import: {} steps, see wtf-import.log", log.len());
}

/// The import's steps, each logged.
pub(crate) fn import(wtf: &Path, log: &mut Vec<String>) {
    // `Config.wtf`: `SET name "value"` lines.
    if let Ok(text) = std::fs::read_to_string(child(wtf, "Config.wtf")) {
        let pairs = parse_config_wtf(&text);
        match crate::cvars::merge_into_config(&pairs) {
            Ok(n) => log.push(format!("Config.wtf: {n} of {} settings", pairs.len())),
            Err(e) => log.push(format!("Config.wtf: could not write config.toml ({e})")),
        }
    }
    let defaults = crate::bindings::store::key_table(crate::ui_script::default_bindings());
    let mut realms = Vec::new();
    for account in subdirs(&child(wtf, "Account")) {
        let name = file_name(&account);
        for (file, section) in ACCOUNT_FILES {
            if let Ok(bytes) = std::fs::read(child(&account, file)) {
                sections::write(section, None, &defaults, &bytes);
                log.push(format!("{name}: {file}"));
            }
        }
        if let Some(dir) = local_state::addon_saved_account_dir() {
            let n = copy_lua(&child(&account, "SavedVariables"), &dir);
            if n > 0 {
                log.push(format!("{name}: {n} addon saved-variable files"));
            }
        }
        // The stock UI's `RegisterForSave` globals, the same `NAME = value` lines benilla's file
        // keeps.
        let stock = child(&account, "SavedVariables.lua");
        if let Some(dest) = local_state::saved_variables_path() {
            if stock.is_file() && copy(&stock, &dest) {
                log.push(format!("{name}: SavedVariables.lua"));
            }
        }
        // Every other folder is a realm, holding a folder per character.
        for realm in subdirs(&account) {
            let realm_name = file_name(&realm);
            if realm_name.eq_ignore_ascii_case("SavedVariables") {
                continue;
            }
            realms.push(realm_name.clone());
            for character in subdirs(&realm) {
                let identity = (realm_name.clone(), file_name(&character));
                import_character(&character, &identity, &defaults, log);
            }
        }
    }
    // Each realm's spelling, which its per-character key does not keep, for an export back.
    if let Err(e) = crate::transfer::remember_realms(&realms) {
        log.push(format!("could not record the realm names ({e})"));
    }
}

fn import_character(
    dir: &Path,
    identity: &sections::Identity,
    defaults: &[benilla_ui::script::keybind::KeyBinding],
    log: &mut Vec<String>,
) {
    let (realm, character) = identity;
    let who = format!("{realm}/{character}");
    for (file, section) in CHARACTER_FILES {
        if let Ok(bytes) = std::fs::read(child(dir, file)) {
            sections::write(section, Some(identity), defaults, &bytes);
            log.push(format!("{who}: {file}"));
        }
    }
    for (file, dest) in [
        (
            "AddOns.txt",
            local_state::addons_state_path(realm, character),
        ),
        (
            "camera-settings.txt",
            local_state::camera_character_path(realm, character),
        ),
    ] {
        let (src, Some(dest)) = (child(dir, file), dest) else {
            continue;
        };
        if src.is_file() && copy(&src, &dest) {
            log.push(format!("{who}: {file}"));
        }
    }
    if let Some(dest) = local_state::addon_saved_character_dir(realm, character) {
        let n = copy_lua(&child(dir, "SavedVariables"), &dest);
        if n > 0 {
            log.push(format!("{who}: {n} addon saved-variable files"));
        }
    }
}

/// `SET name "value"` per line, as the reference writes `Config.wtf`; anything else is skipped.
pub(crate) fn parse_config_wtf(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| {
            let rest = line.trim().strip_prefix("SET ")?;
            let (name, value) = rest.split_once(' ')?;
            let value = value.trim().trim_matches('"');
            (!SKIPPED_CVARS.iter().any(|s| s.eq_ignore_ascii_case(name)))
                .then(|| (name.to_string(), value.to_string()))
        })
        .collect()
}

/// Each `<Addon>.lua` of `src` into `dest`, the `.bak` copies left out; how many were copied.
fn copy_lua(src: &Path, dest: &Path) -> usize {
    std::fs::read_dir(src)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("lua")))
        .filter(|p| copy(p, &dest.join(file_name(p))))
        .count()
}

pub(crate) fn copy(src: &Path, dest: &Path) -> bool {
    if let Some(parent) = dest.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match std::fs::copy(src, dest) {
        Ok(_) => true,
        Err(e) => {
            warn!("wtf import: {} -> {}: {e}", src.display(), dest.display());
            false
        }
    }
}

/// What the import may replace, copied to `to` first.
pub(crate) fn backup(home: &Path, to: &Path, log: &mut Vec<String>) {
    let mut n = 0;
    for dir in TOUCHED {
        n += copy_tree(&home.join(dir), &to.join(dir));
    }
    for file in TOUCHED_FILES {
        let src = home.join(file);
        if src.is_file() && copy(&src, &to.join(file)) {
            n += 1;
        }
    }
    log.push(format!("backed up {n} existing files to {}", file_name(to)));
}

pub(crate) fn copy_tree(src: &Path, dest: &Path) -> usize {
    let mut n = 0;
    for entry in std::fs::read_dir(src).into_iter().flatten().flatten() {
        let path = entry.path();
        let target = dest.join(file_name(&path));
        if path.is_dir() {
            n += copy_tree(&path, &target);
        } else if copy(&path, &target) {
            n += 1;
        }
    }
    n
}

/// `dir/name`, else the entry whose name matches ignoring ASCII case: a `WTF` folder copied from
/// Windows to a case-sensitive file system keeps whatever spelling its files had there.
pub(crate) fn child(dir: &Path, name: &str) -> PathBuf {
    let exact = dir.join(name);
    if exact.exists() {
        return exact;
    }
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(name))
        .map_or(exact, |e| e.path())
}

pub(crate) fn subdirs(dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    out.sort();
    out
}

pub(crate) fn file_name(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// `YYYYMMDD-HHMMSS`, local time, for the backup and the renamed folder.
pub(crate) fn time_stamp() -> String {
    let (c, _) = benilla_ui::civil::local(benilla_ui::civil::unix_seconds());
    format!(
        "{}{:02}{:02}-{:02}{:02}{:02}",
        c.year, c.month, c.day, c.hour, c.min, c.sec
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local_state::test_env::{EnvGuard, ENV_LOCK};

    fn put(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    /// A whole `WTF` tree lands where each piece is read from, what it replaces is backed up, and
    /// the folder is renamed so the next launch does not import it again.
    #[test]
    fn a_wtf_folder_is_taken_in_once() {
        let _l = ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let home = std::env::temp_dir().join(format!("benilla-wtf-import-{}", std::process::id()));
        std::fs::remove_dir_all(&home).ok();
        let _h = EnvGuard::set("BENILLA_HOME", home.to_str().unwrap());
        put(
            &home.join("config.toml"),
            "[cvars]\nSoundVolume = \"0.5\"\n",
        );
        let wtf = home.join("WTF");
        put(
            &wtf.join("Config.wtf"),
            "SET MusicVolume \"0.3\"\nSET gxApi \"d3d\"\n",
        );
        let acc = wtf.join("Account").join("VRISKA");
        put(&acc.join("SavedVariables/pfUI.lua"), "pfUI_config = {}\n");
        put(&acc.join("SavedVariables/pfUI.lua.bak"), "old\n");
        put(&acc.join("bindings-cache.wtf"), "bind F JUMP\n");
        let ch = acc.join("N'Zoth").join("Vriska");
        put(&ch.join("AddOns.txt"), "pfUI: enabled\n");
        put(&ch.join("SavedVariables/pfUI.lua"), "pfUI_char = {}\n");

        run();

        let config = std::fs::read_to_string(home.join("config.toml")).unwrap();
        assert!(config.contains("MusicVolume = \"0.3\""), "{config}");
        assert!(config.contains("SoundVolume = \"0.5\""), "kept: {config}");
        assert!(!config.contains("gxApi"), "{config}");
        assert!(home.join("saved/pfUI.lua").is_file());
        assert!(!home.join("saved/pfUI.lua.bak").exists());
        assert!(home.join("saved/N_Zoth-Vriska/pfUI.lua").is_file());
        assert!(home.join("addon-state/N_Zoth-Vriska.txt").is_file());
        let binds = std::fs::read_to_string(home.join("bindings/account.txt")).unwrap();
        assert!(binds.contains("bind JUMP F"), "{binds}");
        assert!(!wtf.exists(), "renamed so it imports once");
        let names: Vec<String> = std::fs::read_dir(&home)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert!(
            names.iter().any(|n| n.starts_with("WTF-imported-")),
            "{names:?}"
        );
        let backup = names
            .iter()
            .find(|n| n.starts_with("import-backup-"))
            .unwrap();
        assert!(home.join(backup).join("config.toml").is_file());
        assert!(home.join("wtf-import.log").is_file());
        drop(_h);
        std::fs::remove_dir_all(&home).ok();
    }

    /// A name spelled in another case is found, on a case-sensitive file system too.
    #[test]
    fn a_child_is_found_whatever_its_case() {
        let dir = std::env::temp_dir().join(format!("benilla-wtf-child-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        put(&dir.join("config.WTF"), "");
        std::fs::create_dir_all(dir.join("savedvariables")).unwrap();
        assert!(child(&dir, "Config.wtf").is_file());
        assert!(child(&dir, "SavedVariables").is_dir());
        assert_eq!(child(&dir, "Missing.txt"), dir.join("Missing.txt"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn config_wtf_lines_become_pairs_and_the_renderer_api_is_left_out() {
        let text =
            "SET realmList \"logon.octowow.st\"\nSET gxApi \"d3d\"\nSET MusicVolume \"0.3\"\n\
                    garbage\nSET accountName \"VRISKA\"\n";
        assert_eq!(
            parse_config_wtf(text),
            [
                ("realmList".to_string(), "logon.octowow.st".to_string()),
                ("MusicVolume".to_string(), "0.3".to_string()),
                ("accountName".to_string(), "VRISKA".to_string()),
            ]
        );
    }
}
