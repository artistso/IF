#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MANIFEST="$ROOT/engine/Cargo.toml"

cargo fmt --manifest-path "$MANIFEST" --all -- --check
cargo test --manifest-path "$MANIFEST" -p inkframe-core -p inkframe-engine -p inkframe-raster -p inkframe-android

# The JNI/Vulkan bridge is cfg(target_os = "android"), so a host-only check does
# not compile the code that ships in the APK. Check both Android ABIs explicitly
# so JNI/NDK API drift fails in the fast Rust gate rather than late in Gradle.
cargo check --manifest-path "$MANIFEST" -p inkframe-android --target aarch64-linux-android
cargo check --manifest-path "$MANIFEST" -p inkframe-android --target x86_64-linux-android
