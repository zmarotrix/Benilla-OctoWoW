//! benilla out to a normal client: `config.toml` into its `Config.wtf`, every account-data section
//! as the server is sent it ([`sections::read`]) into the reference's file for it, the addon
//! enable lists, camera poses and saved variables into their `WTF` places, and
//! `benilla-config/AddOns` into its `Interface/AddOns`. The import's map, run backwards
//! ([`crate::wtf_import`]).

use std::collections::BTreeMap;
use std::path::Path;

use crate::account_data::sections;
use crate::local_state;
use crate::wtf_import::{
    child, copy, copy_tree, file_name, subdirs, ACCOUNT_FILES, CHARACTER_FILES, SKIPPED_CVARS,
};

use super::{copy_addons, refuse_overlap, write_log, Report};

/// Write benilla's settings and addons into the client at `wow`, under the account `account`, or
/// the one the login screen remembers (`accountName`), or the one account its `WTF` already has.
/// Its `WTF` as it was is copied to `<wow>/benilla-transfer-backup-<time>/` first, with every addon
/// folder replaced.
pub fn export_client(wow: &Path, account: Option<&str>) -> Result<Report, String> {
    let home = local_state::home().ok_or("benilla has no settings folder here")?;
    let wow = super::client_root(wow);
    if !wow.is_dir() {
        return Err(format!("{} is not a folder", wow.display()));
    }
    refuse_overlap(&wow, &home)?;
    let cvars = crate::cvars::config_values();
    let wtf = child(&wow, "WTF");
    let accounts = child(&wtf, "Account");
    let account = account_name(account, &cvars, &accounts)?;
    let backup = super::backup_dir(&wow, "benilla-transfer-backup");
    let mut report = Report {
        backup: Some(backup.clone()),
        ..Report::default()
    };
    let log = &mut report.lines;
    log.push(format!("export to {} as {account}", wow.display()));
    let n = copy_tree(&wtf, &backup.join("WTF"));
    log.push(format!(
        "backed up {n} existing WTF files to {}",
        file_name(&backup)
    ));

    match write_config_wtf(&child(&wtf, "Config.wtf"), &cvars) {
        Ok(n) => log.push(format!("Config.wtf: {n} settings")),
        Err(e) => log.push(format!("Config.wtf: not written ({e})")),
    }

    let account_dir = child(&accounts, &account);
    let defaults = crate::bindings::store::key_table(crate::ui_script::default_bindings());
    for (file, section) in ACCOUNT_FILES {
        if let Some(bytes) = sections::read(section, None, &defaults) {
            put(
                &account_dir.join(file),
                &bytes,
                &format!("{account}: {file}"),
                log,
            );
        }
    }
    if let Some(src) = local_state::addon_saved_account_dir() {
        let n = copy_lua(&src, &child(&account_dir, "SavedVariables"));
        if n > 0 {
            log.push(format!("{account}: {n} addon saved-variable files"));
        }
    }
    if let Some(src) = local_state::saved_variables_path().filter(|p| p.is_file()) {
        if copy(&src, &child(&account_dir, "SavedVariables.lua")) {
            log.push(format!("{account}: SavedVariables.lua"));
        }
    }

    let mut realms = realm_spellings(&cvars, &home, &account_dir);
    for (token, character) in local_state::character_keys() {
        let realm = realms.entry(token.clone()).or_insert_with(|| {
            // A token with no `_` is the name itself; one with it lost a character there.
            if token.contains('_') {
                log.push(format!(
                    "realm {token}: its name's spelling is unknown, written as {token}"
                ));
            }
            token.clone()
        });
        let realm = realm.clone();
        let dir = child(&child(&account_dir, &realm), &character);
        export_character(&dir, &(token, character), &realm, &defaults, log);
    }

    let addons = local_state::addons_root().filter(|p| p.is_dir());
    if let Some(src) = addons {
        let dest = child(&child(&wow, "Interface"), "AddOns");
        copy_addons(&src, &dest, &backup.join("Interface").join("AddOns"), log);
    }
    write_log(&backup.join("transfer.log"), &report);
    write_log(&home.join("transfer.log"), &report);
    Ok(report)
}

/// One character's files into its `WTF` folder `dir`.
fn export_character(
    dir: &Path,
    identity: &sections::Identity,
    realm: &str,
    defaults: &[benilla_ui::script::keybind::KeyBinding],
    log: &mut Vec<String>,
) {
    let (token, character) = identity;
    let who = format!("{realm}/{character}");
    for (file, section) in CHARACTER_FILES {
        let bytes = sections::read(section, Some(identity), defaults);
        match bytes {
            Some(bytes) if !bytes.is_empty() => {
                put(&child(dir, file), &bytes, &format!("{who}: {file}"), log)
            }
            // A character on the account's key bindings has no file of its own (`0x4b64b0`), so
            // one the client had goes; the backup holds it.
            _ if sections::BINDINGS.contains(&section) => {
                let old = child(dir, file);
                if old.is_file() && std::fs::remove_file(&old).is_ok() {
                    log.push(format!("{who}: {file} removed, the account set is in use"));
                }
            }
            _ => {}
        }
    }
    for (src, file) in [
        (
            local_state::addons_state_path(token, character),
            "AddOns.txt",
        ),
        (
            local_state::camera_character_path(token, character),
            "camera-settings.txt",
        ),
    ] {
        if let Some(src) = src.filter(|p| p.is_file()) {
            if copy(&src, &child(dir, file)) {
                log.push(format!("{who}: {file}"));
            }
        }
    }
    if let Some(src) = local_state::addon_saved_character_dir(token, character) {
        let n = copy_lua(&src, &child(dir, "SavedVariables"));
        if n > 0 {
            log.push(format!("{who}: {n} addon saved-variable files"));
        }
    }
}

