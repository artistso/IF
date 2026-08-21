use crate::stroke::{MAX_BOOTSTRAP_DABS, StrokeDab, StrokePreview};
use crate::viewport::{ViewportCache, transition_swapchain_for_legacy_render};
use crate::viewport_pixels::full_tile_rect;
use ash::{Device, Entry, Instance, khr, vk};
use inkframe_core::StrokeSample;
use inkframe_engine::{NativeSurface, RendererBackend};

const BACKGROUND_COLOR: [f32; 4] = [0.055, 0.055, 0.065, 1.0];
const BACKGROUND_RGB8: [u8; 3] = [14, 14, 17];
const INK_COLOR: [f32; 4] = [0.94, 0.94, 0.98, 1.0];
const PREDICTED_COLOR: [f32; 4] = [0.55, 0.65, 0.90, 1.0];
const PREDICTED_ERASER_COLOR: [f32; 4] = [0.18, 0.20, 0.26, 1.0];

struct SwapchainState {
    loader: khr::swapchain::Device,
    swapchain: vk::SwapchainKHR,
    extent: vk::Extent2D,
    format: vk::Format,
    images: Vec<vk::Image>,
    image_views: Vec<vk::ImageView>,
    render_pass: vk::RenderPass,
    framebuffers: Vec<vk::Framebuffer>,
    command_pool: vk::CommandPool,
    command_buffers: Vec<vk::CommandBuffer>,
    image_available: vk::Semaphore,
    render_finished: vk::Semaphore,
    in_flight: vk::Fence,
    persistent_copy_supported: bool,
}

impl SwapchainState {
    fn new(
        loader: khr::swapchain::Device,
        swapchain: vk::SwapchainKHR,
        extent: vk::Extent2D,
        format: vk::Format,
        persistent_copy_supported: bool,
    ) -> Self {
        Self {
            loader,
            swapchain,
            extent,
            format,
            images: Vec::new(),
            image_views: Vec::new(),
            render_pass: vk::RenderPass::null(),
            framebuffers: Vec::new(),
            command_pool: vk::CommandPool::null(),
            command_buffers: Vec::new(),
            image_available: vk::Semaphore::null(),
            render_finished: vk::Semaphore::null(),
            in_flight: vk::Fence::null(),
            persistent_copy_supported,
        }
    }

    fn destroy(&mut self, device: &Device) {
        unsafe {
            if self.in_flight != vk::Fence::null() {
                device.destroy_fence(self.in_flight, None);
            }
            if self.render_finished != vk::Semaphore::null() {
                device.destroy_semaphore(self.render_finished, None);
            }
            if self.image_available != vk::Semaphore::null() {
                device.destroy_semaphore(self.image_available, None);
            }
            if self.command_pool != vk::CommandPool::null() {
                device.destroy_command_pool(self.command_pool, None);
            }
            for framebuffer in self.framebuffers.drain(..) {
                device.destroy_framebuffer(framebuffer, None);
            }
            if self.render_pass != vk::RenderPass::null() {
                device.destroy_render_pass(self.render_pass, None);
            }
            for view in self.image_views.drain(..) {
                device.destroy_image_view(view, None);
            }
            if self.swapchain != vk::SwapchainKHR::null() {
                self.loader.destroy_swapchain(self.swapchain, None);
            }
        }
        self.images.clear();
        self.command_buffers.clear();
        self.in_flight = vk::Fence::null();
        self.render_finished = vk::Semaphore::null();
        self.image_available = vk::Semaphore::null();
        self.command_pool = vk::CommandPool::null();
        self.render_pass = vk::RenderPass::null();
        self.swapchain = vk::SwapchainKHR::null();
    }
}

pub(crate) struct AndroidRenderer {
    entry: Entry,
    instance: Instance,
    surface_loader: khr::surface::Instance,
    android_surface_loader: khr::android_surface::Instance,
    window: Option<usize>,
    surface: Option<vk::SurfaceKHR>,
    physical_device: Option<vk::PhysicalDevice>,
    queue_family_index: Option<u32>,
    device: Option<Device>,
    graphics_queue: Option<vk::Queue>,
    swapchain: Option<SwapchainState>,
    viewport_cache: Option<ViewportCache>,
    width: u32,
    height: u32,
    last_input_time_ns: i64,
    stroke: StrokePreview,
}

