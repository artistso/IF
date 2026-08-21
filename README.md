# InkFrame

InkFrame is a native Android drawing and frame-by-frame animation application built around a custom Rust graphics/animation engine and a Kotlin/Jetpack Compose Android shell.

This repository is the clean successor to `artistso/inkframesv5`. The v5 repository is a reference for product behavior and proven algorithms; it is **not** copied wholesale. Rendering, document state, stroke processing, timeline evaluation, undo/redo, canvas geometry, and persistence are being rebuilt as engine-owned Rust systems.

## Architecture

```text
Kotlin / Jetpack Compose
        |
        | JNI + packed direct buffers
        v
Rust InkFrame Engine
        |
        +-- input / stroke processing
        +-- brush dynamics
        +-- canvas + document model
        +-- layers + animation timeline
        +-- history / persistence
        +-- Vulkan renderer
        |
        v
Android NDK / Vulkan / GPU
```

## Design rules

1. Kotlin owns Android lifecycle, Compose UI, platform pickers, permissions, and raw `MotionEvent` capture.
2. Rust owns authoritative artwork/document state and all performance-critical graphics/animation logic.
3. The artwork is rendered by Vulkan, not Compose Canvas.
4. JNI is a narrow transport boundary; high-rate input is passed in packed direct buffers.
5. Predicted stylus samples are transient and never become committed artwork.
6. Large raster documents use sparse tiled storage rather than monolithic bitmaps.
7. Canvas shape/crop is nondestructive metadata/masking, not destructive resampling.
8. Local files remain first-class. No account, subscription, cloud backend, or AI service is required.

## First vertical slice

The first execution path is:

`S Pen -> MotionEvent history -> packed Kotlin direct buffer -> JNI -> Rust decoder -> bounded engine queue -> Vulkan Android surface`

The initial Android device focus is Samsung Galaxy S24 FE and Galaxy Tab S10+ while keeping the engine portable to other Vulkan-capable Android devices.

## Repository layout

```text
app/                       Kotlin Android application
engine/                    Rust workspace
  crates/inkframe-core/    pure data/math/input contracts
  crates/inkframe-engine/  engine thread + commands/state
  crates/inkframe-android/ JNI + Android NDK/Vulkan bridge
docs/                      architecture and migration documents
scripts/                   reproducible build/check helpers
.github/workflows/          CI
```

## Current bootstrap status

Implemented now:

- native Kotlin/Compose application shell using the existing `com.inkframe.studio` package identity;
- `SurfaceView` lifecycle bridge into Rust;
- historical S Pen/stylus samples encoded into a fixed 32-byte little-endian record;
- bounded, non-blocking Rust engine command queue;
- Rust brush/document/canvas seed model, including exposure-held sparse cels;
- Vulkan loader + instance + Android `VkSurfaceKHR` creation;
- physical-device/graphics-present queue-family validation;
- ARM64 and x86_64 Rust/Android build script;
- Rust unit checks and Android debug-build CI.

Not implemented yet:

- Vulkan logical device and swapchain;
- frame synchronization and command buffers;
- visible clear/present pass;
- GPU brush stamping and scratch-stroke compositing;
- sparse raster tile allocation/residency;
- full timeline/layers UI;
- `.inkframe` compatibility decoder/export pipeline.

That distinction is intentional: the repository should never claim a rendered brush path before one actually exists.

## Build prerequisites

- JDK 17
- Android SDK Platform 37
- Android NDK `28.2.13676358`
- Rust stable
- Rust Android targets `aarch64-linux-android` and `x86_64-linux-android`
- `cargo-ndk` `4.1.2`
- Gradle `9.5.0` (CI is pinned to this version)

Bootstrap Rust tooling:

```bash
./scripts/bootstrap-rust-android.sh
```

Run the Rust checks:

```bash
./scripts/check.sh
```

Build the Android debug APK with a local Gradle 9.5 installation:

```bash
gradle :app:assembleDebug
```

A generated Gradle wrapper is intentionally not copied from v5. It should be generated cleanly from the pinned Gradle toolchain rather than transplanting an old repository binary.

## Migration documents

- `docs/ARCHITECTURE.md` — native runtime ownership, JNI packet ABI, threading and rendering direction.
- `docs/V5_MIGRATION_MAP.md` — what is preserved behaviorally from v5 and what is explicitly discarded.
