use crate::renderer::AndroidRenderer;
use inkframe_core::decode_stroke_samples;
use inkframe_engine::{EngineHost, NativeSurface};
use jni::EnvUnowned;
use jni::objects::{JByteBuffer, JClass, JObject};
use jni::sys::{JNI_FALSE, JNI_TRUE, jboolean, jint, jlong};
use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

static ENGINES: OnceLock<Mutex<HashMap<i64, Arc<EngineHost>>>> = OnceLock::new();
static NEXT_ENGINE_ID: AtomicI64 = AtomicI64::new(1);

fn engines() -> &'static Mutex<HashMap<i64, Arc<EngineHost>>> {
    ENGINES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn engine(id: jlong) -> Option<Arc<EngineHost>> {
    engines().lock().ok()?.get(&id).cloned()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_inkframe_studio_engine_NativeBridge_createEngine<'caller>(
    mut unowned_env: EnvUnowned<'caller>,
    _class: JClass<'caller>,
) -> jlong {
    unowned_env
        .with_env(|_env| -> Result<jlong, jni::errors::Error> {
            let renderer = match AndroidRenderer::new() {
                Ok(renderer) => renderer,
                Err(_) => return Ok(0),
            };
            let id = NEXT_ENGINE_ID.fetch_add(1, Ordering::Relaxed);
            let host = Arc::new(EngineHost::spawn(renderer));
            engines().lock().unwrap().insert(id, host);
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
