#!/bin/sh
# Starts a Linux build of Benilla. WOW_DATA names the OctoWoW install's Data folder; unset, it is
# "Fresh Install/Data" beside this checkout. Benilla only ever reads that folder.
root="$(cd "$(dirname "$0")/.." && pwd)"
export WOW_DATA="${WOW_DATA:-$root/../Fresh Install/Data}"
for exe in "$root/target/release/benilla" \
           "$root/target/x86_64-unknown-linux-gnu/release/benilla"; do
    if [ -x "$exe" ]; then
        exec "$exe" "$@"
    fi
done
echo "Benilla is not built yet. Run: cargo build --release -p benilla" >&2
exit 1
