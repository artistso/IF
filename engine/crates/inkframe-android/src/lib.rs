pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");

// Keep existing module paths concise while avoiding a rustdoc collision with
// this cdylib's required `inkframe_engine` library name.
pub(crate) use inkframe_engine_core as inkframe_engine;

mod stroke;
mod viewport_pixels;

#[cfg(target_os = "android")]
mod android;
#[cfg(target_os = "android")]
mod renderer;
#[cfg(target_os = "android")]
mod viewport;
