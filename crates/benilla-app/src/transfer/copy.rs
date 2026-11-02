//! One `benilla-config` into another: to a USB stick and from it on another PC, or between a
//! Windows and a Linux install. A merge: a file the destination has and the source does not stays.
//! The differences between the systems it handles:
//! - Case: Windows file names ignore it, Linux's do not. Two source names that differ only by case
//!   cannot both land on a folder that ignores it, so the second is skipped and logged; and below
//!   the top level an entry the destination spells another way is the same entry, so a folder
//!   copied from Windows does not end up twice on Linux.
//! - Names Windows cannot hold (`<>:"|?*`, a trailing dot or space, `CON` and the other device
//!   names) are skipped and logged when the destination is on Windows.
//! - Links are not followed, and the folders that belong to one machine (the template cache, the
//!   diagnostics, earlier backups) are left out.
//!
//! The files themselves move as their bytes: nothing in them names a path of the machine.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::wtf_import::{child, file_name};

use super::{move_aside, refuse_overlap, write_log, Report};

/// Top-level entries left out: rebuilt where they run, or a record of one machine.
const LEFT_OUT: [&str; 5] = [
    "cache",
    "Diagnostics",
    "wtf-import.log",
    "transfer.log",
    ".case-probe",
];

/// Top-level prefixes left out: the backups and imported `WTF` folders of earlier runs.
const LEFT_OUT_PREFIXES: [&str; 4] = [
    "import-backup-",
    "transfer-backup-",
    "WTF-imported-",
    "benilla-transfer-backup-",
];

/// Copy the `benilla-config` folder `from` into `to`, which is created if it is missing. Each file
/// it replaces with other bytes is moved to `<to>/transfer-backup-<time>/` first, under its own
/// path.
pub fn copy_config(from: &Path, to: &Path) -> Result<Report, String> {
    if !looks_like_config(from) {
        return Err(format!(
            "{} is not a benilla-config folder (no config.toml, AddOns or saved)",
            from.display()
        ));
    }
    refuse_overlap(from, to)?;
    std::fs::create_dir_all(to).map_err(|e| format!("cannot make {}: {e}", to.display()))?;
    let backup = super::backup_dir(to, "transfer-backup");
    let mut run = Run {
        backup: backup.clone(),
        folds_case: folds_case(to),
        report: Report {
            backup: Some(backup),
            ..Report::default()
        },
        counts: [0; 4],
    };
    run.report.lines.push(format!(
        "copy {} to {}{}",
        from.display(),
        to.display(),
        if run.folds_case {
            " (names there ignore case)"
        } else {
            ""
        }
    ));
    run.tree(from, to, Path::new(""));
    let [copied, same, replaced, skipped] = run.counts;
    run.report.lines.push(format!(
        "{copied} files copied, {same} already the same, {replaced} replaced (the old ones backed \
         up), {skipped} skipped"
    ));
    write_log(&to.join("transfer.log"), &run.report);
    Ok(run.report)
}

/// A folder benilla wrote, by the files only it has.
fn looks_like_config(dir: &Path) -> bool {
    ["config.toml", "AddOns", "saved", "addon-state"]
        .iter()
        .any(|n| child(dir, n).exists())
}

struct Run {
    backup: PathBuf,
    /// The destination's file system ignores case (Windows, macOS, a FAT stick).
    folds_case: bool,
    report: Report,
    /// Copied, already the same, replaced, skipped.
    counts: [usize; 4],
}

impl Run {
    /// Copy the folder `src` into `dest`, `rel` being their path under the two roots.
    fn tree(&mut self, src: &Path, dest: &Path, rel: &Path) {
        let top = rel.as_os_str().is_empty();
        let mut entries: Vec<_> = std::fs::read_dir(src)
            .into_iter()
            .flatten()
            .flatten()
            .collect();
        entries.sort_by_key(|e| e.file_name());
        // Names this folder holds more than once, in other cases: kept apart where case counts.
        let mut seen = BTreeSet::new();
        let twins: BTreeSet<String> = entries
            .iter()
            .map(|e| e.file_name().to_string_lossy().to_lowercase())
            .filter(|n| !seen.insert(n.clone()))
            .collect();
        seen.clear();
        for entry in entries {
            let name = entry.file_name().to_string_lossy().into_owned();
            let rel = rel.join(&name);
            if top && left_out(&name) {
                continue;
            }
            let shown = rel.display().to_string().replace('\\', "/");
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_symlink() {
                self.skip(format!("{shown}: a link, not followed"));
                continue;
            }
            if cfg!(windows) && !windows_name(&name) {
                self.skip(format!("{shown}: a name Windows cannot hold"));
                continue;
            }
            if self.folds_case && !seen.insert(name.to_lowercase()) {
                self.skip(format!(
                    "{shown}: another entry here differs from it only by case"
                ));
                continue;
            }
            // The top level's names are benilla's own, where `addons` and `AddOns` are two
            // folders; below it an entry spelled another way is the same one.
            let target = if top || twins.contains(&name.to_lowercase()) {
                dest.join(&name)
            } else {
                child(dest, &name)
            };
            if kind.is_dir() {
                if target.exists() && !target.is_dir() {
                    self.set_aside(&target, &rel);
                }
                if let Err(e) = std::fs::create_dir_all(&target) {
                    self.skip(format!("{shown}: cannot make the folder ({e})"));
                    continue;
                }
                self.tree(&entry.path(), &target, &rel);
            } else {
                self.file(&entry.path(), &target, &rel, &shown);
            }
        }
    }

