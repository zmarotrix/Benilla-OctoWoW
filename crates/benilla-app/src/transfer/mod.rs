//! Moving a player's settings and addons between 1.12 clients, for the `benilla-transfer` tool:
//! a normal client into benilla ([`import_client`]), benilla out to a normal client
//! ([`export_client`]), and one `benilla-config` into another ([`copy_config`]), on another PC or
//! another OS. Each direction first puts what it replaces in a `*-backup-<time>` folder, and
//! writes what it did to a log. Nothing here runs inside the game: the tool calls it.
//!
//! Deviation: the reference has no such tool; its `WTF` and `Interface\AddOns` are copied by hand.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::local_state;
use crate::wtf_import::{child, copy_tree, file_name, subdirs, time_stamp};

mod copy;
mod export;

#[cfg(test)]
mod tests;

pub use copy::copy_config;
pub use export::export_client;

/// What one run did: a line per step, and where what it replaced was put.
#[derive(Debug, Default)]
pub struct Report {
    pub lines: Vec<String>,
    pub backup: Option<PathBuf>,
}

/// The `benilla-config` folder the tool works on: `dir` when given (through `$BENILLA_HOME`, the
/// first step of the folder's resolution), else the game's own. Call before any other function
/// here, while the tool is still one thread.
pub fn use_home(dir: Option<&Path>) -> Option<PathBuf> {
    if let Some(dir) = dir {
        std::env::set_var("BENILLA_HOME", dir);
    }
    local_state::home()
}

/// A normal client's settings and addons into benilla: its `WTF` through the launch importer's
/// steps ([`crate::wtf_import`]), read in place and left as it is, and each folder of its
/// `Interface/AddOns` into `benilla-config/AddOns`. `wow` is the client's folder; its `WTF` or
/// `Interface` folder is taken for it too.
pub fn import_client(wow: &Path) -> Result<Report, String> {
    let home = local_state::home().ok_or("benilla has no settings folder here")?;
    let wow = client_root(wow);
    let wtf = child(&wow, "WTF");
    let addons = child(&child(&wow, "Interface"), "AddOns");
    if !wtf.is_dir() && !addons.is_dir() {
        return Err(format!(
            "{} has no WTF or Interface/AddOns folder",
            wow.display()
        ));
    }
    refuse_overlap(&wow, &home)?;
    let backup = backup_dir(&home, "import-backup");
    let mut report = Report {
        backup: Some(backup.clone()),
        ..Report::default()
    };
    let log = &mut report.lines;
    log.push(format!("import from {}", wow.display()));
    if wtf.is_dir() {
        crate::wtf_import::backup(&home, &backup, log);
        crate::wtf_import::import(&wtf, log);
    }
    if addons.is_dir() {
        copy_addons(&addons, &home.join("AddOns"), &backup.join("AddOns"), log);
    }
    write_log(&home.join("transfer.log"), &report);
    Ok(report)
}

/// `<dir>/<prefix>-<time>`, or with `-2`, `-3`… after it when a run in the same second has it.
fn backup_dir(dir: &Path, prefix: &str) -> PathBuf {
    let base = format!("{prefix}-{}", time_stamp());
    (1..)
        .map(|n| match n {
            1 => dir.join(&base),
            n => dir.join(format!("{base}-{n}")),
        })
        .find(|p| !p.exists())
        .expect("some name is free")
}

/// The client folder for a path the player picked: the folder itself, or the one holding it when
/// they picked its `WTF`, `Interface` or `Interface/AddOns`.
fn client_root(picked: &Path) -> PathBuf {
    let name = file_name(picked);
    let up = |p: &Path| {
        p.parent()
            .map_or_else(|| p.to_path_buf(), Path::to_path_buf)
    };
    if name.eq_ignore_ascii_case("WTF") || name.eq_ignore_ascii_case("Interface") {
        up(picked)
    } else if name.eq_ignore_ascii_case("AddOns")
        && picked
            .parent()
            .is_some_and(|p| file_name(p).eq_ignore_ascii_case("Interface"))
    {
        up(&up(picked))
    } else {
        picked.to_path_buf()
    }
}

