//! benilla's login screen tools, beyond 1.12's glue surface: the settings verbs the in-game state
//! has (`GetCVar`, `SetCVar`, `GetScreenResolutions`, `RestartGx` and their kin, so the settings
//! window works before a character is chosen), and two launcher actions the host carries out at
//! its next frame, emptying the template caches and opening one of the config folders.
//! Deviation: the reference's glue state has none of these.

use mlua::{Lua, Value};

use super::Model;

/// A launcher action the host carries out at its next frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LauncherAsk {
    /// Empty every saved template cache, on disk and in memory.
    ClearCache,
    /// Open one of the config folders in the system's file browser.
    OpenFolder(LauncherFolder),
}

/// The folders `BenillaOpenFolder` names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LauncherFolder {
    AddOns,
    Screenshots,
    Logs,
}

impl LauncherFolder {
    fn named(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "addons" => Some(Self::AddOns),
            "screenshots" => Some(Self::Screenshots),
            "logs" => Some(Self::Logs),
            _ => None,
        }
    }
}

/// The globals [`install`] adds past 1.12's glue surface: the surface gate's exceptions.
#[cfg(test)]
pub(crate) const GLUE_GLOBALS: &[&str] = &[
    "RegisterCVar",
    "GetCVar",
    "GetCVarDefault",
    "GetMultisampleFormats",
    "GetCurrentMultisampleFormat",
    "SetMultisampleFormat",
    "SetCVar",
    "GetScreenResolutions",
    "GetCurrentResolution",
    "SetScreenResolution",
    "GetRefreshRates",
    "GetVideoCaps",
    "RestartGx",
    "RestoreVideoDefaults",
    "GetGamma",
    "SetGamma",
    "GetBaseMip",
    "SetBaseMip",
    "GetDoodadAnim",
    "SetDoodadAnim",
    "GetFarclip",
    "SetFarclip",
    "GetTerrainMip",
    "SetTerrainMip",
    "GetTexLodBias",
    "SetTexLodBias",
    "GetWaterDetail",
    "SetWaterDetail",
    "GetWorldDetail",
    "SetWorldDetail",
    "BenillaClearCache",
    "BenillaOpenFolder",
];

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    super::cvars::install(lua)?;
    let g = lua.globals();

    // `BenillaClearCache()`: the saved item, creature, object, quest and text records go, to be
    // asked again as they are met.
    g.set(
        "BenillaClearCache",
        lua.create_function(|lua, ()| {
            ask(lua, LauncherAsk::ClearCache);
            Ok(())
        })?,
    )?;

    // `BenillaOpenFolder(name)`: "AddOns", "Screenshots" or "Logs"; any other name is an error.
    g.set(
        "BenillaOpenFolder",
        lua.create_function(|lua, name: Value| {
            let name = match name {
                Value::String(s) => s.to_str()?.to_string(),
                _ => String::new(),
            };
            let Some(folder) = LauncherFolder::named(&name) else {
                return Err(mlua::Error::runtime(
                    "Usage: BenillaOpenFolder(\"AddOns\" | \"Screenshots\" | \"Logs\")",
                ));
            };
            ask(lua, LauncherAsk::OpenFolder(folder));
            Ok(())
        })?,
    )?;
    Ok(())
}

fn ask(lua: &Lua, ask: LauncherAsk) {
    lua.app_data_mut::<Model>()
        .expect("model app_data")
        .launcher_asks
        .push(ask);
}

impl super::UiScript {
    /// The login screen's addon manager before a session: every installed addon listed with no
    /// server reply, and a node per character the host keeps an `AddOns.txt` for on the last
    /// realm played, so "All" reaches each. A character list or a reply later replaces both.
    pub fn seat_launcher_addons(&mut self, characters: &[String]) {
        let mut model = self.model_mut();
        super::addon::index_without_reply(&mut model);
        let realm = super::glue_realm::realm_name(&model);
        model.glue.addons.load(&realm, characters);
    }

    /// The launcher actions asked since the last take, in order.
    pub fn take_launcher_asks(&mut self) -> Vec<LauncherAsk> {
        std::mem::take(&mut self.model_mut().launcher_asks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::script::UiScript;

    /// Before a session the login screen lists every installed addon, title-sorted, where the
    /// reference lists none until the reply.
    #[test]
    fn the_login_screen_lists_installed_addons_before_a_reply() {
        let mut s = UiScript::new_glue().unwrap();
        let row = |name: &str| crate::script::AddOnInfo {
            name: name.into(),
            title: Some(name.into()),
            interface: 11200,
            enabled: true,
            ..Default::default()
        };
        s.register_addons(vec![row("pfUI"), row("Cursive")], None, None, None);
        assert_eq!(s.eval::<i64>("return GetNumAddOns()").unwrap(), 0);
        s.seat_launcher_addons(&["Vriska".to_string()]);
        assert_eq!(
            s.eval::<Vec<String>>(
                "local t = {} for i = 1, GetNumAddOns() do t[i] = (GetAddOnInfo(i)) end return t"
            )
            .unwrap(),
            ["Cursive", "pfUI"]
        );
    }

    #[test]
    fn the_glue_vm_queues_launcher_actions_and_reads_cvars() {
        let mut s = UiScript::new_glue().unwrap();
        s.run(r#"BenillaClearCache(); BenillaOpenFolder("addons")"#)
            .unwrap();
        assert_eq!(
            s.take_launcher_asks(),
            [
                LauncherAsk::ClearCache,
                LauncherAsk::OpenFolder(LauncherFolder::AddOns)
            ]
        );
        assert!(s.run(r#"BenillaOpenFolder("C:\\Windows")"#).is_err());
        s.run(r#"RegisterCVar("benillaProbe", "7")"#).unwrap();
        assert_eq!(
            s.eval::<String>(r#"return GetCVar("benillaProbe")"#)
                .unwrap(),
            "7"
        );
    }
}
