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

## Initial target

The first vertical slice is:

`S Pen -> MotionEvent history -> packed Kotlin buffer -> JNI -> Rust queue -> stroke processor -> Vulkan surface -> display`

The initial Android device focus is Samsung Galaxy S24 FE and Galaxy Tab S10+ while keeping the engine portable enough for other Vulkan-capable Android devices.

## Repository layout

```text
app/                       Kotlin Android application
engine/                    Rust workspace
  crates/inkframe-core/    pure data/math/input contracts
  crates/inkframe-engine/  engine thread + commands/state
  crates/inkframe-android/ JNI + Android NDK/Vulkan bridge
docs/                      architecture and migration documents
scripts/                   reproducible local build helpers
.github/workflows/          CI
```

## Status

Engine bootstrap in progress. See `docs/ARCHITECTURE.md` and `docs/V5_MIGRATION_MAP.md` as they land.
