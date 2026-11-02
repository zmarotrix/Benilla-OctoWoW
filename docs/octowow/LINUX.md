# Benilla for OctoWoW on Linux

Benilla is a native Linux program: no Wine, no Proton. It reads the OctoWoW install and never
writes into it; its settings, addons and saved data live in `benilla-config`.

The easy way: download `benilla-linux-x64.tar.gz` from the
[latest release](https://github.com/zmarotrix/Benilla-OctoWoW/releases/latest), unpack it into an
empty folder, and put the [OctoWoW updater](https://github.com/zmarotrix/Benilla-OctoWoW-Updater)
beside it. The updater fetches the OctoWoW game files and starts the game. Install the packages in
step 2 first. The rest of this page is for building from source.

## 1. What to copy to the Linux machine

Copy the whole `OctoWoW Benilla` folder, keeping this layout:

```
OctoWoW Benilla/
  Fresh Install/          the OctoWoW install (Data, WoW.exe, fmod.dll, ...)
  benilla-main/           the Benilla source (or just the built program, see step 3)
  ```

Copy all of `Fresh Install`, not only `Data`. Benilla does not run `WoW.exe` or the DLLs, but it
reads `WoW.exe`, `fmod.dll`, `ijl15.dll`, `dbghelp.dll` and `unicows.dll` to answer the server's
version check.

## 2. System packages

**To run** (most desktops already have these):

| Distro | Command |
|---|---|
| Ubuntu / Debian / Mint / Pop!_OS | `sudo apt install libasound2 libudev1 libvulkan1 mesa-vulkan-drivers libxkbcommon-x11-0 libxcursor1 libxrandr2 libxi6 libwayland-client0 zenity xdg-utils` |
| Fedora | `sudo dnf install alsa-lib systemd-libs vulkan-loader mesa-vulkan-drivers libxkbcommon-x11 libXcursor libXrandr libXi libwayland-client zenity xdg-utils` |
| Arch / Manjaro / SteamOS | `sudo pacman -S alsa-lib systemd-libs vulkan-icd-loader libxkbcommon-x11 libxcursor libxrandr libxi wayland zenity xdg-utils` plus your GPU's Vulkan driver (`vulkan-radeon`, `vulkan-intel` or `nvidia-utils`) |

What they are for: ALSA is sound (PipeWire and PulseAudio provide it too), udev is gamepads,
Vulkan is the graphics (NVIDIA users need the proprietary driver), the X11/Wayland libraries are
the window, keyboard and mouse cursor, `zenity` shows the "WoW install not found" message, and
`xdg-utils` lets the login screen's buttons open the AddOns, Screenshots and Logs folders.

**To build** on Linux, add a C compiler, pkg-config and the ALSA and udev development files:

| Distro | Command |
|---|---|
| Ubuntu / Debian | `sudo apt install build-essential pkg-config libasound2-dev libudev-dev` |
| Fedora | `sudo dnf install gcc pkgconf-pkg-config alsa-lib-devel systemd-devel` |
| Arch | `sudo pacman -S base-devel pkgconf alsa-lib systemd-libs` |

and Rust from <https://rustup.rs> (the project picks its own Rust version on the first build).

## 3. Build

Either build on the Linux machine:

```sh
cd "OctoWoW Benilla/benilla-main"
cargo build --release -p benilla
```

The first build takes a while (several minutes); later ones are faster. The program lands in
`benilla-main/target/release/benilla`.

Or build on Windows and copy the result over (see "Building the Linux version on Windows"
below). That program lands in `benilla-main/target/x86_64-unknown-linux-gnu/release/benilla`.

## 4. Play

```sh
cd "OctoWoW Benilla/benilla-main"
chmod +x scripts/play-octowow-benilla.sh
./scripts/play-octowow-benilla.sh
```

The launcher points Benilla at `Fresh Install/Data` and starts whichever build it finds.
To use a different copy of the game data, set it yourself:

```sh
WOW_DATA="/path/to/OctoWoW/Data" ./benilla-main/target/release/benilla
```

## 5. Your settings and addons

- Addons go in `benilla-config/AddOns` (the login screen's AddOns button opens it). Folder and
  file names may use any upper/lower case, as on Windows.
- To bring over a normal client's settings, put its `WTF` folder into `benilla-config/WTF`. The
  next launch takes in its settings, key bindings, macros, layout, chat, addon on/off lists and
  saved variables, backs up what it replaces, then renames the folder so it runs once.
  `benilla-config/wtf-import.log` lists what moved.
- To move Benilla itself between PCs (Windows to Linux too), copy the `benilla-config` folder,
  or use [Benilla-OctoWoW-Transfer](https://github.com/zmarotrix/Benilla-OctoWoW-Transfer).

## Building the Linux version on Windows

This is how the current Linux build was made. It needs no Linux machine, but it cannot run or
test the game; that needs Linux (or WSL2).

1. `rustup target add x86_64-unknown-linux-gnu`
2. Install Zig and pkgconf (for example `python -m pip install --target <tools> ziglang pkgconf`)
   and `cargo install --locked cargo-zigbuild`.
3. Unpack Debian 11's `libasound2`, `libasound2-dev`, `libudev1` and `libudev-dev` packages
   (from deb.debian.org) into a folder used as the sysroot.
4. In Git Bash, set `PKG_CONFIG` to `pkgconf.exe`, `PKG_CONFIG_ALLOW_CROSS=1`,
   `PKG_CONFIG_DONT_DEFINE_PREFIX=1`, `PKG_CONFIG_SYSROOT_DIR=<sysroot>` and
   `PKG_CONFIG_LIBDIR=<sysroot>/usr/lib/x86_64-linux-gnu/pkgconfig`, put Zig on `PATH`, then:

   ```sh
   cargo zigbuild --release -p benilla --target x86_64-unknown-linux-gnu.2.31
   ```

The `.2.31` suffix builds against glibc 2.31, so the program runs on Ubuntu 20.04, Debian 11,
Fedora 32 and anything newer.

## Known limits

- Like the Windows build, Benilla does not answer Warden.
- Graphics run through Vulkan. The graphics library can fall back to OpenGL when no Vulkan
  driver is found, but Benilla has not been tested that way, so install your GPU's Vulkan driver.
