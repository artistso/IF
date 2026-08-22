pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");

mod brush_geometry;
mod stroke;
mod viewport_pixels;

#[cfg(target_os = "android")]
mod android;
#[cfg(target_os = "android")]
mod brush_pipeline;
#[cfg(target_os = "android")]
mod renderer;
#[cfg(target_os = "android")]
mod viewport;
