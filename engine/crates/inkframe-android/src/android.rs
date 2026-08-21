use ash::{khr, vk, Entry, Instance};
use inkframe_core::decode_stroke_samples;
use inkframe_engine::{EngineHost, NativeSurface, RendererBackend};
use jni::objects::{JByteBuffer, JClass, JObject};
use jni::sys::{jboolean, jint, jlong, JNI_FALSE, JNI_TRUE};
use jni::EnvUnowned;
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

struct AndroidRenderer {
    entry: Entry,
    instance: Instance,
    surface_loader: khr::surface::Instance,
    android_surface_loader: khr::android_surface::Instance,
    window: Option<usize>,
    surface: Option<vk::SurfaceKHR>,
    presentation_queue: Option<(vk::PhysicalDevice, u32)>,
    width: u32,
    height: u32,
    last_input_time_ns: i64,
}

impl AndroidRenderer {
    fn new() -> Result<Self, String> {
        let entry = unsafe { Entry::load() }.map_err(|e| format!("Vulkan loader unavailable: {e}"))?;
        let app_name = c"InkFrame";
        let engine_name = c"InkFrame Rust Engine";
        let app_info = vk::ApplicationInfo::default()
            .application_name(app_name)
            .application_version(vk::make_api_version(0, 0, 1, 0))
            .engine_name(engine_name)
            .engine_version(vk::make_api_version(0, 0, 1, 0))
            .api_version(vk::API_VERSION_1_1);
        let extensions = [khr::surface::NAME.as_ptr(), khr::android_surface::NAME.as_ptr()];
        let create_info = vk::InstanceCreateInfo::default()
            .application_info(&app_info)
            .enabled_extension_names(&extensions);
        let instance = unsafe { entry.create_instance(&create_info, None) }
            .map_err(|e| format!("vkCreateInstance failed: {e:?}"))?;
        let surface_loader = khr::surface::Instance::new(&entry, &instance);
        let android_surface_loader = khr::android_surface::Instance::new(&entry, &instance);
        Ok(Self {
            entry,
            instance,
            surface_loader,
            android_surface_loader,
            window: None,
            surface: None,
            presentation_queue: None,
            width: 0,
            height: 0,
            last_input_time_ns: 0,
        })
    }

    fn release_window(window: usize) {
        if window != 0 {
            unsafe { ndk_sys::ANativeWindow_release(window as *mut _) };
        }
    }

    fn destroy_surface(&mut self) {
        if let Some(surface) = self.surface.take() {
            unsafe { self.surface_loader.destroy_surface(surface, None) };
        }
        if let Some(window) = self.window.take() {
            Self::release_window(window);
        }
        self.presentation_queue = None;
        self.width = 0;
        self.height = 0;
    }

    fn find_presentation_queue(&self, surface: vk::SurfaceKHR) -> Result<(vk::PhysicalDevice, u32), String> {
        let devices = unsafe { self.instance.enumerate_physical_devices() }
            .map_err(|e| format!("enumerate_physical_devices failed: {e:?}"))?;
        for device in devices {
            let families = unsafe { self.instance.get_physical_device_queue_family_properties(device) };
            for (index, family) in families.iter().enumerate() {
                if !family.queue_flags.contains(vk::QueueFlags::GRAPHICS) {
                    continue;
                }
                let present = unsafe {
                    self.surface_loader
                        .get_physical_device_surface_support(device, index as u32, surface)
                }
                .map_err(|e| format!("surface support query failed: {e:?}"))?;
                if present {
                    return Ok((device, index as u32));
                }
            }
        }
        Err("no Vulkan graphics queue can present to the Android surface".into())
    }
}

impl RendererBackend for AndroidRenderer {
    fn attach_surface(&mut self, target: NativeSurface) -> Result<(), String> {
        self.destroy_surface();
        if target.handle == 0 {
            return Err("null ANativeWindow".into());
        }
        let window_ptr = target.handle as *mut _;
        let create_info = vk::AndroidSurfaceCreateInfoKHR::default().window(window_ptr);
        let surface = match unsafe {
            self.android_surface_loader
                .create_android_surface(&create_info, None)
        } {
            Ok(surface) => surface,
            Err(error) => {
                Self::release_window(target.handle);
                return Err(format!("vkCreateAndroidSurfaceKHR failed: {error:?}"));
            }
        };
        let queue = match self.find_presentation_queue(surface) {
            Ok(queue) => queue,
            Err(error) => {
                unsafe { self.surface_loader.destroy_surface(surface, None) };
                Self::release_window(target.handle);
                return Err(error);
            }
        };
        self.window = Some(target.handle);
        self.surface = Some(surface);
        self.presentation_queue = Some(queue);
        self.width = target.width;
        self.height = target.height;
        Ok(())
    }

    fn detach_surface(&mut self) {
        self.destroy_surface();
    }

    fn resize(&mut self, width: u32, height: u32) -> Result<(), String> {
        if width == 0 || height == 0 {
            return Err("surface dimensions must be non-zero".into());
        }
        self.width = width;
        self.height = height;
        Ok(())
    }

    fn ingest_input(&mut self, samples: &[inkframe_core::StrokeSample]) -> Result<(), String> {
        if let Some(last) = samples.last() {
            self.last_input_time_ns = last.time_ns;
        }
        Ok(())
    }
}

impl Drop for AndroidRenderer {
    fn drop(&mut self) {
        self.destroy_surface();
        unsafe { self.instance.destroy_instance(None) };
        let _ = &self.entry;
    }
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
            let Some(host) = engine(id) else { return Ok(JNI_FALSE) };
            let window = unsafe {
                ndk_sys::ANativeWindow_fromSurface(
                    env.as_raw() as *mut _,
                    surface.as_raw() as _,
                )
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
            Ok(if engine(id).is_some_and(|host| host.detach_surface().is_ok()) {
                JNI_TRUE
            } else {
                JNI_FALSE
            })
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
            Ok(if engine(id).is_some_and(|host| host.resize(width as u32, height as u32).is_ok()) {
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
            let Some(host) = engine(id) else { return Ok(JNI_FALSE) };
            let capacity = env.get_direct_buffer_capacity(&buffer)?;
            let address = env.get_direct_buffer_address(&buffer)?;
            let bytes = unsafe { std::slice::from_raw_parts(address.cast_const(), capacity) };
            let samples = match decode_stroke_samples(bytes, sample_count as usize) {
                Ok(samples) => samples,
                Err(_) => return Ok(JNI_FALSE),
            };
            Ok(if host.submit_input(samples).is_ok() { JNI_TRUE } else { JNI_FALSE })
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}
