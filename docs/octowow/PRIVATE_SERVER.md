# Benilla on your Rust server

Status on 2026-10-31: Benilla logs in to `G:\Projects\Private Server`, lists the realm and characters,
enters the world and plays. Tested live with `benilla.exe` (account TESTER, character Tester, level 2
Human Mage in Northshire) and with a headless probe.

## How to play

1. Start the server from `G:\Projects\Private Server` (two windows):

   ```
   target\release\authd.exe serve
   target\release\gateway.exe --client-data "G:\Games\WoW Private Server\Turtle WoW Dev\Data" --maps G:\Projects\tortoise-local\server\data\maps
   ```

   authd listens on 127.0.0.1:3724 and advertises the gateway at 127.0.0.1:8085 (their defaults).
2. Double-click `Play Private Server (Benilla).bat`. Log in as usual.

`Play OctoWoW (Benilla).bat` still goes to OctoWoW. The private-server launcher only sets the login
address for that session (`WOW_HOST`), so nothing in `benilla-config` or the WoW install changes.

Port 3724 can only have one owner. If `sniff-proxy` is running (it was, on 2026-10-31, pointed at
127.0.0.1:3726), either stop it, or start authd with `--listen 127.0.0.1:3726` so the proxy relays to it.
If you run authd on any other port, put that port in the launcher (`set "WOW_HOST=127.0.0.1:3727"`).

To test logins without opening the game:

```
cd benilla-main
set WOW_HOST=127.0.0.1
set WOW_USER=TESTER
set WOW_PASS=...
cargo run --release -p benilla-protocol --example private_server_probe
```

## Server settings you need

None for login. Benilla already sends what authd and the gateway accept by default: build 7272 at
login, 5875 in the world handshake. No Warden, no PIN.

Things the server does that Benilla tolerates, worth tidying on the server side:

| What | Effect today | Suggested server change |
|---|---|---|
| `SMSG_INITIALIZE_FACTIONS` has 25 bytes more than 1.12's 64 factions | Benilla reads 64 and logs the extra once | Fine if Turtle needs 69 factions; otherwise send 64 |
| `SMSG_INITIALIZE_FACTIONS`, `SMSG_FRIEND_LIST`, `SMSG_IGNORE_LIST` arrive twice at login | Harmless | Send once |
| `SMSG_SET_REST_START` | Benilla drops it (1.12 has no handler for it either) | None |
| `.xp` and `.owl hello` answered with "There is no such command" | These come from OctoWoW's own interface files (XP boost panel and similar), which ask the OctoWoW server about its features | Either implement them or ignore them silently; see "Capability handshake" below |
| Strict global cooldown and cast-in-progress checks, to the millisecond | A queued spell that arrived a few ms early was refused ("Spell is not ready yet") | Grace window, see below |

### Spell queue grace window

nampower's queue sends the next spell the moment the client thinks the cast or global cooldown ends.
That can land a few ms before the server's own end, and the server used to refuse it ("Spell is
not ready yet").

The fix, on the server side: a cast that only timing stops (the cast under way, or the spell's,
its category's or the global cooldown), and that may go within 200 ms, is held, one per player,
and started the exact moment it may. A cast more than 200 ms early is refused as before. Cancelling
the held spell drops it. This helps every client with a spell queue, not only Benilla.

The server offers it in the handshake as `spell.queue_grace_ms = 200`
(`SPELL_QUEUE_GRACE_MS` in `crates\wow-proto\src\extensions.rs`; the holding is in
`crates\world\src\spells.rs`, `cast_spell` and `start_held_casts`). Benilla needs no change for
it: its queue already sends at its own end of the cooldown, which now always lands inside the
window. Benilla does not send earlier than that, because its own 1.12 cooldown check would refuse
the cast before it leaves the client.

Tested 2026-10-31: the server's test `early_casts_are_held_within_the_grace_window` (a Fireball
sent 1.2 s early is refused, one sent 0.1 s early starts on time), and a live `benilla.exe` run
chaining Frost Armor through the queue: every cast sent was accepted, none refused.

## OctoWoW mod features on this server

All the built-in mod features are client-side, so they keep working on a plain server:
nampower (spell queue, cast events), SuperWoW (extra Lua API, guids), UnitXP (distance and line of
sight), perf_boost (render limits), ClassicAPI and SuperCleveRoidMacros. What does not work is
anything that asks OctoWoW's server (the `.xp` and `.owl` commands above).

The spell queue had a client bug, now fixed: the client's cooldown sweep (on the first spell after
login, every 2 minutes after that, and after any failed cast) deleted the running global cooldown,
so the queue sent the next spell too early and the server refused it.

## Game data

