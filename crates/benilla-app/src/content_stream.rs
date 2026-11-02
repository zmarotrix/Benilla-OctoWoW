//! Content streaming ([`benilla_content`]): a server that offers packs in the extension handshake
//! has them downloaded into `benilla-config/content/` while the player plays, and mounted above
//! the install at the next start, before any archive opens. Only for a server the handshake is
//! opened with (`serverExtensions`, [`crate::server_extensions`]). Deviation: 1.12.1 reads only
//! its own `Data` folder.
//!
//! Live hotfixes: a server that changes its packs while the player is in the world sends its offer
//! again ([`SessionEvent::ServerExtensions`]). Once the new packs are kept, the chain is opened
//! again over them and everything read from the changed files is read again ([`apply_live`]), with
//! no restart.

use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use benilla_assets::LockRecover;
use benilla_content::{server_key, split_host, Store, Synced, KEY_MANIFEST, KEY_PORT};
use benilla_protocol::SessionEvent;
use bevy::prelude::*;

/// Where the packs and the servers' manifests are kept.
fn store() -> Option<Store> {
    crate::local_state::home().map(|home| Store::new(home.join("content")))
}

/// Mount the packs kept for the server this start will log in to, before any chain opens:
/// `$WOW_HOST`, else `realmList` as `config.toml` holds it. Runs before the logger exists.
pub(crate) fn mount_at_boot(data_dir: &std::path::Path) {
    let _ = DATA_DIR.set(data_dir.to_path_buf());
    let realmlist = std::env::var("WOW_HOST")
        .ok()
        .filter(|h| !h.trim().is_empty())
        .or_else(|| crate::cvars::boot_cvar(crate::realmlist::CVAR_REALMLIST))
        .or_else(|| {
            crate::cvars::registered_default(crate::realmlist::CVAR_REALMLIST, 0)
                .map(str::to_string)
        })
        .unwrap_or_default();
    let mode = crate::server_extensions::Mode::parse(
        &crate::cvars::boot_cvar(crate::server_extensions::CVAR).unwrap_or_default(),
    );
    if !mode.wanted(&realmlist) {
        return;
    }
    let Some(store) = store() else { return };
    let key = server_key(&realmlist);
    let mounts = store.mounts(&key);
    if mounts.is_empty() {
        return;
    }
    println!(
        "content: mounting {} pack(s) streamed from {key} above the install",
        mounts.len()
    );
    benilla_formats::set_overlay(mounts);
}

/// What the download thread reports.
enum Report {
    Progress { pack: String, percent: u64 },
    Done(Synced),
    Failed(String),
}

/// One offer to sync: the server's key, its host, the packs' port and the manifest's hash.
struct Offer {
    key: String,
    host: String,
    port: u16,
    manifest: String,
}

/// The download under way, by server key, and the latest offer that came while it ran.
#[derive(Resource, Default)]
struct Download {
    running: Option<(String, Mutex<Receiver<Report>>)>,
    pending: Option<Offer>,
}

impl Download {
    fn start(&mut self, offer: Offer) {
        let Some(store) = store() else { return };
        let (tx, rx) = channel();
        info!(
            "content: the server offers packs (manifest {}) on port {}",
            offer.manifest, offer.port
        );
        self.running = Some((offer.key.clone(), Mutex::new(rx)));
        std::thread::Builder::new()
            .name("content-download".into())
            .spawn(move || sync(store, offer, tx))
            .ok();
    }
}

fn on_event(
    In(ev): In<SessionEvent>,
    realmlist: Res<crate::realmlist::Realmlist>,
    mut download: ResMut<Download>,
) {
    let SessionEvent::ServerExtensions { extensions } = ev else {
        return;
    };
    let (Some(port), Some(manifest)) = (
        extensions.get(KEY_PORT).and_then(|p| p.parse::<u16>().ok()),
        extensions.get(KEY_MANIFEST).map(str::to_string),
    ) else {
        return;
    };
    let address = realmlist.address().to_string();
    let offer = Offer {
        key: server_key(&address),
        host: split_host(&address).0,
        port,
        manifest,
    };
    if download.running.is_some() {
        download.pending = Some(offer);
    } else {
        download.start(offer);
    }
}

