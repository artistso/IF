#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODE="${1:-debug}"

case "$MODE" in
  debug) RELEASE_ARGS=() ;;
  release) RELEASE_ARGS=(--release) ;;
  *)
    echo "usage: $0 [debug|release]" >&2
    exit 2
    ;;
esac

command -v cargo >/dev/null 2>&1 || {
  echo "cargo is required" >&2
  exit 127
}

cargo ndk --version >/dev/null 2>&1 || {
  echo "cargo-ndk 4.1.2 is required: cargo install cargo-ndk --version 4.1.2 --locked" >&2
  exit 127
}

mkdir -p "$ROOT/app/src/main/jniLibs"

cargo ndk \
  -t arm64-v8a \
  -t x86_64 \
  -o "$ROOT/app/src/main/jniLibs" \
  build \
  --manifest-path "$ROOT/engine/Cargo.toml" \
  -p inkframe-android \
  "${RELEASE_ARGS[@]}"