/// The account's folder name: `given`, else `accountName`, else the one account `accounts`
/// holds, in the spelling a folder there already has.
fn account_name(
    given: Option<&str>,
    cvars: &BTreeMap<String, String>,
    accounts: &Path,
) -> Result<String, String> {
    let remembered = cvars
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("accountName"))
        .map(|(_, v)| v.as_str());
    let existing = subdirs(accounts);
    let name = match given
        .or(remembered)
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(name) => name.to_uppercase(),
        None => match existing.as_slice() {
            [only] => file_name(only),
            _ => {
                return Err(
                    "which account? Benilla remembers none: log in once in Benilla, or \
                            name it with --account"
                        .into(),
                )
            }
        },
    };
    if name.contains(['/', '\\', ':']) || name.starts_with('.') {
        return Err(format!("{name} is not an account name"));
    }
    Ok(file_name(&child(accounts, &name)))
}

/// Token to realm name, from the realms the account folder already has, then the names an import
/// recorded, then the realm folders of the `WTF` folders the launch importer took in
/// (`WTF-imported-<time>`, kept in the home), then the realm the login screen remembers.
fn realm_spellings(
    cvars: &BTreeMap<String, String>,
    home: &Path,
    account_dir: &Path,
) -> BTreeMap<String, String> {
    let mut names = BTreeMap::new();
    let imported = subdirs(home)
        .into_iter()
        .filter(|d| file_name(d).starts_with("WTF-imported-"));
    for account in imported.flat_map(|wtf| subdirs(&child(&wtf, "Account"))) {
        for dir in subdirs(&account) {
            let realm = file_name(&dir);
            if !realm.eq_ignore_ascii_case("SavedVariables") {
                names.insert(local_state::file_token(&realm), realm);
            }
        }
    }
    names.extend(super::realm_names());
    if let Some((_, realm)) = cvars
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("realmName"))
    {
        names
            .entry(local_state::file_token(realm))
            .or_insert_with(|| realm.clone());
    }
    for dir in subdirs(account_dir) {
        let realm = file_name(&dir);
        if !realm.eq_ignore_ascii_case("SavedVariables") {
            names.insert(local_state::file_token(&realm), realm);
        }
    }
    names
}

/// `SET name "value"` for each saved setting, over the matching line of the file `path` already
/// has or added at its end; every other line stays, in the file's own line ending. A value holding
/// a `"` has no spelling in the file and is left out. How many settings were written.
fn write_config_wtf(path: &Path, cvars: &BTreeMap<String, String>) -> std::io::Result<usize> {
    let old = std::fs::read_to_string(path).unwrap_or_default();
    let mut lines: Vec<String> = old.lines().map(str::to_string).collect();
    // Line index by name, lowercased: the reference matches a CVar's name ignoring case.
    let present: BTreeMap<String, usize> = lines
        .iter()
        .enumerate()
        .filter_map(|(i, line)| {
            let rest = line.trim().strip_prefix("SET ")?;
            let (name, _) = rest.split_once(' ')?;
            Some((name.to_ascii_lowercase(), i))
        })
        .collect();
    let mut n = 0;
    for (name, value) in cvars {
        if SKIPPED_CVARS.iter().any(|s| s.eq_ignore_ascii_case(name)) || value.contains('"') {
            continue;
        }
        let line = format!("SET {name} \"{value}\"");
        match present.get(&name.to_ascii_lowercase()) {
            Some(&i) => lines[i] = line,
            None => lines.push(line),
        }
        n += 1;
    }
    let eol = if old.contains("\r\n") { "\r\n" } else { "\n" };
    local_state::write_atomic(path, &(lines.join(eol) + eol))?;
    Ok(n)
}

/// Write `bytes` to `path`, logging `what`.
fn put(path: &Path, bytes: &[u8], what: &str, log: &mut Vec<String>) {
    match local_state::write_atomic_bytes(path, bytes) {
        Ok(()) => log.push(what.to_string()),
        Err(e) => log.push(format!("{what}: not written ({e})")),
    }
}

/// Each `<Addon>.lua` of `src` into `dest`; how many were copied.
fn copy_lua(src: &Path, dest: &Path) -> usize {
    std::fs::read_dir(src)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|x| x.eq_ignore_ascii_case("lua")))
        .filter(|p| copy(p, &child(dest, &file_name(p))))
        .count()
}
