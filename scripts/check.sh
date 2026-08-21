#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

cargo test --manifest-path "$ROOT/engine/Cargo.toml" -p inkframe-core -p inkframe-engine
cargo check --manifest-path "$ROOT/engine/Cargo.toml" -p inkframe-android
cargo fmt --manifest-path "$ROOT/engine/Cargo.toml" --all -- --check
