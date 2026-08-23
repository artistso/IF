use crate::perf::{SharedFrameTimingStats, shared_frame_timing_stats};
use crate::renderer::AndroidRenderer;
use inkframe_core::decode_stroke_samples;
use inkframe_engine::{BrushSettings, EngineHost, NativeSurface};
use jni::EnvUnowned;
use jni::objects::{JByteBuffer, JClass, JObject};
use jni::sys::{JNI_FALSE, JNI_TRUE, jboolean, jfloat, jint, jlong, jstring};
use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

struct EngineEntry {
    host: Arc<EngineHost>,
    perf_stats: SharedFrameTimingStats,
}

static ENGINES: OnceLock<Mutex<HashMap<i64, EngineEntry>>> = OnceLock::new();
static NEXT_ENGINE_ID: AtomicI64 = AtomicI64::new(1);

fn engines() -> &'static Mutex<HashMap<i64, EngineEntry>> {
    ENGINES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn engine(id: jlong) -> Option<Arc<EngineHost>> {
    engines()
        .lock()
        .ok()?
        .get(&id)
        .map(|entry| Arc::clone(&entry.host))
}

fn performance_summary(id: jlong) -> Option<String> {
    let entries = engines().lock().ok()?;
    let stats = entries.get(&id)?.perf_stats.lock().ok()?;
    Some(stats.debug_summary())
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_inkframe_studio_engine_NativeBridge_createEngine<'caller>(
    mut unowned_env: EnvUnowned<'caller>,
    _class: JClass<'caller>,
) -> jlong {
    unowned_env
        .with_env(|_env| -> Result<jlong, jni::errors::Error> {
            let perf_stats = shared_frame_timing_stats();
            let renderer = match AndroidRenderer::new(Arc::clone(&perf_stats)) {
                Ok(renderer) => renderer,
                Err(_) => return Ok(0),
            };
            let id = NEXT_ENGINE_ID.fetch_add(1, Ordering::Relaxed);
            let host = Arc::new(EngineHost::spawn(renderer));
            engines()
                .lock()
                .unwrap()
                .insert(id, EngineEntry { host, perf_stats });
            Ok(id)
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_inkframe_studio_engine_NativeBridge_destroyEngine<'caller>(
    mut unowned_env: EnvUnowned<'caller>,
    _class: JClass<'caller>,
    id: jlong,
) {
    unowned_env
        .with_env(|_env| -> Result<(), jni::errors::Error> {
            let removed = engines().lock().unwrap().remove(&id);
            drop(removed);
            Ok(())
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_inkframe_studio_engine_NativeBridge_attachSurface<'caller>(
    mut unowned_env: EnvUnowned<'caller>,
    _class: JClass<'caller>,
    id: jlong,
    surface: JObject<'caller>,
    width: jint,
    height: jint,
) -> jboolean {
    unowned_env
        .with_env(|env| -> Result<jboolean, jni::errors::Error> {
            if width <= 0 || height <= 0 {
                return Ok(JNI_FALSE);
            }
            let Some(host) = engine(id) else {
                return Ok(JNI_FALSE);
            };
            let window = unsafe {
                ndk_sys::ANativeWindow_fromSurface(env.get_raw() as *mut _, surface.as_raw() as _)
            };
            if window.is_null() {
                return Ok(JNI_FALSE);
            }
            let target = NativeSurface {
                handle: window as usize,
                width: width as u32,
                height: height as u32,
            };
            match host.attach_surface(target) {
                Ok(()) => Ok(JNI_TRUE),
                Err(_) => {
                    unsafe { ndk_sys::ANativeWindow_release(window) };
                    Ok(JNI_FALSE)
                }
            }
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_inkframe_studio_engine_NativeBridge_detachSurface<'caller>(
    mut unowned_env: EnvUnowned<'caller>,
    _class: JClass<'caller>,
    id: jlong,
) -> jboolean {
    unowned_env
        .with_env(|_env| -> Result<jboolean, jni::errors::Error> {
            Ok(
                if engine(id).is_some_and(|host| host.detach_surface().is_ok()) {
                    JNI_TRUE
                } else {
                    JNI_FALSE
                },
            )
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_inkframe_studio_engine_NativeBridge_resizeSurface<'caller>(
    mut unowned_env: EnvUnowned<'caller>,
    _class: JClass<'caller>,
    id: jlong,
    width: jint,
    height: jint,
) -> jboolean {
    unowned_env
        .with_env(|_env| -> Result<jboolean, jni::errors::Error> {
            if width <= 0 || height <= 0 {
                return Ok(JNI_FALSE);
            }
            Ok(
                if engine(id).is_some_and(|host| host.resize(width as u32, height as u32).is_ok()) {
                    JNI_TRUE
                } else {
                    JNI_FALSE
                },
            )
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_inkframe_studio_engine_NativeBridge_setBrush<'caller>(
    mut unowned_env: EnvUnowned<'caller>,
    _class: JClass<'caller>,
    id: jlong,
    color_rgb: jint,
    size_px: jfloat,
    opacity: jfloat,
    eraser: jboolean,
) -> jboolean {
    unowned_env
        .with_env(|_env| -> Result<jboolean, jni::errors::Error> {
            let Some(host) = engine(id) else {
                return Ok(JNI_FALSE);
            };
            let rgb = color_rgb as u32;
            let settings = BrushSettings {
                color_srgb: [
                    ((rgb >> 16) & 0xff) as u8,
                    ((rgb >> 8) & 0xff) as u8,
                    (rgb & 0xff) as u8,
                ],
                size_px,
                opacity,
                eraser: eraser != JNI_FALSE,
            };
            Ok(if host.set_brush(settings).is_ok() {
                JNI_TRUE
            } else {
                JNI_FALSE
            })
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_inkframe_studio_engine_NativeBridge_pushInput<'caller>(
    mut unowned_env: EnvUnowned<'caller>,
    _class: JClass<'caller>,
    id: jlong,
    buffer: JByteBuffer<'caller>,
    sample_count: jint,
) -> jboolean {
    unowned_env
        .with_env(|env| -> Result<jboolean, jni::errors::Error> {
            if sample_count < 0 {
                return Ok(JNI_FALSE);
            }
            let Some(host) = engine(id) else {
                return Ok(JNI_FALSE);
            };
            let capacity = env.get_direct_buffer_capacity(&buffer)?;
            let address = env.get_direct_buffer_address(&buffer)?;
            let bytes = unsafe { std::slice::from_raw_parts(address.cast_const(), capacity) };
            let samples = match decode_stroke_samples(bytes, sample_count as usize) {
                Ok(samples) => samples,
                Err(_) => return Ok(JNI_FALSE),
            };
            Ok(if host.submit_input(samples).is_ok() {
                JNI_TRUE
            } else {
                JNI_FALSE
            })
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_inkframe_studio_engine_NativeBridge_performanceStats<'caller>(
    mut unowned_env: EnvUnowned<'caller>,
    _class: JClass<'caller>,
    id: jlong,
) -> jstring {
    unowned_env
        .with_env(|env| -> Result<jstring, jni::errors::Error> {
            let summary = performance_summary(id).unwrap_or_else(|| "unavailable".to_string());
            Ok(env.new_string(summary)?.into_raw())
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}
