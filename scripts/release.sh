#!/usr/bin/env bash
# Builds the player release of benilla for Windows and Linux, packs one archive per platform plus
# SHA256SUMS.txt, and publishes them as a GitHub release. The OctoWoW game files are not in it:
# Benilla-OctoWoW-Updater keeps those current.
#
#   scripts/release.sh [--no-publish] [tag]
#
# The tag defaults to today's date (2026.11.01, then 2026.11.01-2, ...). Builds go to
# target/dist, apart from the dev builds the play scripts start. Run from Git Bash on Windows;
# the Linux half cross-compiles with the tools in ../tools/linux-cross (or $BENILLA_CROSS_ENV).
set -euo pipefail
cd "$(dirname "$0")/.."

repo=zmarotrix/Benilla-OctoWoW
publish=1
if [ "${1:-}" = "--no-publish" ]; then
    publish=0
    shift
fi

tag=${1:-}
if [ -z "$tag" ]; then
    tag=$(date +%Y.%m.%d)
    n=1
    while [ "$publish" = 1 ] && gh release view "$tag" --repo "$repo" >/dev/null 2>&1; do
        n=$((n + 1))
        tag="$(date +%Y.%m.%d)-$n"
    done
fi

dist=target/dist
out="$dist/release-$tag"
linux=x86_64-unknown-linux-gnu
export CARGO_TARGET_DIR="$dist"

echo "== Windows build"
cargo build --release -p benilla --no-default-features

echo "== Linux build"
# shellcheck disable=SC1090
. "${BENILLA_CROSS_ENV:-../tools/linux-cross/env.sh}"
cargo zigbuild --release -p benilla --no-default-features --target "$linux.2.31"

echo "== Packing $tag"
rm -rf "$out"
mkdir -p "$out"
python -I - "$dist" "$out" "$linux" <<'EOF'
import sys, tarfile, zipfile, os
dist, out, linux = sys.argv[1:]
docs = ["LICENSE-MIT", "LICENSE-APACHE"]
with zipfile.ZipFile(os.path.join(out, "benilla-windows-x64.zip"), "w", zipfile.ZIP_DEFLATED) as z:
    for exe in ["benilla.exe"]:
        z.write(os.path.join(dist, "release", exe), exe)
    for d in docs:
        z.write(d, d)
with tarfile.open(os.path.join(out, "benilla-linux-x64.tar.gz"), "w:gz") as t:
    for exe in ["benilla"]:
        info = t.gettarinfo(os.path.join(dist, linux, "release", exe), exe)
        info.mode = 0o755
        with open(os.path.join(dist, linux, "release", exe), "rb") as f:
            t.addfile(info, f)
    for d in docs:
        t.add(d, d)
EOF
(cd "$out" && sha256sum benilla-windows-x64.zip benilla-linux-x64.tar.gz > SHA256SUMS.txt)
ls -l "$out"

if [ "$publish" = 0 ]; then
    echo "Built $out (not published)."
    exit 0
fi

echo "== Publishing $tag to $repo"
notes="Benilla for OctoWoW, built $(date +%Y-%m-%d).

New here? Unpack the archive for your system into an empty folder, then put the OctoWoW updater (https://github.com/zmarotrix/Benilla-OctoWoW-Updater) beside it: it downloads the OctoWoW game files, or copies them from an install you already have, keeps them current and starts the game. To bring your settings and addons over, use https://github.com/zmarotrix/Benilla-OctoWoW-Transfer."
gh release create "$tag" --repo "$repo" --title "Benilla $tag" --notes "$notes" \
    "$out/benilla-windows-x64.zip" "$out/benilla-linux-x64.tar.gz" "$out/SHA256SUMS.txt"