impl AndroidRenderer {
    pub(crate) fn new() -> Result<Self, String> {
        let entry =
            unsafe { Entry::load() }.map_err(|e| format!("Vulkan loader unavailable: {e}"))?;
        let app_name = c"InkFrame";
        let engine_name = c"InkFrame Rust Engine";
        let app_info = vk::ApplicationInfo::default()
            .application_name(app_name)
            .application_version(vk::make_api_version(0, 0, 1, 0))
            .engine_name(engine_name)
            .engine_version(vk::make_api_version(0, 0, 1, 0))
            .api_version(vk::API_VERSION_1_0);
        let extensions = [
            khr::surface::NAME.as_ptr(),
            khr::android_surface::NAME.as_ptr(),
        ];
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
            physical_device: None,
            queue_family_index: None,
            device: None,
            graphics_queue: None,
            swapchain: None,
            viewport_cache: None,
            width: 0,
            height: 0,
            last_input_time_ns: 0,
            stroke: StrokePreview::default(),
        })
    }

    fn release_window(window: usize) {
        if window != 0 {
            unsafe { ndk_sys::ANativeWindow_release(window as *mut _) };
        }
    }

    fn wait_device_idle(&self) {
        if let Some(device) = &self.device {
            let _ = unsafe { device.device_wait_idle() };
        }
    }

    fn destroy_viewport_cache(&mut self) {
        let Some(cache) = self.viewport_cache.take() else {
            return;
        };
        if let Some(device) = &self.device {
            cache.destroy(device);
        }
    }

    fn destroy_swapchain(&mut self) {
        self.destroy_viewport_cache();
        let Some(mut state) = self.swapchain.take() else {
            return;
        };
        if let Some(device) = &self.device {
            state.destroy(device);
        }
    }

    fn destroy_surface(&mut self) {
        self.wait_device_idle();
        self.destroy_swapchain();
        if let Some(surface) = self.surface.take() {
            unsafe { self.surface_loader.destroy_surface(surface, None) };
        }
        if let Some(window) = self.window.take() {
            Self::release_window(window);
        }
        self.width = 0;
        self.height = 0;
    }

    fn destroy_device(&mut self) {
        self.wait_device_idle();
        self.destroy_swapchain();
        if let Some(device) = self.device.take() {
            unsafe { device.destroy_device(None) };
        }
        self.graphics_queue = None;
        self.physical_device = None;
        self.queue_family_index = None;
    }

    fn find_presentation_queue(
        &self,
        surface: vk::SurfaceKHR,
    ) -> Result<(vk::PhysicalDevice, u32), String> {
        let devices = unsafe { self.instance.enumerate_physical_devices() }
            .map_err(|e| format!("enumerate_physical_devices failed: {e:?}"))?;
        for physical_device in devices {
            let families = unsafe {
                self.instance
                    .get_physical_device_queue_family_properties(physical_device)
            };
            for (index, family) in families.iter().enumerate() {
                if !family.queue_flags.contains(vk::QueueFlags::GRAPHICS) {
                    continue;
                }
                let present = unsafe {
                    self.surface_loader.get_physical_device_surface_support(
                        physical_device,
                        index as u32,
                        surface,
                    )
                }
                .map_err(|e| format!("surface support query failed: {e:?}"))?;
                if present {
                    return Ok((physical_device, index as u32));
                }
            }
        }
        Err("no Vulkan graphics queue can present to the Android surface".into())
    }

    fn ensure_device(
        &mut self,
        physical_device: vk::PhysicalDevice,
        queue_family_index: u32,
    ) -> Result<(), String> {
        if self.physical_device == Some(physical_device)
            && self.queue_family_index == Some(queue_family_index)
            && self.device.is_some()
        {
            return Ok(());
        }

        self.destroy_device();
        let queue_priorities = [1.0_f32];
        let queue_info = vk::DeviceQueueCreateInfo::default()
            .queue_family_index(queue_family_index)
            .queue_priorities(&queue_priorities);
        let queue_infos = [queue_info];
        let extensions = [khr::swapchain::NAME.as_ptr()];
        let create_info = vk::DeviceCreateInfo::default()
            .queue_create_infos(&queue_infos)
            .enabled_extension_names(&extensions);
        let device = unsafe {
            self.instance
                .create_device(physical_device, &create_info, None)
        }
        .map_err(|e| format!("vkCreateDevice failed: {e:?}"))?;
        let queue = unsafe { device.get_device_queue(queue_family_index, 0) };

        self.physical_device = Some(physical_device);
        self.queue_family_index = Some(queue_family_index);
        self.graphics_queue = Some(queue);
        self.device = Some(device);
        Ok(())
    }

    fn choose_surface_format(
        formats: &[vk::SurfaceFormatKHR],
    ) -> Result<vk::SurfaceFormatKHR, String> {
        if formats.is_empty() {
            return Err("Vulkan surface exposes no image formats".into());
        }
        if formats.len() == 1 && formats[0].format == vk::Format::UNDEFINED {
            return Ok(vk::SurfaceFormatKHR {
                format: vk::Format::B8G8R8A8_SRGB,
                color_space: vk::ColorSpaceKHR::SRGB_NONLINEAR,
            });
        }
        for preferred in [vk::Format::B8G8R8A8_SRGB, vk::Format::R8G8B8A8_SRGB] {
            if let Some(format) = formats.iter().copied().find(|candidate| {
                candidate.format == preferred
                    && candidate.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
            }) {
                return Ok(format);
            }
        }
        Ok(formats[0])
    }

    fn choose_extent(
        capabilities: vk::SurfaceCapabilitiesKHR,
        width: u32,
        height: u32,
    ) -> vk::Extent2D {
        if capabilities.current_extent.width != u32::MAX {
            return capabilities.current_extent;
        }
        vk::Extent2D {
            width: width.clamp(
                capabilities.min_image_extent.width,
                capabilities.max_image_extent.width,
            ),
            height: height.clamp(
                capabilities.min_image_extent.height,
                capabilities.max_image_extent.height,
            ),
        }
    }

    fn choose_image_count(capabilities: vk::SurfaceCapabilitiesKHR) -> u32 {
        let desired = capabilities.min_image_count.saturating_add(1);
        if capabilities.max_image_count == 0 {
            desired
        } else {
            desired.min(capabilities.max_image_count)
        }
    }

    fn choose_composite_alpha(
        supported: vk::CompositeAlphaFlagsKHR,
    ) -> Result<vk::CompositeAlphaFlagsKHR, String> {
        for candidate in [
            vk::CompositeAlphaFlagsKHR::OPAQUE,
            vk::CompositeAlphaFlagsKHR::PRE_MULTIPLIED,
            vk::CompositeAlphaFlagsKHR::POST_MULTIPLIED,
            vk::CompositeAlphaFlagsKHR::INHERIT,
        ] {
            if supported.contains(candidate) {
                return Ok(candidate);
            }
        }
        Err("Vulkan surface exposes no supported composite-alpha mode".into())
    }

    fn create_swapchain(
        &mut self,
        surface: vk::SurfaceKHR,
        width: u32,
        height: u32,
    ) -> Result<(), String> {
        self.wait_device_idle();
        self.destroy_swapchain();

        let physical_device = self
            .physical_device
            .ok_or_else(|| "logical device has no physical-device owner".to_string())?;
        let queue_family_index = self
            .queue_family_index
            .ok_or_else(|| "logical device has no graphics queue family".to_string())?;
        let device = self
            .device
            .as_ref()
            .ok_or_else(|| "Vulkan logical device is not initialized".to_string())?;

        let capabilities = unsafe {
            self.surface_loader
                .get_physical_device_surface_capabilities(physical_device, surface)
        }
        .map_err(|e| format!("surface capability query failed: {e:?}"))?;
        if !capabilities
            .supported_usage_flags
            .contains(vk::ImageUsageFlags::COLOR_ATTACHMENT)
        {
            return Err("Vulkan surface cannot be used as a color attachment".into());
        }
        let formats = unsafe {
            self.surface_loader
                .get_physical_device_surface_formats(physical_device, surface)
        }
        .map_err(|e| format!("surface format query failed: {e:?}"))?;
        let present_modes = unsafe {
            self.surface_loader
                .get_physical_device_surface_present_modes(physical_device, surface)
        }
        .map_err(|e| format!("present-mode query failed: {e:?}"))?;
        if !present_modes.contains(&vk::PresentModeKHR::FIFO) {
            return Err("Vulkan surface does not expose mandatory FIFO presentation".into());
        }

        let surface_format = Self::choose_surface_format(&formats)?;
        let extent = Self::choose_extent(capabilities, width, height);
        if extent.width == 0 || extent.height == 0 {
            return Err("Vulkan surface extent resolved to zero".into());
        }
        let image_count = Self::choose_image_count(capabilities);
        let composite_alpha = Self::choose_composite_alpha(capabilities.supported_composite_alpha)?;
        let persistent_copy_supported = capabilities
            .supported_usage_flags
            .contains(vk::ImageUsageFlags::TRANSFER_DST)
            && ViewportCache::pixel_encoding_for_format(surface_format.format).is_some();
        let mut image_usage = vk::ImageUsageFlags::COLOR_ATTACHMENT;
        if persistent_copy_supported {
            image_usage |= vk::ImageUsageFlags::TRANSFER_DST;
        }

        let swapchain_loader = khr::swapchain::Device::new(&self.instance, device);
        let create_info = vk::SwapchainCreateInfoKHR::default()
            .surface(surface)
            .min_image_count(image_count)
            .image_format(surface_format.format)
            .image_color_space(surface_format.color_space)
            .image_extent(extent)
            .image_array_layers(1)
            .image_usage(image_usage)
            .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
            .pre_transform(capabilities.current_transform)
            .composite_alpha(composite_alpha)
            .present_mode(vk::PresentModeKHR::FIFO)
            .clipped(true);
        let swapchain = unsafe { swapchain_loader.create_swapchain(&create_info, None) }
            .map_err(|e| format!("vkCreateSwapchainKHR failed: {e:?}"))?;

        let mut state = SwapchainState::new(
            swapchain_loader,
            swapchain,
            extent,
            surface_format.format,
            persistent_copy_supported,
        );
        let build_result = (|| -> Result<(), String> {
            state.images = unsafe { state.loader.get_swapchain_images(state.swapchain) }
                .map_err(|e| format!("vkGetSwapchainImagesKHR failed: {e:?}"))?;
            if state.images.is_empty() {
                return Err("Vulkan swapchain contains no images".into());
            }

            let subresource_range = vk::ImageSubresourceRange::default()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .base_mip_level(0)
                .level_count(1)
                .base_array_layer(0)
                .layer_count(1);
            for &image in &state.images {
                let view_info = vk::ImageViewCreateInfo::default()
                    .image(image)
                    .view_type(vk::ImageViewType::TYPE_2D)
                    .format(surface_format.format)
                    .subresource_range(subresource_range);
                let view = unsafe { device.create_image_view(&view_info, None) }
                    .map_err(|e| format!("vkCreateImageView failed: {e:?}"))?;
                state.image_views.push(view);
            }

            // The swapchain image is explicitly initialized before the render
            // pass: either copied from the persistent viewport or cleared by the
            // legacy fallback. LOAD preserves that base while active/predicted
            // dabs are overlaid.
            let color_attachment = vk::AttachmentDescription::default()
                .format(surface_format.format)
                .samples(vk::SampleCountFlags::TYPE_1)
                .load_op(vk::AttachmentLoadOp::LOAD)
                .store_op(vk::AttachmentStoreOp::STORE)
                .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
                .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
                .initial_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .final_layout(vk::ImageLayout::PRESENT_SRC_KHR);
            let color_reference = vk::AttachmentReference::default()
                .attachment(0)
                .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);
            let color_references = [color_reference];
            let subpass = vk::SubpassDescription::default()
                .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
                .color_attachments(&color_references);
            let dependency = vk::SubpassDependency::default()
                .src_subpass(vk::SUBPASS_EXTERNAL)
                .dst_subpass(0)
                .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
                .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
                .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE);
            let attachments = [color_attachment];
            let subpasses = [subpass];
            let dependencies = [dependency];
            let render_pass_info = vk::RenderPassCreateInfo::default()
                .attachments(&attachments)
                .subpasses(&subpasses)
                .dependencies(&dependencies);
            state.render_pass = unsafe { device.create_render_pass(&render_pass_info, None) }
                .map_err(|e| format!("vkCreateRenderPass failed: {e:?}"))?;

            for &view in &state.image_views {
                let framebuffer_attachments = [view];
                let framebuffer_info = vk::FramebufferCreateInfo::default()
                    .render_pass(state.render_pass)
                    .attachments(&framebuffer_attachments)
                    .width(extent.width)
                    .height(extent.height)
                    .layers(1);
                let framebuffer = unsafe { device.create_framebuffer(&framebuffer_info, None) }
                    .map_err(|e| format!("vkCreateFramebuffer failed: {e:?}"))?;
                state.framebuffers.push(framebuffer);
            }

            let pool_info = vk::CommandPoolCreateInfo::default()
                .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
                .queue_family_index(queue_family_index);
            state.command_pool = unsafe { device.create_command_pool(&pool_info, None) }
                .map_err(|e| format!("vkCreateCommandPool failed: {e:?}"))?;
            let allocation_info = vk::CommandBufferAllocateInfo::default()
                .command_pool(state.command_pool)
                .level(vk::CommandBufferLevel::PRIMARY)
                .command_buffer_count(state.framebuffers.len() as u32);
            state.command_buffers = unsafe { device.allocate_command_buffers(&allocation_info) }
                .map_err(|e| format!("vkAllocateCommandBuffers failed: {e:?}"))?;

            let semaphore_info = vk::SemaphoreCreateInfo::default();
            state.image_available = unsafe { device.create_semaphore(&semaphore_info, None) }
                .map_err(|e| format!("vkCreateSemaphore(image_available) failed: {e:?}"))?;
            state.render_finished = unsafe { device.create_semaphore(&semaphore_info, None) }
                .map_err(|e| format!("vkCreateSemaphore(render_finished) failed: {e:?}"))?;
            let fence_info = vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);
            state.in_flight = unsafe { device.create_fence(&fence_info, None) }
                .map_err(|e| format!("vkCreateFence failed: {e:?}"))?;
            Ok(())
        })();

        if let Err(error) = build_result {
            state.destroy(device);
            return Err(error);
        }
        self.swapchain = Some(state);

        if persistent_copy_supported {
            if let Err(error) = self.rebuild_viewport_cache() {
                self.wait_device_idle();
                self.destroy_swapchain();
                return Err(error);
            }
        }
        Ok(())
    }

    fn rebuild_viewport_cache(&mut self) -> Result<(), String> {
        self.destroy_viewport_cache();
        let (extent, format, command_pool, supported) = {
            let state = self
                .swapchain
                .as_ref()
                .ok_or_else(|| "cannot create viewport cache without a swapchain".to_string())?;
            (
                state.extent,
                state.format,
                state.command_pool,
                state.persistent_copy_supported,
            )
        };
        if !supported {
            return Ok(());
        }
        let physical_device = self
            .physical_device
            .ok_or_else(|| "viewport cache has no physical device".to_string())?;
        let queue = self
            .graphics_queue
            .ok_or_else(|| "viewport cache has no graphics queue".to_string())?;
        let device = self
            .device
            .as_ref()
            .ok_or_else(|| "viewport cache has no logical device".to_string())?;

        let cache = ViewportCache::new(
            &self.instance,
            device,
            physical_device,
            queue,
            command_pool,
            extent,
            format,
            BACKGROUND_COLOR,
        )?;

        let upload_result = {
            let raster = self.stroke.raster();
            let coords: Vec<_> = self.stroke.raster_tile_coords().collect();
            let tiles: Vec<_> = coords
                .into_iter()
                .filter_map(|coord| {
                    raster
                        .tile(coord)
                        .map(|tile| (coord, full_tile_rect(), tile.pixels()))
                })
                .collect();
            cache.upload_tiles(
                &self.instance,
                device,
                physical_device,
                queue,
                command_pool,
                BACKGROUND_RGB8,
                tiles,
            )
        };
        if let Err(error) = upload_result {
            cache.destroy(device);
            return Err(error);
        }

        self.viewport_cache = Some(cache);
        self.stroke.raster_mut().clear_dirty();
        Ok(())
    }

    fn sync_dirty_raster_to_viewport(&mut self) -> Result<(), String> {
        if self.viewport_cache.is_none() {
            return Ok(());
        }
        let (command_pool, physical_device, queue) = {
            let state = self
                .swapchain
                .as_ref()
                .ok_or_else(|| "viewport cache exists without a swapchain".to_string())?;
            (
                state.command_pool,
                self.physical_device
                    .ok_or_else(|| "viewport cache has no physical device".to_string())?,
                self.graphics_queue
                    .ok_or_else(|| "viewport cache has no graphics queue".to_string())?,
            )
        };

        {
            let cache = self.viewport_cache.as_ref().unwrap();
            let device = self
                .device
                .as_ref()
                .ok_or_else(|| "viewport cache has no logical device".to_string())?;
            let tiles: Vec<_> = self.stroke.raster().dirty_tiles().collect();
            cache.upload_tiles(
                &self.instance,
                device,
                physical_device,
                queue,
                command_pool,
                BACKGROUND_RGB8,
                tiles,
            )?;
        }
        self.stroke.raster_mut().clear_dirty();
        Ok(())
    }

    fn dab_rect(dab: StrokeDab, extent: vk::Extent2D) -> Option<vk::ClearRect> {
        let half = dab.diameter.max(1.0) * 0.5;
        let max_x = extent.width as f32;
        let max_y = extent.height as f32;
        let left = (dab.x - half).floor().clamp(0.0, max_x) as i32;
        let top = (dab.y - half).floor().clamp(0.0, max_y) as i32;
        let right = (dab.x + half).ceil().clamp(0.0, max_x) as i32;
        let bottom = (dab.y + half).ceil().clamp(0.0, max_y) as i32;
        if right <= left || bottom <= top {
            return None;
        }
        Some(vk::ClearRect {
            rect: vk::Rect2D {
                offset: vk::Offset2D { x: left, y: top },
                extent: vk::Extent2D {
                    width: (right - left) as u32,
                    height: (bottom - top) as u32,
                },
            },
            base_array_layer: 0,
            layer_count: 1,
        })
    }

    fn clear_background(device: &Device, command_buffer: vk::CommandBuffer, extent: vk::Extent2D) {
        let attachment = vk::ClearAttachment::default()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .color_attachment(0)
            .clear_value(vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: BACKGROUND_COLOR,
                },
            });
        let rect = vk::ClearRect {
            rect: vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent,
            },
            base_array_layer: 0,
            layer_count: 1,
        };
        unsafe {
            device.cmd_clear_attachments(command_buffer, &[attachment], &[rect]);
        }
    }

    fn emit_dab(
        device: &Device,
        command_buffer: vk::CommandBuffer,
        extent: vk::Extent2D,
        dab: StrokeDab,
        predicted: bool,
    ) {
        let Some(rect) = Self::dab_rect(dab, extent) else {
            return;
        };
        let color = if predicted {
            if dab.eraser {
                PREDICTED_ERASER_COLOR
            } else {
                PREDICTED_COLOR
            }
        } else if dab.eraser {
            BACKGROUND_COLOR
        } else {
            INK_COLOR
        };
        let attachment = vk::ClearAttachment::default()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .color_attachment(0)
            .clear_value(vk::ClearValue {
                color: vk::ClearColorValue { float32: color },
            });
        unsafe {
            device.cmd_clear_attachments(command_buffer, &[attachment], &[rect]);
        }
    }

    fn record_frame(
        &self,
        state: &SwapchainState,
        image_index: u32,
    ) -> Result<vk::CommandBuffer, String> {
        let device = self
            .device
            .as_ref()
            .ok_or_else(|| "Vulkan logical device is not initialized".to_string())?;
        let command_buffer = *state
            .command_buffers
            .get(image_index as usize)
            .ok_or_else(|| "swapchain returned an invalid image index".to_string())?;
        let framebuffer = *state
            .framebuffers
            .get(image_index as usize)
            .ok_or_else(|| "swapchain framebuffer index is invalid".to_string())?;
        let swapchain_image = *state
            .images
            .get(image_index as usize)
            .ok_or_else(|| "swapchain image index is invalid".to_string())?;

        unsafe {
            device
                .reset_command_buffer(command_buffer, vk::CommandBufferResetFlags::empty())
                .map_err(|e| format!("vkResetCommandBuffer failed: {e:?}"))?;
            let begin_info = vk::CommandBufferBeginInfo::default();
            device
                .begin_command_buffer(command_buffer, &begin_info)
                .map_err(|e| format!("vkBeginCommandBuffer failed: {e:?}"))?;

            if let Some(cache) = &self.viewport_cache {
                cache.record_copy_to_swapchain(device, command_buffer, swapchain_image);
            } else {
                transition_swapchain_for_legacy_render(device, command_buffer, swapchain_image);
            }
        }

        let render_area = vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent: state.extent,
        };
        let render_pass_begin = vk::RenderPassBeginInfo::default()
            .render_pass(state.render_pass)
            .framebuffer(framebuffer)
            .render_area(render_area);
        unsafe {
            device.cmd_begin_render_pass(
                command_buffer,
                &render_pass_begin,
                vk::SubpassContents::INLINE,
            );
        }

        if self.viewport_cache.is_some() {
            // Completed strokes already live in the persistent viewport cache.
            // Only the still-cancellable actual stroke is drawn as an overlay.
            for &dab in self.stroke.active() {
                Self::emit_dab(device, command_buffer, state.extent, dab, false);
            }
        } else {
            // Compatibility fallback for surfaces without TRANSFER_DST or a
            // supported 32-bit RGBA/BGRA format.
            Self::clear_background(device, command_buffer, state.extent);
            for &dab in self.stroke.committed().iter().take(MAX_BOOTSTRAP_DABS) {
                Self::emit_dab(device, command_buffer, state.extent, dab, false);
            }
        }
        for &dab in self.stroke.predicted() {
            Self::emit_dab(device, command_buffer, state.extent, dab, true);
        }

        unsafe {
            device.cmd_end_render_pass(command_buffer);
            device
                .end_command_buffer(command_buffer)
                .map_err(|e| format!("vkEndCommandBuffer failed: {e:?}"))?;
        }
        Ok(command_buffer)
    }

    fn present_frame(&mut self) -> Result<(), String> {
        let device = self
            .device
            .as_ref()
            .ok_or_else(|| "Vulkan logical device is not initialized".to_string())?;
        let queue = self
            .graphics_queue
            .ok_or_else(|| "Vulkan graphics queue is not initialized".to_string())?;
        let state = self
            .swapchain
            .as_ref()
            .ok_or_else(|| "Vulkan swapchain is not initialized".to_string())?;

        unsafe {
            device
                .wait_for_fences(&[state.in_flight], true, u64::MAX)
                .map_err(|e| format!("vkWaitForFences failed: {e:?}"))?;
        }
        let (image_index, acquire_suboptimal) = unsafe {
            state.loader.acquire_next_image(
                state.swapchain,
                u64::MAX,
                state.image_available,
                vk::Fence::null(),
            )
        }
        .map_err(|e| format!("vkAcquireNextImageKHR failed: {e:?}"))?;
        if acquire_suboptimal {
            return Err("Vulkan swapchain became suboptimal during image acquisition".into());
        }

        let command_buffer = self.record_frame(state, image_index)?;
        unsafe {
            device
                .reset_fences(&[state.in_flight])
                .map_err(|e| format!("vkResetFences failed: {e:?}"))?;
        }
        let wait_semaphores = [state.image_available];
        // Persistent frames touch the acquired image in TRANSFER before the
        // overlay render pass, so acquisition must be visible to both stages.
        let wait_stages =
            [vk::PipelineStageFlags::TRANSFER | vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
        let command_buffers = [command_buffer];
        let signal_semaphores = [state.render_finished];
        let submit_info = vk::SubmitInfo::default()
            .wait_semaphores(&wait_semaphores)
            .wait_dst_stage_mask(&wait_stages)
            .command_buffers(&command_buffers)
            .signal_semaphores(&signal_semaphores);
        unsafe {
            device
                .queue_submit(queue, &[submit_info], state.in_flight)
                .map_err(|e| format!("vkQueueSubmit failed: {e:?}"))?;
        }

        let present_wait = [state.render_finished];
        let swapchains = [state.swapchain];
        let image_indices = [image_index];
        let present_info = vk::PresentInfoKHR::default()
            .wait_semaphores(&present_wait)
            .swapchains(&swapchains)
            .image_indices(&image_indices);
        let present_suboptimal = unsafe { state.loader.queue_present(queue, &present_info) }
            .map_err(|e| format!("vkQueuePresentKHR failed: {e:?}"))?;
        if present_suboptimal {
            return Err("Vulkan swapchain became suboptimal during presentation".into());
        }
        Ok(())
    }

    fn recreate_and_present(
        &mut self,
        surface: vk::SurfaceKHR,
        width: u32,
        height: u32,
    ) -> Result<(), String> {
        let mut last_error = None;
        for _attempt in 0..2 {
            match self.create_swapchain(surface, width, height) {
                Ok(()) => match self.present_frame() {
                    Ok(()) => return Ok(()),
                    Err(error) => {
                        // Presentation may fail after queue submission. Do not tear down
                        // synchronization/render objects until submitted work is idle.
                        self.wait_device_idle();
                        self.destroy_swapchain();
                        last_error = Some(error);
                    }
                },
                Err(error) => last_error = Some(error),
            }
        }
        Err(last_error.unwrap_or_else(|| "Vulkan swapchain presentation failed".into()))
    }

    fn attach_surface_inner(&mut self, target: NativeSurface) -> Result<vk::SurfaceKHR, String> {
        if target.handle == 0 {
            return Err("null ANativeWindow".into());
        }
        let window_ptr = target.handle as *mut _;
        let create_info = vk::AndroidSurfaceCreateInfoKHR::default().window(window_ptr);
        let surface = unsafe {
            self.android_surface_loader
                .create_android_surface(&create_info, None)
        }
        .map_err(|error| format!("vkCreateAndroidSurfaceKHR failed: {error:?}"))?;

        let (physical_device, queue_family_index) = match self.find_presentation_queue(surface) {
            Ok(selection) => selection,
            Err(error) => {
                unsafe { self.surface_loader.destroy_surface(surface, None) };
                return Err(error);
            }
        };
        if let Err(error) = self.ensure_device(physical_device, queue_family_index) {
            unsafe { self.surface_loader.destroy_surface(surface, None) };
            return Err(error);
        }
        if let Err(error) = self.recreate_and_present(surface, target.width, target.height) {
            unsafe { self.surface_loader.destroy_surface(surface, None) };
            return Err(error);
        }
        Ok(surface)
    }
}

