#!/usr/bin/env bash
set -euo pipefail

rustup target add aarch64-linux-android x86_64-linux-android
cargo install cargo-ndk --version 4.1.2 --locked

echo "Rust Android targets and cargo-ndk 4.1.2 are ready."