fn sync(store: Store, offer: Offer, tx: Sender<Report>) {
    let mut last: Option<(String, u64, Instant)> = None;
    let result = store.sync(&offer.key, &offer.host, offer.port, &offer.manifest, |p| {
        let percent = p.done * 100 / p.pack.size.max(1);
        // A line each tenth of a pack, and at most one every two seconds.
        let due = match &last {
            Some((name, shown, at)) => {
                name != &p.pack.name
                    || (percent >= shown + 10 && at.elapsed() >= Duration::from_secs(2))
            }
            None => true,
        };
        if due {
            last = Some((p.pack.name.clone(), percent, Instant::now()));
            let _ = tx.send(Report::Progress {
                pack: p.pack.name.clone(),
                percent,
            });
        }
    });
    let _ = tx.send(match result {
        Ok(synced) => Report::Done(synced),
        Err(e) => Report::Failed(format!("{e:#}")),
    });
}

/// The install's `Data` folder, from the start-up that mounted the overlay: what a live remount
/// opens again.
static DATA_DIR: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

/// The interface's refresh signals a table reload raises, each argumentless in 1.12.
const REFRESH_EVENTS: &[&str] = &["SPELLS_CHANGED", "SKILL_LINES_CHANGED", "UPDATE_FACTION"];

/// Make the running client match the packs kept for `key`, as chat lines. The chain is opened
/// again over the new overlay and handed to every reader (the tables', the asset sources' and the
/// interface's), so packs added, replaced, reordered or taken away all apply. What changed, the
/// files of every pack added or taken away, is then read again: the start-up tables that read one
/// ([`crate::live_data`]), the loaded textures, and the interface, rebuilt when its own files
/// changed.
fn apply_live(world: &mut World, key: &str) -> Vec<String> {
    let (Some(store), Some(data)) = (store(), DATA_DIR.get()) else {
        return Vec::new();
    };
    let now = store.mounts(key);
    let before = benilla_formats::overlay();
    if now == before {
        return Vec::new();
    }
    let Some(tables_chain) = world
        .get_resource::<benilla_assets::WorldAssets>()
        .map(|a| a.chain.clone())
    else {
        return Vec::new();
    };
    // The files of the packs taken away, read before their chain goes.
    let mut changed = std::collections::HashSet::new();
    let mut unlisted = false;
    {
        let old = tables_chain.lock_recover();
        for p in before.iter().filter(|p| !now.contains(p)) {
            let listed = old.listed_in(p);
            unlisted |= listed.is_empty();
            changed.extend(listed.iter().map(|n| benilla_formats::chain_name(n)));
        }
    }
    benilla_formats::set_overlay(now.clone());
    let open = || {
        benilla_formats::Chain::open(data).inspect_err(|e| {
            warn!("content: opening the chain over the new packs: {e:#}");
        })
    };
    let Ok(chain) = open() else {
        benilla_formats::set_overlay(before);
        return vec!["This server's new game content could not be loaded.".into()];
    };
    for p in now.iter().filter(|p| !before.contains(p)) {
        let listed = chain.listed_in(p);
        unlisted |= listed.is_empty();
        changed.extend(listed.iter().map(|n| benilla_formats::chain_name(n)));
    }
    changed.retain(|n| !n.starts_with('('));
    *tables_chain.lock_recover() = chain;
    if let Some(mpq) = world.get_resource::<benilla_assets::MpqChain>().cloned() {
        if let Ok(chain) = open() {
            mpq.swap(chain);
        }
    }
    crate::ui_script::reopen();

    let changed = (!unlisted).then_some(changed);
    let tables = crate::live_data::rerun(world, changed.as_ref());
    let assets = reload_assets(world, changed.as_ref());
    // Models, buildings and terrain are built from their files once: what shows them is built
    // again, the world's tiles streamed again and every unit's model rebuilt.
    let world_files = changed
        .as_ref()
        .is_none_or(|c| c.iter().any(|n| is_world_file(n)));
    let catalogs = tables.iter().any(|t| t.ends_with("reload_catalogs"));
    if world_files {
        world.write_message(benilla_world::terrain_stream::RestreamWorld);
    }
    if world_files || catalogs {
        crate::entities::rebuild_all_models(world);
    }
    let interface = changed.as_ref().is_none_or(|c| {
        c.iter()
            .any(|n| n.starts_with("interface\\") && !n.ends_with(".blp"))
    });
    info!(
        "content: {} pack(s) now mounted; {} file(s) changed{}; reloaded {} table loader(s) {:?}, {assets} asset file(s){}{}",
        now.len(),
        changed.as_ref().map_or("every".to_string(), |c| c.len().to_string()),
        if unlisted { " (a pack has no listfile)" } else { "" },
        tables.len(),
        tables,
        if world_files { ", world streamed again" } else { "" },
        if interface { ", interface rebuilt" } else { "" },
    );
    if interface {
        if let Some(mut pending) = world.get_resource_mut::<crate::ui_script::ReloadUiPending>() {
            pending.0 = true;
        }
    } else if !tables.is_empty() {
        if let Err(e) = world.run_system_cached(refresh_interface) {
            debug!("content: the refresh signals did not fire: {e}");
        }
    }
    let mut what = Vec::new();
    if !tables.is_empty() {
        what.push("game data");
    }
    if world_files {
        what.push("world");
    } else if assets > 0 {
        what.push("textures");
    }
    if interface {
        what.push("interface");
    }
    if what.is_empty() {
        return Vec::new();
    }
    vec![format!("This server updated its {}.", what.join(", "))]
}