impl RendererBackend for AndroidRenderer {
    fn attach_surface(&mut self, target: NativeSurface) -> Result<(), String> {
        self.destroy_surface();
        let surface = self.attach_surface_inner(target)?;
        // ANativeWindow ownership transfers from JNI only after Vulkan surface,
        // device, swapchain, command recording, and first presentation succeed.
        self.window = Some(target.handle);
        self.surface = Some(surface);
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
        if let Some(surface) = self.surface {
            self.recreate_and_present(surface, width, height)?;
        }
        Ok(())
    }

    fn ingest_input(&mut self, samples: &[StrokeSample]) -> Result<(), String> {
        if samples.is_empty() {
            return Ok(());
        }
        self.stroke.ingest(samples);
        if let Some(last) = samples.last() {
            self.last_input_time_ns = last.time_ns;
        }

        let Some(surface) = self.surface else {
            return Ok(());
        };
        if self.width == 0 || self.height == 0 {
            return Ok(());
        }

        // A previous transient failure may have torn down the swapchain. Keep
        // retrying from fresh stylus input instead of silently remaining blank
        // until Android happens to deliver another lifecycle callback.
        if self.swapchain.is_none() {
            return self.recreate_and_present(surface, self.width, self.height);
        }

        // UP may have just sealed persistent raster pixels. Upload only those
        // dirty tile regions before the next base-frame copy. MOVE/prediction
        // packets normally produce no persistent dirt and return immediately.
        self.sync_dirty_raster_to_viewport()?;

        match self.present_frame() {
            Ok(()) => Ok(()),
            Err(first_error) => self
                .recreate_and_present(surface, self.width, self.height)
                .map_err(|recovery_error| {
                    format!(
                        "stroke presentation failed ({first_error}); swapchain recovery failed ({recovery_error})"
                    )
                }),
        }
    }
}

impl Drop for AndroidRenderer {
    fn drop(&mut self) {
        self.destroy_surface();
        self.destroy_device();
        unsafe { self.instance.destroy_instance(None) };
        let _ = &self.entry;
    }
}
