use ash::{Device, Entry, Instance, khr, vk};
use inkframe_core::StrokeSample;
use inkframe_engine::{NativeSurface, RendererBackend};

struct SwapchainState {
    loader: khr::swapchain::Device,
    swapchain: vk::SwapchainKHR,
    format: vk::Format,
    extent: vk::Extent2D,
    image_views: Vec<vk::ImageView>,
    render_pass: vk::RenderPass,
    framebuffers: Vec<vk::Framebuffer>,
    command_pool: vk::CommandPool,
    command_buffers: Vec<vk::CommandBuffer>,
    image_available: vk::Semaphore,
    render_finished: vk::Semaphore,
    in_flight: vk::Fence,
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
    width: u32,
    height: u32,
    last_input_time_ns: i64,
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
            .api_version(vk::API_VERSION_1_1);
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

    fn wait_device_idle(&self) {
        if let Some(device) = &self.device {
            let _ = unsafe { device.device_wait_idle() };
        }
    }

    fn destroy_swapchain(&mut self) {
        let Some(state) = self.swapchain.take() else {
            return;
        };
        let Some(device) = &self.device else {
            return;
        };

        unsafe {
            device.destroy_fence(state.in_flight, None);
            device.destroy_semaphore(state.render_finished, None);
            device.destroy_semaphore(state.image_available, None);
            device.destroy_command_pool(state.command_pool, None);
            for framebuffer in state.framebuffers {
                device.destroy_framebuffer(framebuffer, None);
            }
            device.destroy_render_pass(state.render_pass, None);
            for view in state.image_views {
                device.destroy_image_view(view, None);
            }
            state.loader.destroy_swapchain(state.swapchain, None);
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
        let device = unsafe { self.instance.create_device(physical_device, &create_info, None) }
            .map_err(|e| format!("vkCreateDevice failed: {e:?}"))?;
        let queue = unsafe { device.get_device_queue(queue_family_index, 0) };

        self.physical_device = Some(physical_device);
        self.queue_family_index = Some(queue_family_index);
        self.graphics_queue = Some(queue);
        self.device = Some(device);
        Ok(())
    }

    fn choose_surface_format(formats: &[vk::SurfaceFormatKHR]) -> Result<vk::SurfaceFormatKHR, String> {
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
        let swapchain_loader = khr::swapchain::Device::new(&self.instance, device);
        let create_info = vk::SwapchainCreateInfoKHR::default()
            .surface(surface)
            .min_image_count(image_count)
            .image_format(surface_format.format)
            .image_color_space(surface_format.color_space)
            .image_extent(extent)
            .image_array_layers(1)
            .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
            .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
            .pre_transform(capabilities.current_transform)
            .composite_alpha(composite_alpha)
            .present_mode(vk::PresentModeKHR::FIFO)
            .clipped(true);
        let swapchain = unsafe { swapchain_loader.create_swapchain(&create_info, None) }
            .map_err(|e| format!("vkCreateSwapchainKHR failed: {e:?}"))?;

        let result = self.finish_swapchain_state(
            swapchain_loader,
            swapchain,
            surface_format.format,
            extent,
            queue_family_index,
        );
        if let Err(error) = result {
            return Err(error);
        }
        Ok(())
    }

    fn finish_swapchain_state(
        &mut self,
        loader: khr::swapchain::Device,
        swapchain: vk::SwapchainKHR,
        format: vk::Format,
        extent: vk::Extent2D,
        queue_family_index: u32,
    ) -> Result<(), String> {
        let device = self
            .device
            .as_ref()
            .ok_or_else(|| "Vulkan logical device is not initialized".to_string())?;
        let images = match unsafe { loader.get_swapchain_images(swapchain) } {
            Ok(images) => images,
            Err(error) => {
                unsafe { loader.destroy_swapchain(swapchain, None) };
                return Err(format!("vkGetSwapchainImagesKHR failed: {error:?}"));
            }
        };
        if images.is_empty() {
            unsafe { loader.destroy_swapchain(swapchain, None) };
            return Err("Vulkan swapchain contains no images".into());
        }

        let mut image_views = Vec::with_capacity(images.len());
        for image in images {
            let subresource_range = vk::ImageSubresourceRange::default()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .base_mip_level(0)
                .level_count(1)
                .base_array_layer(0)
                .layer_count(1);
            let view_info = vk::ImageViewCreateInfo::default()
                .image(image)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(format)
                .subresource_range(subresource_range);
            match unsafe { device.create_image_view(&view_info, None) } {
                Ok(view) => image_views.push(view),
                Err(error) => {
                    for view in image_views {
                        unsafe { device.destroy_image_view(view, None) };
                    }
                    unsafe { loader.destroy_swapchain(swapchain, None) };
                    return Err(format!("vkCreateImageView failed: {error:?}"));
                }
            }
        }

        let color_attachment = vk::AttachmentDescription::default()
            .format(format)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
            .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
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
        let render_pass = match unsafe { device.create_render_pass(&render_pass_info, None) } {
            Ok(render_pass) => render_pass,
            Err(error) => {
                for view in image_views {
                    unsafe { device.destroy_image_view(view, None) };
                }
                unsafe { loader.destroy_swapchain(swapchain, None) };
                return Err(format!("vkCreateRenderPass failed: {error:?}"));
            }
        };

        let mut framebuffers = Vec::with_capacity(image_views.len());
        for &view in &image_views {
            let framebuffer_attachments = [view];
            let framebuffer_info = vk::FramebufferCreateInfo::default()
                .render_pass(render_pass)
                .attachments(&framebuffer_attachments)
                .width(extent.width)
                .height(extent.height)
                .layers(1);
            match unsafe { device.create_framebuffer(&framebuffer_info, None) } {
                Ok(framebuffer) => framebuffers.push(framebuffer),
                Err(error) => {
                    for framebuffer in framebuffers {
                        unsafe { device.destroy_framebuffer(framebuffer, None) };
                    }
                    unsafe { device.destroy_render_pass(render_pass, None) };
                    for view in image_views {
                        unsafe { device.destroy_image_view(view, None) };
                    }
                    unsafe { loader.destroy_swapchain(swapchain, None) };
                    return Err(format!("vkCreateFramebuffer failed: {error:?}"));
                }
            }
        }

        let pool_info = vk::CommandPoolCreateInfo::default()
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
            .queue_family_index(queue_family_index);
        let command_pool = match unsafe { device.create_command_pool(&pool_info, None) } {
            Ok(pool) => pool,
            Err(error) => {
                for framebuffer in framebuffers {
                    unsafe { device.destroy_framebuffer(framebuffer, None) };
                }
                unsafe { device.destroy_render_pass(render_pass, None) };
                for view in image_views {
                    unsafe { device.destroy_image_view(view, None) };
                }
                unsafe { loader.destroy_swapchain(swapchain, None) };
                return Err(format!("vkCreateCommandPool failed: {error:?}"));
            }
        };

        let allocation_info = vk::CommandBufferAllocateInfo::default()
            .command_pool(command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(framebuffers.len() as u32);
        let command_buffers = match unsafe { device.allocate_command_buffers(&allocation_info) } {
            Ok(buffers) => buffers,
            Err(error) => {
                unsafe { device.destroy_command_pool(command_pool, None) };
                for framebuffer in framebuffers {
                    unsafe { device.destroy_framebuffer(framebuffer, None) };
                }
                unsafe { device.destroy_render_pass(render_pass, None) };
                for view in image_views {
                    unsafe { device.destroy_image_view(view, None) };
                }
                unsafe { loader.destroy_swapchain(swapchain, None) };
                return Err(format!("vkAllocateCommandBuffers failed: {error:?}"));
            }
        };

        for (&command_buffer, &framebuffer) in command_buffers.iter().zip(&framebuffers) {
            let begin_info = vk::CommandBufferBeginInfo::default();
            if let Err(error) = unsafe { device.begin_command_buffer(command_buffer, &begin_info) } {
                unsafe { device.destroy_command_pool(command_pool, None) };
                for framebuffer in framebuffers {
                    unsafe { device.destroy_framebuffer(framebuffer, None) };
                }
                unsafe { device.destroy_render_pass(render_pass, None) };
                for view in image_views {
                    unsafe { device.destroy_image_view(view, None) };
                }
                unsafe { loader.destroy_swapchain(swapchain, None) };
                return Err(format!("vkBeginCommandBuffer failed: {error:?}"));
            }
            let clear_values = [vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: [0.055, 0.055, 0.065, 1.0],
                },
            }];
            let render_area = vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent,
            };
            let render_pass_begin = vk::RenderPassBeginInfo::default()
                .render_pass(render_pass)
                .framebuffer(framebuffer)
                .render_area(render_area)
                .clear_values(&clear_values);
            unsafe {
                device.cmd_begin_render_pass(
                    command_buffer,
                    &render_pass_begin,
                    vk::SubpassContents::INLINE,
                );
                device.cmd_end_render_pass(command_buffer);
            }
            if let Err(error) = unsafe { device.end_command_buffer(command_buffer) } {
                unsafe { device.destroy_command_pool(command_pool, None) };
                for framebuffer in framebuffers {
                    unsafe { device.destroy_framebuffer(framebuffer, None) };
                }
                unsafe { device.destroy_render_pass(render_pass, None) };
                for view in image_views {
                    unsafe { device.destroy_image_view(view, None) };
                }
                unsafe { loader.destroy_swapchain(swapchain, None) };
                return Err(format!("vkEndCommandBuffer failed: {error:?}"));
            }
        }

