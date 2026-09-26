#!/bin/sh
# gen + iconcheck + render (scratch). usage: sh tools/icons/tmnt/run.sh [ids] [--rows] [--tile N] [--out PATH]
set -e
cd "$(dirname "$0")/../../.."
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-C:\Sandbox\BrightestDay\game\target\icons}"
python tools/icons/tmnt/gen.py
cargo run -q --manifest-path tools/iconcheck/Cargo.toml -- assets/icons/tmnt.ron
python tools/icons/tmnt/render.py "$@"
