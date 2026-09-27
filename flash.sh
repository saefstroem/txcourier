#!/bin/sh
# Flash the firmware to an RP2350 in BOOTSEL mode.
#
# Usually invoked by cargo as the runner for the firmware workspace
# (`cd crates/firmware && cargo run --release`), which passes the built ELF as
# $1. Run it by hand with no arguments to flash the most recent build.
set -e

ROOT=$(cd "$(dirname "$0")" && pwd)

BIN=${1:-$(ls -t "$ROOT"/crates/firmware/target/*/release/* 2>/dev/null \
      | grep -v '\.\(d\|rlib\|json\)$' | head -1)}

if [ -z "$BIN" ] || [ ! -f "$BIN" ]; then
    echo "no firmware binary found — build it first:" >&2
    echo "    cd crates/firmware && cargo build --release" >&2
    exit 1
fi

echo "flashing $BIN"
exec picotool load -u -v -x -t elf "$BIN" -f