        let semaphore_info = vk::SemaphoreCreateInfo::default();
        let image_available = match unsafe { device.create_semaphore(&semaphore_info, None) } {
            Ok(semaphore) => semaphore,
            Err(error) => {
                unsafe { device.destroy_command_pool(command_pool, None) };
                for framebuffer in framebuffers {
                    unsafe { device.destroy_framebuffer(framebuffer, None) };
                }
                unsafe { device.destroy_render_pass(render_pass, None) };
                for view in image_views {
                    unsafe { device.destroy_image_view(view, None) };
                }
                unsafe { loader.destroy_swapchain(swapchain, None) };
                return Err(format!("vkCreateSemaphore(image_available) failed: {error:?}"));
            }
        };
        let render_finished = match unsafe { device.create_semaphore(&semaphore_info, None) } {
            Ok(semaphore) => semaphore,
            Err(error) => {
                unsafe { device.destroy_semaphore(image_available, None) };
                unsafe { device.destroy_command_pool(command_pool, None) };
                for framebuffer in framebuffers {
                    unsafe { device.destroy_framebuffer(framebuffer, None) };
                }
                unsafe { device.destroy_render_pass(render_pass, None) };
                for view in image_views {
                    unsafe { device.destroy_image_view(view, None) };
                }
                unsafe { loader.destroy_swapchain(swapchain, None) };
                return Err(format!("vkCreateSemaphore(render_finished) failed: {error:?}"));
            }
        };
        let fence_info = vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);
        let in_flight = match unsafe { device.create_fence(&fence_info, None) } {
            Ok(fence) => fence,
            Err(error) => {
                unsafe { device.destroy_semaphore(render_finished, None) };
                unsafe { device.destroy_semaphore(image_available, None) };
                unsafe { device.destroy_command_pool(command_pool, None) };
                for framebuffer in framebuffers {
                    unsafe { device.destroy_framebuffer(framebuffer, None) };
                }
                unsafe { device.destroy_render_pass(render_pass, None) };
                for view in image_views {
                    unsafe { device.destroy_image_view(view, None) };
                }
                unsafe { loader.destroy_swapchain(swapchain, None) };
                return Err(format!("vkCreateFence failed: {error:?}"));
            }
        };

        self.swapchain = Some(SwapchainState {
            loader,
            swapchain,
            format,
            extent,
            image_views,
            render_pass,
            framebuffers,
            command_pool,
            command_buffers,
            image_available,
            render_finished,
            in_flight,
        });
        Ok(())
    }

    fn present_clear_frame(&mut self) -> Result<(), String> {
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
        let (image_index, _suboptimal) = unsafe {
            state.loader.acquire_next_image(
                state.swapchain,
                u64::MAX,
                state.image_available,
                vk::Fence::null(),
            )
        }
        .map_err(|e| format!("vkAcquireNextImageKHR failed: {e:?}"))?;
        let command_buffer = *state
            .command_buffers
            .get(image_index as usize)
            .ok_or_else(|| "swapchain returned an invalid image index".to_string())?;

        unsafe {
            device
                .reset_fences(&[state.in_flight])
                .map_err(|e| format!("vkResetFences failed: {e:?}"))?;
        }
        let wait_semaphores = [state.image_available];
        let wait_stages = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
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
        unsafe { state.loader.queue_present(queue, &present_info) }
            .map_err(|e| format!("vkQueuePresentKHR failed: {e:?}"))?;
        Ok(())
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
        if let Err(error) = self.create_swapchain(surface, target.width, target.height) {
            unsafe { self.surface_loader.destroy_surface(surface, None) };
            return Err(error);
        }
        if let Err(error) = self.present_clear_frame() {
            self.destroy_swapchain();
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
        // ANativeWindow ownership transfers from JNI to the renderer only after the
        // entire Vulkan surface + device + swapchain + first-present path succeeds.
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
            self.create_swapchain(surface, width, height)?;
            self.present_clear_frame()?;
        }
        Ok(())
    }

    fn ingest_input(&mut self, samples: &[StrokeSample]) -> Result<(), String> {
        if let Some(last) = samples.last() {
            self.last_input_time_ns = last.time_ns;
        }
        Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_count_respects_surface_maximum() {
        let capabilities = vk::SurfaceCapabilitiesKHR {
            min_image_count: 2,
            max_image_count: 2,
            ..Default::default()
        };
        assert_eq!(AndroidRenderer::choose_image_count(capabilities), 2);
    }

    #[test]
    fn image_count_uses_one_more_than_minimum_when_unbounded() {
        let capabilities = vk::SurfaceCapabilitiesKHR {
            min_image_count: 2,
            max_image_count: 0,
            ..Default::default()
        };
        assert_eq!(AndroidRenderer::choose_image_count(capabilities), 3);
    }

    #[test]
    fn variable_extent_is_clamped_to_surface_limits() {
        let capabilities = vk::SurfaceCapabilitiesKHR {
            current_extent: vk::Extent2D {
                width: u32::MAX,
                height: u32::MAX,
            },
            min_image_extent: vk::Extent2D {
                width: 320,
                height: 240,
            },
            max_image_extent: vk::Extent2D {
                width: 1920,
                height: 1080,
            },
            ..Default::default()
        };
        assert_eq!(
            AndroidRenderer::choose_extent(capabilities, 4000, 100),
            vk::Extent2D {
                width: 1920,
                height: 240,
            }
        );
    }
}
