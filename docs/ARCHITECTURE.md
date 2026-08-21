# InkFrame Native Architecture

## Ownership boundary

InkFrame is one Android product with two deliberately separate runtime domains.

- **Kotlin / Android** owns Activity lifecycle, Jetpack Compose, accessibility, system pickers, raw `MotionEvent` collection and the Android `Surface` object.
- **Rust** owns the authoritative document, input/stroke processing, brushes, tiles, layers, timeline evaluation, history, persistence orchestration and rendering.
- **Vulkan** owns artwork presentation. Compose never rasterizes document artwork.

```text
Android UI thread
  MotionEvent + Compose commands
            |
            | packed direct ByteBuffer / narrow JNI methods
            v
Rust EngineHost command queue
            |
            v
Rust engine/render thread
  document + stroke processor + renderer
            |
            v
Vulkan -> ANativeWindow -> Android Surface
```

## JNI contract

JNI is transport, not application architecture. Engine objects never leak as JVM object graphs.

High-frequency stylus samples use a direct `ByteBuffer` with a fixed 32-byte little-endian record:

| Offset | Type | Meaning |
|---:|---|---|
| 0 | f32 | canvas/view x |
| 4 | f32 | canvas/view y |
| 8 | f32 | pressure |
| 12 | f32 | tilt |
| 16 | f32 | orientation |
| 20 | i64 | event time in nanoseconds |
| 28 | u32 | sample flags |

The Rust decoder validates `sampleCount * 32 <= buffer capacity` before reading.

## Threads

1. Android UI thread captures input and sends commands.
2. Rust `EngineHost` owns a bounded command queue and a dedicated engine thread.
3. The Vulkan renderer lives on the engine thread in the first implementation. A separate render thread is permitted later only when profiling proves it beneficial.
4. File encoding/export workers must never block stylus ingestion.

A bounded queue is intentional. An unbounded queue can convert a temporary render stall into hundreds of megabytes of delayed stylus data and seconds of perceived latency.

## Document storage

Raster content is designed around sparse tiles rather than one image per layer. The initial tile key is `(layer/cel surface, tileX, tileY)`; tile dimensions will be benchmarked before being frozen. Canvas shape is a nondestructive clip/mask above document space.

Animation uses sparse cels keyed by frame. Empty frames resolve to the most recent preceding cel (traditional exposure hold) unless a future explicit blank/stop marker overrides the hold.

## Stroke model

The v5 behavioral contract is retained:

1. ingest current + historical stylus samples;
2. smooth/interpolate/resample in space rather than stamping raw event positions;
3. accumulate normal-brush coverage into a per-stroke scratch target without overlap darkening;
4. composite the completed stroke once at whole-stroke opacity;
5. retain a separate build-up mode for airbrush-like media;
6. record only the dirty region needed for undo.

Predicted samples are always transient preview data. They are replaced by real samples and never enter committed document history.

## First vertical slice

The first executable milestone is intentionally narrow:

`S Pen -> MotionEvent history -> direct buffer -> JNI -> Rust decoder -> bounded engine queue -> Vulkan surface`

After this path is measurable and stable, brush rasterization is added to the same renderer rather than prototyped in Compose or Android Canvas.
