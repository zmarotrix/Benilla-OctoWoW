# OctoWoW mod coverage in Benilla

Goal: every feature and setting of OctoWoW's client mods built into Benilla, so addons written for
those mods work unchanged, and every mod setting lives in **Benilla Settings**.

Legend: ✅ done · 🟡 partial · ⬜ to do · ➖ not needed (Benilla's engine already covers it)

## Not needed: performance and compatibility fixes for the old engine

| Mod | What it does | Benilla |
|---|---|---|
| VanillaFixes (`VanillaFixes.exe`, `VfPatcher.dll`) | Timer resolution and CPU-frequency fixes, DLL loader | ➖ modern engine; no DLLs are loaded |
| `weirdperformance.dll` | Spatial culling | ➖ Benilla has its own culling |
| DXVK (`d3d9.dll`, `dxvk.conf`) | DirectX 9 → Vulkan; 16x MSAA, 16x anisotropic, tear-free vsync | ➖ Benilla renders natively; map the quality options below |
| `no1600x1200.dll` | Display-mode workaround | ➖ |

## Vanilla Tweaks (`WoW_Tweaked.exe`)

| Tweak | Original → tweaked | Benilla |
|---|---|---|
| Field of view | 1.92 → 2.12 rad (≈110° → 122°) | ✅ `FoV` CVar and a Field of View slider in Graphics |
| Sound channels | 12 → 64 | ⬜ (shared with SuperWoW `UncapSounds`) |
| Distance limit (max camera distance) | 50 → 100 | ✅ camera zooms out to 100 yd; the Max Camera Distance slider reaches it |
| Distance constant | 3000 → 1500 | ⬜ identify |
| Distance constant | 70 → 300 | ⬜ identify |

## SuperWoW (`SuperWoWhook.dll`, companion addon SuperAPI)

**Functions and events**
- ✅ `SUPERWOW_VERSION`, `SUPERWOW_STRING`
- ✅ `ImportFile`, `ExportFile` (sandboxed to `benilla-config/imports`)
- ✅ GUID and `mark1`–`mark8` unit tokens; `UnitExists` returns the GUID
- ✅ `UnitBuff`/`UnitDebuff` spell IDs, `GetPlayerBuffID`, `SpellInfo`
- ✅ `CastSpellByName(name, unit)`
- ✅ `SetMouseoverUnit`, `SetAutoloot`, `Clickthrough`
- ✅ `UnitPosition` (self and group only)
- ✅ `UNIT_CASTEVENT`, `RAW_COMBATLOG`, `CombatLogAdd`, raw log file
- ✅ `TrackUnit`, `UntrackUnit`
- ✅ `owner` token suffix (`targetowner`, `mouseoverowner`, any token + `owner`)
- ✅ `CastSpellByName(name, "CLICK")` (ground spell placed at the cursor)
- ✅ `UnitMana`/`UnitManaMax` druid caster mana as the second return
- ⬜ `UnitNameplate`, `frame:GetName(1)` on nameplates
- ✅ `CanLootUnit`, `CursorPosition` (the ground under the mouse)
- ⬜ `LootSlot(slot, force)`
- ⬜ `GetWorldLocMapPosition`, `GetMapPositionWorldLoc`, `GetMapBoundaries`
- ✅ `IsSwimming`, `IsMounted`, `isIndoors`, `GetSpeed`
- ⬜ `GetSendMailItemLink`, `GetInboxItemLink`, `GetWeaponEnchantID`, `GetWeaponEnchantInfo(unit)`
- ⬜ `GetQuestIDForLogIndex`, `GetQuestLinkForLogIndex`, `GetQuestLinkForID`, quest and spell links
- ⬜ `GetActionText` extra returns, `/tooltip` macro reads, 510-character macros
- ⬜ `GetContainerItemInfo` charges, `SetRaidTarget(unit, i, "local")`
- ⬜ `CREATE_CHATBUBBLE` event

**Settings** (SuperAPI uses the first six)
- ✅ `BackgroundSound` (Benilla Settings → Audio)
- ⬜ `FoV`, `SelectionCircleStyle`, `LootSparkle`, `UncapSounds`, `SoundSoftwareChannels`
- ⬜ `InterfaceVolume`, `HealingText`, `ChatBubbleRange`, `NameplateRange`, `NameplateMotion`

## UnitXP SP3 (`UnitXP_SP3.dll`, companion addon UnitXP_SP3_Addon)

All through `UnitXP("command", ...)`.
- ✅ `nop`, `version` (presence check addons use; pfUI and HealersMate detect UnitXP)
- ✅ `distanceBetween` (all five meters: ranged, `AoE`, `meleeAutoAttack`, `chains`, `Gaussian`)
- ✅ `inSight` (eye-height sweep through Benilla's world collision), `behind` (90° threshold)
- ⬜ `behindThreshold` setting; `behind`'s in-combat facing correction
- ✅ Targeting (`UnitXP("target", …)`): `nearestEnemy`, `mostHP`, `worldBoss`,
  `nextEnemyInCycle`, `previousEnemyInCycle`, `nextEnemyConsideringDistance` (living, attackable,
  in sight, within 41 yd)
- ⬜ Targeting: `previousEnemyConsideringDistance`, marked-enemy cycling, `rangeCone`, `farRange`,
  `disableInCombatFilter`
- ⬜ `timer` (`arm`, `disarm`, `size`)
- ⬜ `notify` (`taskbarIcon`, `systemSound`)
- ✅ Frame-rate caps (`maxFPS`, `maxFPSBk`, sliders in Graphics; 30 fps in the background by default)
- ⬜ `modernNameplateDistance`, `hideEXPtext`, `addCombatText`,
  `performanceProfile`, camera commands
- ⬜ Settings in Benilla Settings: FPS caps, targeting cone and range, nameplate distance, XP text

## nampower (`nampower.dll`, companion addon NampowerSettings)

- ✅ **Spell queueing** (the main feature), built into the cast path: a spell pressed in the last
  `NP_SpellQueueWindowMs` (500 ms) of a cast or of its cooldown/GCD is held and cast the moment the
  client allows, with `SPELL_QUEUE_EVENT`; `NP_QueueCastTimeSpells`, `NP_QueueInstantSpells` and
  `NP_QueueChannelingSpells` switch it per kind. `QueueSpellByName`, `CastSpellByNameNoQueue`,
  `/unqueue` and `[queuedspell]` use it. Not yet: on-swing queueing, the buffer-time settings
- 🟡 49 `NP_*` settings registered (addons can read and set them), e.g. `NP_QueueCastTimeSpells`, `NP_SpellQueueWindowMs`,
  `NP_QuickcastTargetingSpells`, `NP_NameplateDistance`, `NP_PreventRightClickTargetChange`
- 🟡 Functions done: `GetNampowerVersion`, `GetUnitGUID`, `GetUnitData`, `GetUnitField`, `GetSpellRec`, `GetSpellRecField`, `GetSpellIconTexture`, `GetSpellIdForName`, `GetSpellNameAndRankForId`, `GetSpellSlotTypeIdForName`, `IsSpellUsable`, `IsSpellInRange`, `GetSpellIdCooldown`, `GetItemIdCooldown`, `GetTrinketCooldown`, `UseTrinket`, `GetCastInfo`, `GetCurrentCastingInfo`, `QueueScript`, `QueueSpellByName`, `CastSpellByNameNoQueue`, `ChannelStopCastingNextTick`, `GetSpellModifiers` (reports none)
- ✅ Item reads: `GetEquippedItems`, `GetBagItems`, `GetTrinkets`, `FindPlayerItemSlot`, `UseItemIdOrName`, `GetSpellRangeData`
- ⬜ Remaining functions: `GetItemStats`,
  `GetEquippedItems`, `CancelPlayerAuraSlot`, `DisenchantAll`, `EncryptPassword`,
  `EncryptedServerLogin`, file functions, and more
- ✅ Events: `SPELL_START/GO/FAILED/DELAYED/MISS_*`, `SPELL_CAST_EVENT`, `SPELL_CHANNEL_START`, `AURA_CAST_ON_*`, `AUTO_ATTACK_*`, `SPELL_DAMAGE_EVENT_*`, `SPELL_HEAL_*`, `BUFF/DEBUFF_ADDED/REMOVED_*`, `UNIT_DIED`, `UNIT_*_GUID`, `UNIT_COMBAT_GUID`, each behind its `NP_Enable*` setting
- ✅ `SPELL_QUEUE_EVENT`
- ⬜ `SPELL_ENERGIZE_*`, the remaining unit-event flags

## perf_boost (`perf_boost.dll`, companion addon PerfBoostSettings)

- ✅ All 30 `PB_*` settings registered, so PerfBoostSettings works as shipped
- ✅ Render distances for players (normal, in combat, in cities), pets, summons and totems, trash
  mobs and corpses; show-all, hide-all, always/never lists by name, always-draw PvP; the target,
  your own pet and summons always draw. Player Draw Distance is also in Graphics and the
  Low/Medium presets
- ✅ Raid-marked units always draw (`PB_AlwaysRenderRaidMarks`)
- ⬜ Doodad distance (Detail Distance covers it), spell-visual filters (`PB_HiddenSpellIds`,
  `PB_ShowPlayerSpellVisuals`, …)

## VanillaHelpers (`VanillaHelpers.dll`)

- ⬜ `GetItemDisplayID`, `UnitDisplayInfo`, `ReadFile`, `WriteFile`
- ⬜ `SetUnitDisplayID`, `SetUnitMountDisplayID`, `SetUnitVisibleItemID`
- ⬜ `RemapDisplayID`, `RemapMountDisplayID`, `RemapVisibleItemID`
- ⬜ `SetUnitBlip`, `SetObjectTypeBlip` (custom minimap icons)

## World markers (`worldmarkers.dll`, embedded addon WeirdUtils_WorldMarkers)

- ⬜ `WorldMarker`, `ClearWorldMarker`, `GetWorldMarker`, `CanSetWorldMarker`
- ⬜ `SetMarkerSync`, `ClearMarkerSync`, keybindings, markers drawn on the ground

## ClassicAPI (`ClassicAPI.dll`)

- 🟡 Lua-level backports done: `strsplit`, `strjoin`, `strtrim`, `strreplace`, `string.reverse`, `math.modf`, `table.wipe`/`wipe`, `table.count`, `Mixin`, `CreateFromMixins`, `C_Timer` (`After`, `NewTimer`, `NewTicker`), `C_ColorUtil`, `C_EncodingUtil` (hex, Base64), `GetCoinTextureString`, `UnitGUID`, `UnitPower*`, `UnitHealthMissing`, `UnitClassBase`, `UnitRaceBase`, `UnitIsInMyGuild`, `UnitNameFromGUID`, `UnitTokenFromGUID`, `UnitInRange`, `UnitInLineOfSight`, `UnitDistanceSquared`, `GetItemIcon`, `GetInventoryItemID`, `C_Item` basics with item sets (`GetItemSetIDByID`, `GetItemSetInfo`, `UseAtCursor`), `C_Container` basics, `C_EquipmentSet` (saved per character in `benilla-config`, plus `/equipset` and `/equipsave`), `C_LossOfControl` (school lockouts), `C_Spell.CastAtCursor`, `UnitSpellHaste`, `GetTotemInfo`, `GetTotemTimeLeft`, `GetSpellInfo`, `IsSpellKnown`, `IsPlayerSpell`, `IsUsableSpell`, `FindSpellBookSlotByID`, `C_CVar`, `ClassicExpansionAtLeast/AtMost`
- ✅ Widget methods on every frame type: `SetSize`, `GetSize`, `SetShown`, `HookScript`, `SetAttribute`, `GetAttribute`, `SetEnabled`; textures' `SetColorTexture`; font strings' `SetFormattedText`; `GetMinimapShape`
- ✅ `CastingInfo`, `ChannelInfo`, `UnitCastingInfo`, `UnitChannelInfo` (any unit the client times), `InCombatLockdown`
- ⬜ The rest of the 274: binding overrides, tooltip extensions, quest and reputation IDs, JSON/CBOR/compression, grouped as `C_AddOns`, `C_AuctionHouse`,
  `C_ColorUtil`, `C_Container`, `C_EncodingUtil`, `C_EquipmentSet`, `C_FriendList`,
  `C_GossipInfo`, `C_Item`, `C_Loot`, `C_LossOfControl`, `C_Macro`, …, plus `CastingInfo`,
  `ChannelInfo`, `InCombatLockdown`, `HookScript`, focus (`FocusUnit`, `ClearFocus`) and
  `Model:SetDisplayInfo`/`SetCreature`

## SuperCleveRoidMacros (built in natively, not as an addon)

Implemented in the engine (`crates/benilla-ui/src/script/smartmacro`): each command's argument is
compiled once and cached, conditions read client state directly, and the stock slash handlers are
replaced so typed commands and macros share it.

- ✅ Grammar: `;` clauses, `[..][..]` fallback groups, `[]`, `@unit`, comma/space separators, `no`
  negation, all short aliases (`cd`, `ic`/`ooc`, `melee`, …), `/` OR and `&` AND values,
  comparisons (`>`, `<`, `>=`, `<=`, `=`, `~=`), `#` stack comparisons, chained comparisons
  (`>20&<50`), `_`/quoted names, `(Rank N)`, `!` no-toggle, `~` cancel-aura, `?` no-tooltip
- ✅ Commands: `/cast`, `/use`, `/target` (with conditional scanning), `/castsequence` (reset
  timer, target, combat, modifiers), `/castpet`, `/startattack`, `/stopattack`, `/stopcasting`,
  `/stopchanneling`, `/unqueue`, `/cleartarget`, `/cancelaura` (`/unbuff`), `/unshift`,
  `/stopmacro`, `/skipmacro`, `/firstaction`, `/nofirstaction`, the six pet commands, `/equip`,
  `/equipmh`, `/equipoh`, `/equip11`–`14`, `/applymain`, `/applyoff`, `/retarget`, `/runmacro`,
  `/rl`, `/macrocheck`, `/quickheal` (`/qh`, when QuickHeal is loaded), `{MacroName}` nesting
- ✅ Live action buttons: a conditional macro's bound spell follows its conditions ten times a
  second, so its cooldown, usability and range on the bar track the spell it would cast now;
  `#showtooltip` icon and tooltip
- ✅ Mouseover from any unit frame of any addon (no per-addon patches)
- ✅ Count mode (`[meleerange:>1]`, `[behind:meleerange>0]`, `[distance:30:facing>2]`,
  `[inrange:Multi-Shot>1]`, …) and `[multiscan:…]` (nearest, farthest, HP, raw HP, mark order,
  each mark), over a native nearby-units query
- ✅ Debuff/buff time left on other units for your own auras, timed natively from your casts
- ✅ Unit conditions: alive, dead, exists, help, harm, isplayer, isnpc, playercontrolled, member,
  party, raid, tag, mytag, othertag, name, class, type, level, hp, rawhp, hplost, power, mana,
  rage, energy, rawpower, powerlost, powertype, combat, buff, debuff, buffcapped, debuffcapped,
  cc, magic, curse, disease, poison, dispellable, dispellablebuff, magicbuff, distance,
  meleerange, inrange, outrange, insight, behind, facing, targeting, cursive
- ✅ Player conditions: mod, button, keydown*, hastarget, notarget, focus, group, bg, zone,
  resting, combo, form/stance, stealth, mounted, swimming, indoors, outdoors, moving (speed %),
  rooted, standing/sitting, myhp, myrawhp, myhplost, mylevel, mypower, mymana, myrage, myenergy,
  myrawpower, mypowerlost, druidmana, mybuff, mydebuff, mybuffcapped, mydebuffcapped,
  mybuffcount, mycc, cooldown, cdgcd, gcd, usable, reactive, known, inbag, equipped,
  selfcasting, channeled, checkcasting, checkchanneled, pet, pethappiness, mhimbue, ohimbue,
  stat
- 🟡 Read through addons where loaded: `istank` (pfUI), `threat` (TWThreat)
- ✅ Swing and ranged timers, timed natively off the swing packets: `swingtimer`/`stimer`,
  `rangedtimer`/`rtimer`, `lastswing`, `incominghit`, `slamclip`, `nextslamclip`, `rangedclip`,
  `onswingpending`
- ✅ `keydown` (`IsKeyDown`)
- ✅ School lockout (`locked`, read off the server's lockout cooldown list), `totem` (the totem
  bar, timed natively), item `set` piece counts, `equipset`, `myspellhaste`, `queuedspell`,
  `[cursor]` placement (ground spells drop at the mouse, no reticle click)
- ✅ `spellcasttime` (exact, off the cast-time table and haste), `casting`/`casttime`/
  `channeltime` on any unit (timed natively off its cast packets and channel field), `ttk`/`tte`
  (a fitted health trend per unit)
- ✅ `resisted` (full or partial, off our own spell logs)
- ✅ Immunity learning (`[immune]`, `[immune:fire]`): each creature's school and mechanic
  immunities learned from our own spells' immune misses, kept across sessions in
  `benilla-config/immunities.tsv`
- ⬜ Overflow buff frame, combo-point duration tracking

## AuctionQueryThrottle (`AuctionQueryThrottle.dll`)

- ✅ The next auction browse query opens the moment the server's results arrive, not 5 s later
  (Benilla Settings: `fastAuctionQueries`, on by default)

## Lexara 1.12 (`lexara112.dll`, HD MSDF font renderer)

- ➖ Sharp text at every size: Benilla's text engine rasterizes each glyph at the exact
  device-pixel size it is drawn at, so scaled text never blurs (the problem Lexara fixes)
- ➖ Its two crash fixes (missing-texture fill, character-skin mip overflow) patch the old
  engine's texture code, which Benilla does not have

## Benilla's own performance options (Settings → Graphics)

- ✅ Performance Preset (Low, Medium, High, Ultra): one choice sets render scale, anti-aliasing,
  view and detail distance, grass, weather, spell effects, player draw distance, texture size and
  filtering
- ✅ Texture size (`baseMip`): 1 draws every model and building texture at half size, a quarter
  of the video memory, for weak or integrated GPUs (Low preset; applies after a restart)
- ✅ Saved texture filtering (`anisotropic`) now always applies; before, an early boot texture
  could freeze the default over it
- ✅ Frame-rate caps, Field of View, extended view and detail distance, player draw distance

## Small fixes

- ✅ Shop previews by display ID (`SetUnit(number)`), the behaviour these mods supply
- ⬜ `transmogfix.dll` (transmog appearance coalescing): identify behaviour
- ⬜ `customassets.dll` (custom asset loading): identify behaviour
