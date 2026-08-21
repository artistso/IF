# InkFrame v5 -> IF Migration Map

`artistso/inkframesv5` is a behavioral reference. It is not a source tree to copy wholesale.

| v5 concept | IF destination | Decision |
|---|---|---|
| `core-common` math | `inkframe-core` | Reimplement in Rust with equivalent tests |
| `core-model` document/brush types | `inkframe-core` | Reimplement in Rust; Rust becomes authoritative |
| `engine-gl` | `inkframe-android` + future renderer crate | Replace OpenGL ES with Vulkan |
| `feature-canvas` input/view | Kotlin SurfaceView + Rust engine | Keep Android input capture; move canvas logic to Rust |
| `feature-layers` | Compose UI + Rust document commands | UI only in Kotlin; mutations execute in Rust |
| GL `EngineEvent` queue | `EngineHost` bounded command queue | Preserve message-passing architecture |
| historical stylus samples | Kotlin packet encoder | Preserve |
| Catmull-Rom / spatial resampling | Rust stroke processor | Preserve behavior, retest numerically |
| scratch-stroke compositing | Vulkan brush pass | Preserve no-overlap-darkening semantics |
| dirty-region stroke undo | Rust history/tile diffs | Preserve principle; adapt to sparse tiles |
| sparse cels + exposure holds | Rust animation model | Preserve |
| viewport inverse mapping | Rust math + renderer uniforms | Preserve exact inverse invariant |
| `.inkframe` ZIP package | Rust document I/O + Android SAF edge | Preserve format compatibility where practical |
| onion skin planning | Rust animation evaluator | Preserve, later generalize to multiple tracks |
| GIF/MP4/PNG export | Rust planner + Android MediaCodec bridge | Rebuild after core renderer is deterministic |

## Explicitly not migrated

- flattened Git internals and repository artifacts;
- generated agent bundles and patches;
- OpenGL ES renderer implementation;
- build/release plumbing that is unrelated to the native runtime;
- Java/Kotlin copies of algorithms whose authoritative home is now Rust;
- any architecture that duplicates document state on both sides of JNI.

## Compatibility strategy

The Android package id remains `com.inkframe.studio`. `compileSdk` may move forward independently of `targetSdk`; target changes require an explicit behavior audit. Project-file compatibility will be implemented through a versioned decoder rather than by copying the old serializer into the new runtime.