Benilla reads the OctoWoW install (`Fresh Install\Data`), which is close to plain 1.12. Your server
loads Turtle's data. Anything Turtle adds (new races, zones, items, models, spells) is missing or
wrong in Benilla until Benilla reads Turtle's data. Two ways to get there:

1. Point the launcher's `WOW_DATA` at `G:\Games\WoW Private Server\Turtle WoW Dev\Data`. Untested:
   Turtle changes some DBC layouts and its interface expects Turtle's own WoW.exe features.
2. Content streaming (below), where the server ships its data to the client.

## Capability handshake (built 2026-10-31)

Benilla and your server tell each other what they support right after the world login, so each
new feature switches on only when both sides have it.

| Step | Packet | Body |
|---|---|---|
| Benilla, right after `SMSG_AUTH_RESPONSE` OK | `0x334` (`CMSG_LOTTERY_QUERY_OBSOLETE`) | `BNLX`, version 1 (u16), count (u16), then `key\0value\0` pairs |
| Your gateway, in reply | `0x335` (`SMSG_LOTTERY_QUERY_RESULT_OBSOLETE`) | Same format |

Why those numbers: 0x334 is a leftover from the 1.12 client's lottery code. A stock client only
ever sends it empty, so every 1.12 server already ignores it; tested on the Tortoise server, which
logged "will be skipped" and played on. Your gateway answers only a body tagged `BNLX`, so a stock
Turtle client never receives the reply.

Today the lists are: Benilla `client = benilla 0.2.0`; server `server = Private Server 0.1.0`,
`spell.queue_grace_ms = 200`.

**Adding a feature:** add a key to `server_offer()` in
`Private Server\crates\wow-proto\src\extensions.rs`, and to `client_extensions()` in
`benilla-main\crates\benilla-protocol\src\messages\extensions.rs` if the client needs to announce
it. In the game, `ServerExtensions` (`crates\benilla-app\src\server_extensions.rs`) holds what the
server offered; a feature checks `has("its.key")` before using it.

**The `serverExtensions` setting** (in `benilla-config\config.toml`): `auto` (default) sends the
hello only to a server on this PC or your home network; `on` always; `off` never. So playing on
OctoWoW, Benilla sends nothing a stock client wouldn't.

Files:
- Benilla: `crates\benilla-protocol\src\messages\extensions.rs` (format),
  `world\session.rs` (`request_extensions`, `server_extensions`), `crates\benilla-app\src\server_extensions.rs`
  (setting and offered list), `net\io.rs` (sends the hello), `cvars\table.rs` (the setting).
- Server: `crates\wow-proto\src\extensions.rs` (format, offer), `crates\gateway\src\lib.rs` (the
  answer, next to the ping handler).

## Content streaming (built 2026-10-31)

