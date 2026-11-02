use std::path::{Path, PathBuf};

use super::*;
use crate::local_state::test_env::{EnvGuard, ENV_LOCK};

fn put(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// A fresh scratch folder for one test.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("benilla-transfer-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A normal client with one account, one character and two addons.
fn client(root: &Path) {
    let wtf = root.join("WTF");
    put(
        &wtf.join("Config.wtf"),
        "SET MusicVolume \"0.3\"\r\nSET gxApi \"d3d\"\r\nSET realmList \"logon.octowow.st\"\r\n",
    );
    let acc = wtf.join("Account").join("VRISKA");
    put(&acc.join("SavedVariables/pfUI.lua"), "pfUI_config = {}\n");
    put(
        &acc.join("SavedVariables.lua"),
        "SHOW_FULLSCREEN_STATUS = \"1\"\n",
    );
    put(
        &acc.join("macros-cache.txt"),
        "MACRO 1 \"Hi\" Ability_Ambush\n/wave\nEND\n",
    );
    let ch = acc.join("N'Zoth").join("Vriska");
    put(&ch.join("AddOns.txt"), "pfUI: enabled\n");
    put(
        &ch.join("camera-settings.txt"),
        "CameraDistance 7.5\nCameraPitch 15\n",
    );
    put(&ch.join("SavedVariables/pfUI.lua"), "pfUI_char = {}\n");
    let addons = root.join("Interface").join("AddOns");
    put(&addons.join("pfUI/pfUI.toc"), "## Interface: 11200\n");
    put(&addons.join("pfUI/modules/bags.lua"), "-- bags\n");
    put(
        &addons.join("Blizzard_AuctionUI/Blizzard_AuctionUI.toc"),
        "",
    );
}

/// A normal client's folder comes in whole: its `WTF` through the importer, read in place and
/// left as it was, and its addons, each folder benilla already had, in any case, set aside whole
/// in the backup first.
#[test]
fn a_client_folder_comes_in_with_its_addons() {
    let _l = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let tmp = scratch("import");
    let (wow, home) = (tmp.join("OctoWoW"), tmp.join("benilla-config"));
    let _c = EnvGuard::unset("WOW_CAPTURE");
    let _h = EnvGuard::set("BENILLA_HOME", home.to_str().unwrap());
    client(&wow);
    put(
        &home.join("config.toml"),
        "[cvars]\nSoundVolume = \"0.5\"\n",
    );
    put(&home.join("AddOns/PFUI/old.lua"), "-- an older pfUI\n");
    put(&home.join("AddOns/ShaguDPS/ShaguDPS.toc"), "");

    // The player may pick the client's `Interface/AddOns` folder as well as the client's own.
    let report = import_client(&wow.join("Interface").join("AddOns")).unwrap();

    let config = read(&home.join("config.toml"));
    assert!(config.contains("MusicVolume = \"0.3\""), "{config}");
    assert!(home.join("saved/pfUI.lua").is_file());
    assert!(home.join("saved/N_Zoth-Vriska/pfUI.lua").is_file());
    assert!(home.join("addon-state/N_Zoth-Vriska.txt").is_file());
    assert!(read(&home.join("saved-variables.lua")).contains("SHOW_FULLSCREEN_STATUS"));
    assert_eq!(read(&home.join("realm-names.txt")), "N_Zoth\tN'Zoth\n");
    assert!(home.join("AddOns/pfUI/modules/bags.lua").is_file());
    assert!(!home.join("AddOns/pfUI/old.lua").exists(), "replaced whole");
    assert!(
        home.join("AddOns/ShaguDPS/ShaguDPS.toc").is_file(),
        "others stay"
    );
    assert!(!home.join("AddOns/Blizzard_AuctionUI").exists());
    let backup = report.backup.clone().unwrap();
    assert!(backup.join("AddOns/PFUI/old.lua").is_file(), "{report:?}");
    assert!(backup.join("config.toml").is_file());
    assert!(
        wow.join("WTF/Config.wtf").is_file(),
        "the source is left as it was"
    );
    assert!(home.join("transfer.log").is_file());
    drop(_h);
    std::fs::remove_dir_all(&tmp).ok();
}

/// benilla goes out to a normal client: every setting in the `WTF` place the client reads it from,
/// the realm under its own spelling, `Config.wtf` merged line by line in its own line ending, and
/// the client's `WTF` and replaced addons backed up first.
#[test]
fn benilla_goes_out_to_a_client_folder() {
    let _l = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let tmp = scratch("export");
    let (wow, home) = (tmp.join("OctoWoW"), tmp.join("benilla-config"));
    let _c = EnvGuard::unset("WOW_CAPTURE");
    let _h = EnvGuard::set("BENILLA_HOME", home.to_str().unwrap());
    put(
        &home.join("config.toml"),
        "[cvars]\naccountName = \"vriska\"\nMusicVolume = \"0.2\"\nrealmName = \"N'Zoth\"\n",
    );
    put(&home.join("bindings/account.wtf"), "bind F JUMP\n");
    put(
        &home.join("macros/account.txt"),
        "MACRO 1 \"Hi\" X\n/wave\nEND\n",
    );
    put(&home.join("saved/pfUI.lua"), "pfUI_config = {}\n");
    put(
        &home.join("saved/N_Zoth-Vriska/pfUI.lua"),
        "pfUI_char = {}\n",
    );
    put(&home.join("saved-variables.lua"), "GROUP_ENABLED = \"0\"\n");
    put(
        &home.join("addon-state/N_Zoth-Vriska.txt"),
        "pfUI: enabled\n",
    );
    put(
        &home.join("camera/N_Zoth-Vriska.txt"),
        "CameraDistance 7.5\n",
    );
    put(
        &home.join("layout/N_Zoth-Vriska.txt"),
        "# a note\nFrame: A\n",
    );
    put(&home.join("AddOns/pfUI/pfUI.toc"), "## Interface: 11200\n");
    // A realm whose spelling only a `WTF` folder taken in at an earlier launch still has.
    put(
        &home.join("addon-state/Tel_Abim-Pyro.txt"),
        "pfUI: disabled\n",
    );
    std::fs::create_dir_all(home.join("WTF-imported-20261031-021122/Account/VRISKA/Tel'Abim"))
        .unwrap();
    put(
        &wow.join("WTF/Config.wtf"),
        "SET gxApi \"d3d\"\r\nSET musicvolume \"1.0\"\r\n",
    );
    // A character-specific key binding set the client had: benilla's character is on the
    // account's, so it goes.
    put(
        &wow.join("WTF/Account/VRISKA/N'Zoth/Vriska/bindings-cache.wtf"),
        "bind G JUMP\n",
    );
    put(&wow.join("Interface/AddOns/pfUI/stale.lua"), "-- stale\n");

    let report = export_client(&wow, None).unwrap();

    let config = read(&wow.join("WTF/Config.wtf"));
    assert!(config.contains("SET gxApi \"d3d\"\r\n"), "kept: {config:?}");
    assert!(config.contains("SET MusicVolume \"0.2\"\r\n"), "{config:?}");
    assert!(!config.contains("1.0"), "replaced, not added: {config:?}");
    assert!(config.contains("SET accountName \"vriska\""), "{config:?}");
    let acc = wow.join("WTF/Account/VRISKA");
    assert_eq!(read(&acc.join("bindings-cache.wtf")), "bind F JUMP\n");
    assert!(read(&acc.join("macros-cache.txt")).contains("/wave"));
    assert!(acc.join("SavedVariables/pfUI.lua").is_file());
    assert!(read(&acc.join("SavedVariables.lua")).contains("GROUP_ENABLED"));
    let ch = acc.join("N'Zoth").join("Vriska");
    assert_eq!(read(&ch.join("AddOns.txt")), "pfUI: enabled\n");
    assert!(ch.join("camera-settings.txt").is_file());
    assert!(ch.join("SavedVariables/pfUI.lua").is_file());
    assert_eq!(
        read(&ch.join("layout-cache.txt")),
        "Frame: A\n",
        "notes left out"
    );
    assert!(!ch.join("bindings-cache.wtf").exists());
    assert!(acc.join("Tel'Abim/Pyro/AddOns.txt").is_file(), "{report:?}");
    assert!(wow.join("Interface/AddOns/pfUI/pfUI.toc").is_file());
    assert!(!wow.join("Interface/AddOns/pfUI/stale.lua").exists());
    let backup = report.backup.clone().unwrap();
    assert!(read(&backup.join("WTF/Config.wtf")).contains("1.0"));
    assert!(backup
        .join("WTF/Account/VRISKA/N'Zoth/Vriska/bindings-cache.wtf")
        .is_file());
    assert!(backup.join("Interface/AddOns/pfUI/stale.lua").is_file());
    assert!(backup.join("transfer.log").is_file());
    drop(_h);
    std::fs::remove_dir_all(&tmp).ok();
}

/// With no account named, remembered or already in the client, the export asks for one.
#[test]
fn an_export_with_no_account_asks_for_one() {
    let _l = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let tmp = scratch("export-no-account");
    let (wow, home) = (tmp.join("OctoWoW"), tmp.join("benilla-config"));
    let _c = EnvGuard::unset("WOW_CAPTURE");
    let _h = EnvGuard::set("BENILLA_HOME", home.to_str().unwrap());
    std::fs::create_dir_all(&wow).unwrap();
    put(&home.join("config.toml"), "[cvars]\n");
    let err = export_client(&wow, None).unwrap_err();
    assert!(err.contains("--account"), "{err}");
    assert!(!wow.join("WTF").exists(), "nothing written");
    drop(_h);
    std::fs::remove_dir_all(&tmp).ok();
}

/// A client's settings in and back out to another client land as they were.
#[test]
fn a_client_round_trips_through_benilla() {
    let _l = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let tmp = scratch("round-trip");
    let (a, b, home) = (tmp.join("a"), tmp.join("b"), tmp.join("benilla-config"));
    let _c = EnvGuard::unset("WOW_CAPTURE");
    let _h = EnvGuard::set("BENILLA_HOME", home.to_str().unwrap());
    client(&a);
    import_client(&a).unwrap();
    std::fs::create_dir_all(&b).unwrap();
    export_client(&b, Some("Vriska")).unwrap();
    for file in [
        "WTF/Account/VRISKA/SavedVariables/pfUI.lua",
        "WTF/Account/VRISKA/SavedVariables.lua",
        "WTF/Account/VRISKA/macros-cache.txt",
        "WTF/Account/VRISKA/N'Zoth/Vriska/AddOns.txt",
        "WTF/Account/VRISKA/N'Zoth/Vriska/camera-settings.txt",
        "WTF/Account/VRISKA/N'Zoth/Vriska/SavedVariables/pfUI.lua",
        "Interface/AddOns/pfUI/modules/bags.lua",
    ] {
        assert_eq!(read(&b.join(file)), read(&a.join(file)), "{file}");
    }
    let config = read(&b.join("WTF/Config.wtf"));
    assert!(config.contains("SET MusicVolume \"0.3\""), "{config}");
    assert!(!b.join("Interface/AddOns/Blizzard_AuctionUI").exists());
    drop(_h);
    std::fs::remove_dir_all(&tmp).ok();
}

/// One benilla's folder into another's: a merge that backs up each file it replaces, leaves the
/// machine's own folders behind, lands in a folder the destination spells another way, and on a
/// second run finds everything the same.
#[test]
fn a_config_folder_copies_into_another() {
    let tmp = scratch("copy");
    let (from, to) = (
        tmp.join("pc-a/benilla-config"),
        tmp.join("pc-b/benilla-config"),
    );
    put(
        &from.join("config.toml"),
        "[cvars]\nMusicVolume = \"0.2\"\n",
    );
    put(&from.join("AddOns/pfUI/pfUI.toc"), "## new\n");
    put(&from.join("saved/pfUI.lua"), "pfUI_config = {}\n");
    put(&from.join("cache/N_Zoth/creature.bin"), "x");
    put(
        &from.join("import-backup-20261031-021122/config.toml"),
        "old",
    );
    put(&from.join("Diagnostics/fps-journal.csv"), "x");
    put(&to.join("config.toml"), "[cvars]\nMusicVolume = \"1.0\"\n");
    put(&to.join("AddOns/pfui/pfUI.toc"), "## old\n");
    put(&to.join("Screenshots/a.png"), "png");

    let report = copy_config(&from, &to).unwrap();

    assert!(read(&to.join("config.toml")).contains("0.2"));
    let addons: Vec<String> = std::fs::read_dir(to.join("AddOns"))
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        addons.len(),
        1,
        "one pfUI folder, whatever the case: {addons:?}"
    );
    assert_eq!(
        read(&child(&to.join("AddOns"), "pfUI").join("pfUI.toc")),
        "## new\n"
    );
    assert!(to.join("saved/pfUI.lua").is_file());
    assert!(
        to.join("Screenshots/a.png").is_file(),
        "a merge keeps what it does not have"
    );
    assert!(!to.join("cache").exists());
    assert!(!to.join("Diagnostics").exists());
    assert!(!to.join("import-backup-20261031-021122").exists());
    let backup = report.backup.clone().unwrap();
    assert!(
        read(&backup.join("config.toml")).contains("1.0"),
        "{report:?}"
    );
    assert_eq!(read(&backup.join("AddOns/pfUI/pfUI.toc")), "## old\n");

    let again = copy_config(&from, &to).unwrap();
    let last = again.lines.last().unwrap();
    assert!(
        last.starts_with("0 files copied, 3 already the same"),
        "{last}"
    );
    assert!(
        !again.backup.unwrap().exists(),
        "nothing replaced, so no backup"
    );
    std::fs::remove_dir_all(&tmp).ok();
}

/// A folder benilla did not write, or one inside the other, is refused before anything moves.
#[test]
fn a_copy_refuses_what_is_not_a_config_folder() {
    let tmp = scratch("copy-refuse");
    std::fs::create_dir_all(tmp.join("photos")).unwrap();
    assert!(copy_config(&tmp.join("photos"), &tmp.join("b")).is_err());
    put(&tmp.join("a/config.toml"), "");
    assert!(copy_config(&tmp.join("a"), &tmp.join("a/inner")).is_err());
    assert!(!tmp.join("b").exists());
    std::fs::remove_dir_all(&tmp).ok();
}