/// Whether a chain name is part of what the world is built from: terrain, its maps, models and
/// buildings.
fn is_world_file(name: &str) -> bool {
    [".adt", ".wdt", ".wdl", ".m2", ".mdx", ".skin", ".wmo"]
        .iter()
        .any(|ext| name.ends_with(ext))
}

/// Reload every loaded asset among `changed` (none for `None`, which cannot be named), through
/// both `mpq` sources and, for a texture, each sampler mode: the asset behind each handle is
/// replaced, so a texture updates wherever it shows, and a model or tile built again reads the new
/// one. The count of files asked.
fn reload_assets(world: &World, changed: Option<&std::collections::HashSet<String>>) -> usize {
    let (Some(changed), Some(server)) = (changed, world.get_resource::<AssetServer>()) else {
        return 0;
    };
    let mut asked = 0;
    for name in changed {
        let texture = name.ends_with(".blp");
        if !texture && !is_world_file(name) {
            continue;
        }
        asked += 1;
        for source in [benilla_assets::MPQ_SOURCE, benilla_assets::MPQ_LOOSE_SOURCE] {
            if texture {
                for wrap in [(true, true), (true, false), (false, true), (false, false)] {
                    server.reload(benilla_assets::texture_url_on(source, name, wrap));
                }
            } else if name.ends_with(".mdx") || name.ends_with(".m2") {
                server.reload(benilla_assets::m2_url_on(source, name));
            } else {
                server.reload(format!("{source}://{}", name.replace('\\', "/")));
            }
        }
    }
    asked
}

/// The interface's refresh signals ([`REFRESH_EVENTS`]), with the host lent, so the handlers'
/// queries read the reloaded tables.
fn refresh_interface(mut vm: crate::script_calls::HostedVm) {
    vm.run(|script| {
        if let Some(script) = script {
            for event in REFRESH_EVENTS {
                script.fire_event(event, Vec::new());
            }
        }
    })
}

/// Show the download's reports in the chat window, apply what it brought, and start an offer that
/// came while it ran.
fn report(world: &mut World) {
    let Some((key, reports)) = world.get_resource::<Download>().and_then(|d| {
        let (key, rx) = d.running.as_ref()?;
        let reports: Vec<Report> = rx
            .lock()
            .map(|rx| rx.try_iter().collect())
            .unwrap_or_default();
        Some((key.clone(), reports))
    }) else {
        return;
    };
    let mut lines = Vec::new();
    let mut finished = false;
    for r in reports {
        match r {
            Report::Progress { pack, percent } => {
                info!("content: {pack} {percent}%");
                lines.push(format!(
                    "Downloading game content from this server: {pack} {percent}%"
                ));
            }
            Report::Done(synced) => {
                finished = true;
                match synced {
                    Synced::Unchanged => info!("content: up to date"),
                    Synced::Updated { packs, bytes } => {
                        info!("content: {packs} pack(s) kept, {bytes} bytes downloaded")
                    }
                }
                lines.extend(apply_live(world, &key));
            }
            Report::Failed(e) => {
                finished = true;
                warn!("content: download failed: {e}");
                lines.push(format!(
                    "Downloading this server's game content failed: {e}"
                ));
            }
        }
    }
    if let Some(chat) = world.get_resource::<crate::ui_chat::ChatLog>() {
        for line in lines {
            chat.push_event(crate::ui_chat::ChatEvent::text_only(
                crate::ui_chat::ChatEventKind::System,
                line,
            ));
        }
    }
    if finished {
        let mut download = world.resource_mut::<Download>();
        download.running = None;
        if let Some(offer) = download.pending.take() {
            download.start(offer);
        }
    }
}

pub(crate) fn register(app: &mut App) {
    use crate::net::NetHandlerApp;
    use benilla_protocol::SessionEventKind as K;
    app.init_resource::<Download>()
        .net_handler(K::ServerExtensions, on_event)
        .add_systems(Update, report);
}