/// An error when one folder holds the other: a copy into itself never ends, and a backup inside
/// the source would be copied with it.
fn refuse_overlap(a: &Path, b: &Path) -> Result<(), String> {
    let (a, b) = (full_path(a), full_path(b));
    if a.starts_with(&b) || b.starts_with(&a) {
        return Err(format!(
            "{} and {} are the same folder or one holds the other",
            a.display(),
            b.display()
        ));
    }
    Ok(())
}

/// `path` resolved as the system spells it: its nearest folder that exists, canonical, with the
/// rest after it, so a path not made yet compares with one that is.
fn full_path(path: &Path) -> PathBuf {
    let mut rest = Vec::new();
    let mut at = path;
    loop {
        if let Ok(full) = std::fs::canonicalize(at) {
            return rest.iter().rev().fold(full, |p, name| p.join(name));
        }
        match (at.parent(), at.file_name()) {
            (Some(parent), Some(name)) => {
                rest.push(name.to_os_string());
                at = parent;
            }
            _ => return path.to_path_buf(),
        }
    }
}

/// Each addon folder of `src` into `dest`, an AddOns root. A folder `dest` already has, in any
/// case, is moved to `backup` first, so the copy replaces it whole and no file of the old version
/// stays behind. The stock `Blizzard_` folders and hidden ones are left out.
fn copy_addons(src: &Path, dest: &Path, backup: &Path, log: &mut Vec<String>) {
    let (mut copied, mut replaced, mut files) = (0, 0, 0);
    for addon in subdirs(src) {
        let name = file_name(&addon);
        if name.starts_with('.') || name.to_ascii_lowercase().starts_with("blizzard_") {
            continue;
        }
        let existing = child(dest, &name);
        if existing.exists() {
            if let Err(e) = move_aside(&existing, &backup.join(file_name(&existing))) {
                log.push(format!(
                    "AddOns/{name}: left as it was, cannot back it up ({e})"
                ));
                continue;
            }
            replaced += 1;
        }
        files += copy_tree(&addon, &dest.join(&name));
        copied += 1;
    }
    log.push(format!(
        "AddOns: {copied} folders, {files} files ({replaced} replaced, the old ones backed up)"
    ));
}

/// Move `path` to `to`: a rename, else a copy and a removal when the two are on different drives.
fn move_aside(path: &Path, to: &Path) -> std::io::Result<()> {
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if std::fs::rename(path, to).is_ok() {
        return Ok(());
    }
    if path.is_dir() {
        copy_tree(path, to);
        std::fs::remove_dir_all(path)
    } else {
        std::fs::copy(path, to)?;
        std::fs::remove_file(path)
    }
}

/// Add `realms` to [`local_state::realm_names_path`], each under its per-character key's token.
pub(crate) fn remember_realms(realms: &[String]) -> std::io::Result<()> {
    if realms.is_empty() {
        return Ok(());
    }
    let Some(path) = local_state::realm_names_path() else {
        return Ok(());
    };
    let mut names = realm_names();
    for realm in realms {
        names.insert(local_state::file_token(realm), realm.clone());
    }
    let body: String = names.iter().map(|(t, n)| format!("{t}\t{n}\n")).collect();
    local_state::write_atomic(&path, &body)
}

/// [`local_state::realm_names_path`], token to realm name.
fn realm_names() -> BTreeMap<String, String> {
    local_state::realm_names_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .unwrap_or_default()
        .lines()
        .filter_map(|line| line.split_once('\t'))
        .map(|(t, n)| (t.to_string(), n.to_string()))
        .collect()
}

/// The report as a log file; a failure to write it is a line of its own on screen only.
fn write_log(path: &Path, report: &Report) {
    let text = report.lines.join("\n") + "\n";
    if let Err(e) = local_state::write_atomic(path, &text) {
        bevy::log::warn!("transfer: cannot write {}: {e}", path.display());
    }
}