Your server can hand Benilla extra game data (new or changed models, textures, maps, DBC tables,
interface files) as MPQ archives, the same kind of file WoW's own `patch-?.MPQ`s are. Benilla
downloads them into `benilla-config\content\` and loads them on top of the install straight away,
while the player plays, and again at every start. It never writes into the WoW folder.

**On the server:** make a folder (for example `data\packs`), put the MPQs players should get in it,
and add `--packs data\packs` to the gateway command. `--packs-listen` sets where they are served
(default `127.0.0.1:8086`; use `0.0.0.0:8086` for players on other machines). Archives load in
name order, a later one winning over an earlier one and over the player's own files. The first
start fingerprints each archive (a few seconds for a big one) and remembers the result in
`packs.sha256` in that folder. To change content, change the folder: no restart of anything (see
"Live updates" below).

**What a player sees:** when something is new, chat shows the download's progress, then what
changed ("This server updated its game data, world, interface."). A download that stops part way
resumes next time.

**How it stays safe:**
- Every file is checked by its SHA-256 fingerprint. The list of fingerprints itself is checked
  against the one your server sends over the logged-in game connection, so nobody between the two
  can swap a file.
- Content mounts whole or not at all: if any listed archive is missing, Benilla loads none of them
  and plays on the plain install.
- It follows the same `serverExtensions` rule as the handshake: by default only a server on this
  PC or home network can send content.
- Downloaded archives can change the interface too, so only use it with servers you trust (yours).

How it works underneath:

| Step | What |
|---|---|
| Handshake | Server adds `content.port` and `content.manifest` (the list's SHA-256) |
| Benilla | `GET http://<login host>:<port>/packs/<sha256>` for the list, then each archive it lacks |
| List format | `benilla-packs 1`, then one line per archive: `<sha256> <size> <file name>` |
| Kept in | `benilla-config\content\packs\<sha256>.mpq`, and the list per server in `content\servers\` |
| While playing | The archives load at once ("Live updates" below) |
| Every start | Benilla loads that server's archives above the install (`$WOW_HOST`, else `realmList`) |

Files: server `crates\streaming` (folder scan, list, web server) and the `--packs` option in
`crates\gateway\src\main.rs`; Benilla `crates\benilla-content` (download and checks),
`crates\benilla-app\src\content_stream.rs` (login and start-up), and `set_overlay` in
`crates\benilla-formats\src\chain.rs` (loading the archives on top).

Tested 2026-10-31 with a test archive holding a `Spell.dbc` that renames Frost Armor rank 3: the
first login downloaded and checked it (28 MB), the second start loaded it and the spellbook showed
"Streamed Frost Armor"; the second login found nothing new to download. Automated tests: 4 on the
server (`streaming`), 6 in Benilla (`benilla-content`: download, resume, tampered file refused,
partial set not loaded), 1 for loading on top of the install (`benilla-formats`).

Not yet: removing archives a server no longer lists from the disk (they stay in `content\packs`
until deleted, unused), and per-file updates instead of whole archives.

## Live updates (built 2026-11-01)

Anything you put in, change in or take out of the gateway's `--packs` folder reaches every Benilla
player in the world within about six seconds, with no restart and no loading screen:

| What changed | What the player sees |
|---|---|
| Data tables (`DBFilesClient\*.dbc`: spells, talents, skills, factions, areas, items' looks, sounds, ...) | The tables that use the changed file are read again; the spellbook, skills and reputation windows refresh at once |
| Textures (`.blp`) | Every loaded copy is replaced; whatever shows it updates in place |
| Models, buildings, terrain (`.m2`, `.mdx`, `.skin`, `.wmo`, `.adt`, `.wdt`) | The world around the player is streamed again and every creature, player and object model is rebuilt, in place |
| Interface files (`Interface\...` Lua, XML, TOC) | The interface rebuilds itself automatically, the same as `/reload` (about a second; chat and windows come back) |
| A pack taken away or replaced | All of the above, back to what the remaining packs and the install hold |

So a hotfix is: make an MPQ with the changed files, give it a name that sorts after your other
packs (`patch-z-hotfix1.mpq`), copy it into the folder. To undo it, delete it.

The update costs a short pause in the game: about half a second to a second for an interface
rebuild or for the very first update of a session (Benilla then learns which files each table
uses, so later updates only re-read the tables that changed).

Remember the server keeps its own copy of spell and item data: a hotfix changes what players see
and what their client calculates (tooltips, ranges, cast times shown), while what actually happens
is still decided by the server's own data. Change both.

Tested 2026-11-01 with Benilla in the world and three changes 20 seconds apart: a spell-table pack
added (the spellbook showed "Live Frost Armor" at once), the same pack removed (back to "Frost
Armor"), then a pack with a spell table, an interface file, a texture and a terrain tile added (the
world streamed again, every model rebuilt, the interface rebuilt and ran the streamed file).

How it works: the gateway watches the packs folder (`streaming::start`) and, when the list
changes, sends the handshake answer again to every Benilla connection that opened the handshake
(`offer_changes` in `crates\streaming`, the extra `select!` arm in `crates\gateway\src\lib.rs`).
Benilla's `content_stream.rs` downloads what is new, then `apply_live` opens the archive set again
over the new list and hands it to every reader (tables, models and textures, interface), works out
which files changed from the packs' file lists, and reloads: the start-up table loaders that read a
changed file (`live_data.rs`, 49 loaders, each learning its files with `record_reads`), the loaded
textures and models (`AssetServer::reload`), the world (`RestreamWorld` in
`benilla-world\src	errain_stream.rs`), every unit model (`entities::rebuild_all_models`), and
the interface (`ReloadUI`) when its files changed.

Not live yet: a few tables Benilla reads only at the moment it needs them and then keeps (for
example some sound and map overlays) pick a change up the next time they are read, not at once.

## Server and client features (proposal)

These need changes on both sides. They go on top of Benilla's faithful 1.12 core, behind the
handshake above, so Benilla still works on stock servers and a stock client still works on yours.

1. ~~**Capability handshake.**~~ Done, see above.
2. ~~**Content streaming.**~~ Built, see "Content streaming" above.
3. ~~**Live data hotfixes.**~~ Built for every kind of content, see "Live updates" above. Next step
   there: the admin site writing the hotfix archive itself when you edit a spell.
4. **Cache refresh.** The server tells the client which item, creature, quest or object entries
   changed; Benilla drops those cache entries and asks again. Fixes stale tooltips after an edit.
5. ~~**Server-side spell queue.**~~ Done, see "Spell queue grace window" above.
6. **GM and admin helpers.** Client-side views for `.lookup` results and the content editor (open
   an entity's admin page from a target), using the same extension channel.

Next candidates: cache refresh (4), and hotfixes made straight from the admin site's editor (3).
