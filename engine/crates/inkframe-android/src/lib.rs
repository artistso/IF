pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(target_os = "android")]
mod android;
