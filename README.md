# Benilla for OctoWoW

Benilla is a from-scratch World of Warcraft 1.12.1 client written in Rust, here built for the
OctoWoW server with every OctoWoW client mod (nampower, SuperWoW, UnitXP_SP3, perf_boost,
ClassicAPI and others) compiled in. It runs natively on Windows and Linux.

This repository holds the release builds. Benilla's source is MIT / Apache-2.0 licensed.

## Install

1. Download the archive for your system from the
   [latest release](https://github.com/zmarotrix/Benilla-OctoWoW/releases/latest):
   `benilla-windows-x64.zip` or `benilla-linux-x64.tar.gz`.
2. Unpack it into an empty folder.
3. Run `benilla-launcher`.

On the first start the launcher asks for the OctoWoW game files: it can download them from the
official OctoWoW mirror (about 9 GB), or copy them from an OctoWoW install you already have,
which is much faster. That install is only read, never changed.

After that, the launcher keeps Benilla and the game files up to date every time you open it,
downloading only what changed, and starts the game when you press Play. Your settings, addons
and saved data live in `benilla-config` and are never touched by updates.

On Linux without a desktop, `benilla-launcher --update` does the same in a terminal
(add `--download` or `--import <an OctoWoW install>` for the first install).

The game files come from OctoWoW's own servers; this repository never hosts them.
