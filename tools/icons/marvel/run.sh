#!/bin/sh
# gen + iconcheck + render (scratch). usage: sh tools/icons/marvel/run.sh [ids] [--tile N] [--out PATH]
set -e
cd "$(dirname "$0")/../../.."
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-C:\Sandbox\BrightestDay\game\target\icons}"
python tools/icons/marvel/gen.py
cargo run -q --manifest-path tools/iconcheck/Cargo.toml -- assets/icons/marvel.ron
python tools/icons/marvel/render.py "$@"
