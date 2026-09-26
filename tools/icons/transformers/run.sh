#!/bin/sh
# gen + iconcheck + render (scratch). usage: sh tools/icons/transformers/run.sh [ids] [--tile N]
set -e
cd "$(dirname "$0")/../../.."
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-C:\Sandbox\BrightestDay\game\target\icons}"
python tools/icons/transformers/gen.py
cargo run -q --manifest-path tools/iconcheck/Cargo.toml -- assets/icons/transformers.ron
python tools/icons/transformers/render.py "$@"
