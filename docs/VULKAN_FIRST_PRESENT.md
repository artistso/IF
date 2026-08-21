# Vulkan First-Present Milestone

This milestone proves that the native Rust engine owns a real Android Vulkan presentation path before any brush or document rendering is layered on top.

## Integration status

The native surface lifecycle/JNI foundation has been merged into `main`. This Vulkan milestone is retargeted directly to `main`, so its CI acceptance run validates the renderer against the same base that will receive the merge.

## Runtime path

```text
SurfaceView
  -> ANativeWindow
  -> VkSurfaceKHR
  -> physical device + graphics/present queue family
  -> VkDevice + VK_KHR_swapchain
  -> VkSwapchainKHR
  -> image views
  -> render pass + framebuffers
  -> command buffers
  -> acquire image
  -> clear color attachment
  -> queue submit
  -> queue present
```

## Deliberate constraints

- One graphics/present queue family.
- FIFO presentation for broad Android compatibility and deterministic pacing.
- One frame in flight while the renderer is still only a correctness probe.
- Render-pass clear rather than a graphics pipeline; brush pipelines come after presentation is proven.
- Swapchain recreation is synchronous on surface resize.
- The Android `ANativeWindow` reference transfers to Rust only after surface, logical device, swapchain, and the first present all succeed.

## What this milestone does not claim

- It does not yet render artwork.
- It does not yet implement low-latency front-buffer drawing.
- It does not yet implement brush stamping, scratch-stroke compositing, tiled raster storage, or layer compositing.
- A CI-successful Android cross-compile proves build correctness, not physical-device presentation. Device validation remains a separate acceptance gate.

## Acceptance gates

1. `cargo fmt --check` passes.
2. Host Rust unit tests pass.
3. ARM64 Android Rust cross-compile passes.
4. x86_64 Android Rust cross-compile passes.
5. Android debug APK assembles.
6. Physical Android device shows the engine clear color after surface attach.
7. Rotation/resize recreates the swapchain without crash or leaked native windows.
8. Background/foreground surface destruction and recreation remains stable.

Only after these gates do we begin the pressure-sensitive brush renderer.
