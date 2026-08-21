#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ENGINE_DIR="$ROOT/engine"
JNI_LIBS_DIR="$ROOT/app/src/main/jniLibs"
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

[[ -f "$ENGINE_DIR/Cargo.toml" ]] || {
  echo "InkFrame Rust workspace not found at $ENGINE_DIR/Cargo.toml" >&2
  exit 1
}

mkdir -p "$JNI_LIBS_DIR"

# cargo-ndk resolves workspace metadata before forwarding Cargo build arguments,
# so execute it from the Rust workspace rather than the repository root.
cd "$ENGINE_DIR"

cargo ndk \
  -t arm64-v8a \
  -t x86_64 \
  -o "$JNI_LIBS_DIR" \
  build \
  --manifest-path "$ENGINE_DIR/Cargo.toml" \
  -p inkframe-android \
  "${RELEASE_ARGS[@]}"