    fn file(&mut self, src: &Path, target: &Path, rel: &Path, shown: &str) {
        if target.is_dir() {
            self.set_aside(target, rel);
        } else if target.exists() {
            if same_bytes(src, target) {
                self.counts[1] += 1;
                return;
            }
            if !self.set_aside(target, rel) {
                return;
            }
        }
        // The source's spelling: a destination spelled another way was moved aside above.
        let dest = target.with_file_name(file_name(src));
        match std::fs::copy(src, &dest) {
            Ok(_) => self.counts[0] += 1,
            Err(e) => self.skip(format!("{shown}: not copied ({e})")),
        }
    }

    /// Move the destination's `path` to the backup under `rel`; false when it cannot be moved.
    fn set_aside(&mut self, path: &Path, rel: &Path) -> bool {
        match move_aside(path, &self.backup.join(rel)) {
            Ok(()) => {
                self.counts[2] += 1;
                true
            }
            Err(e) => {
                let shown = rel.display().to_string().replace('\\', "/");
                self.skip(format!("{shown}: left as it was, cannot back it up ({e})"));
                false
            }
        }
    }

    fn skip(&mut self, why: String) {
        self.counts[3] += 1;
        self.report.lines.push(format!("skipped {why}"));
    }
}

fn left_out(name: &str) -> bool {
    LEFT_OUT.iter().any(|n| n.eq_ignore_ascii_case(name))
        || LEFT_OUT_PREFIXES
            .iter()
            .any(|p| name.len() >= p.len() && name[..p.len()].eq_ignore_ascii_case(p))
        || name.ends_with(".tmp")
}

/// Whether Windows can hold `name`: none of its reserved characters, no trailing dot or space, and
/// not a device name, with or without an extension.
fn windows_name(name: &str) -> bool {
    const DEVICES: [&str; 22] = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    let stem = name.split('.').next().unwrap_or(name);
    !name.is_empty()
        && !name.contains(['<', '>', ':', '"', '|', '?', '*', '\\'])
        && !name.chars().any(|c| c.is_control())
        && !name.ends_with(['.', ' '])
        && !DEVICES
            .iter()
            .any(|d| d.eq_ignore_ascii_case(stem.trim_end()))
}

/// Whether `dir`'s file system ignores case: a probe file, then its name in upper case.
fn folds_case(dir: &Path) -> bool {
    let probe = dir.join(".case-probe");
    if std::fs::write(&probe, b"").is_err() {
        return cfg!(any(windows, target_os = "macos"));
    }
    let folds = dir.join(".CASE-PROBE").exists();
    let _ = std::fs::remove_file(&probe);
    folds
}

fn same_bytes(a: &Path, b: &Path) -> bool {
    let size = |p: &Path| std::fs::metadata(p).map(|m| m.len()).ok();
    size(a) == size(b) && matches!((std::fs::read(a), std::fs::read(b)), (Ok(x), Ok(y)) if x == y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_refuses_its_reserved_names() {
        for bad in [
            "a:b", "what?", "trail.", "trail ", "CON", "nul.txt", "Com1.lua", "a|b",
        ] {
            assert!(!windows_name(bad), "{bad}");
        }
        for good in [
            "pfUI",
            "N_Zoth-Maro.txt",
            "!BigWigs.lua",
            "console.lua",
            "Ünïcode",
        ] {
            assert!(windows_name(good), "{good}");
        }
    }

    #[test]
    fn the_machine_s_own_folders_stay_behind() {
        for name in [
            "cache",
            "Diagnostics",
            "import-backup-20261031-021122",
            "WTF-imported-20261031-021122",
            "transfer-backup-1",
            "config.tmp",
        ] {
            assert!(left_out(name), "{name}");
        }
        for name in ["config.toml", "AddOns", "saved", "Screenshots", "WTF"] {
            assert!(!left_out(name), "{name}");
        }
    }
}
