use std::ptr;

use ash::{Device, Instance, vk};
use inkframe_raster::{DirtyRect, TILE_BYTES, TILE_SIZE, TileCoord};

use crate::viewport_pixels::{PixelOrder, clip_tile_region, encode_opaque_tile};

const COLOR_RANGE: vk::ImageSubresourceRange = vk::ImageSubresourceRange {
    aspect_mask: vk::ImageAspectFlags::COLOR,
    base_mip_level: 0,
    level_count: 1,
    base_array_layer: 0,
    layer_count: 1,
};

pub(crate) struct ViewportCache {
    image: vk::Image,
    memory: vk::DeviceMemory,
    extent: vk::Extent2D,
    pixel_order: PixelOrder,
}

impl ViewportCache {
    pub(crate) fn pixel_order_for_format(format: vk::Format) -> Option<PixelOrder> {
        match format {
            vk::Format::R8G8B8A8_UNORM | vk::Format::R8G8B8A8_SRGB => Some(PixelOrder::Rgba),
            vk::Format::B8G8R8A8_UNORM | vk::Format::B8G8R8A8_SRGB => Some(PixelOrder::Bgra),
            _ => None,
        }
    }

    pub(crate) fn new(
        instance: &Instance,
        device: &Device,
        physical_device: vk::PhysicalDevice,
        queue: vk::Queue,
        command_pool: vk::CommandPool,
        extent: vk::Extent2D,
        format: vk::Format,
        background: [f32; 4],
    ) -> Result<Self, String> {
        let pixel_order = Self::pixel_order_for_format(format)
            .ok_or_else(|| format!("unsupported persistent viewport format: {format:?}"))?;
        let image_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(format)
            .extent(vk::Extent3D {
                width: extent.width,
                height: extent.height,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::TRANSFER_SRC)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);
        let image = unsafe { device.create_image(&image_info, None) }
            .map_err(|e| format!("vkCreateImage(viewport cache) failed: {e:?}"))?;
        let requirements = unsafe { device.get_image_memory_requirements(image) };
        let memory_type = find_memory_type(
            instance,
            physical_device,
            requirements.memory_type_bits,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
        )?;
        let allocation = vk::MemoryAllocateInfo::default()
            .allocation_size(requirements.size)
            .memory_type_index(memory_type);
        let memory = match unsafe { device.allocate_memory(&allocation, None) } {
            Ok(memory) => memory,
            Err(error) => {
                unsafe { device.destroy_image(image, None) };
                return Err(format!(
                    "vkAllocateMemory(viewport cache) failed: {error:?}"
                ));
            }
        };
        if let Err(error) = unsafe { device.bind_image_memory(image, memory, 0) } {
            unsafe {
                device.free_memory(memory, None);
                device.destroy_image(image, None);
            }
            return Err(format!(
                "vkBindImageMemory(viewport cache) failed: {error:?}"
            ));
        }

        let cache = Self {
            image,
            memory,
            extent,
            pixel_order,
        };
        let init_result = submit_immediate(device, queue, command_pool, |command_buffer| unsafe {
            transition_image(
                device,
                command_buffer,
                cache.image,
                vk::ImageLayout::UNDEFINED,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::AccessFlags::empty(),
                vk::AccessFlags::TRANSFER_WRITE,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                vk::PipelineStageFlags::TRANSFER,
            );
            device.cmd_clear_color_image(
                command_buffer,
                cache.image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &vk::ClearColorValue {
                    float32: background,
                },
                &[COLOR_RANGE],
            );
            transition_image(
                device,
                command_buffer,
                cache.image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                vk::AccessFlags::TRANSFER_WRITE,
                vk::AccessFlags::TRANSFER_READ,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::TRANSFER,
            );
        });
        if let Err(error) = init_result {
            cache.destroy(device);
            return Err(error);
        }
        Ok(cache)
    }

    pub(crate) fn upload_tiles<'a, I>(
        &self,
        instance: &Instance,
        device: &Device,
        physical_device: vk::PhysicalDevice,
        queue: vk::Queue,
        command_pool: vk::CommandPool,
        background_rgb: [u8; 3],
        tiles: I,
    ) -> Result<(), String>
    where
        I: IntoIterator<Item = (TileCoord, DirtyRect, &'a [u8])>,
    {
        let mut staging_bytes = Vec::new();
        let mut copies = Vec::new();

        for (coord, dirty, pixels) in tiles {
            let Some(region) =
                clip_tile_region(coord, dirty, self.extent.width, self.extent.height)
            else {
                continue;
            };
            let base_offset = staging_bytes.len() as vk::DeviceSize;
            let encoded = encode_opaque_tile(pixels, self.pixel_order, background_rgb)
                .map_err(str::to_string)?;
            staging_bytes.extend_from_slice(&encoded);

            let local_byte_offset = ((region.local_y as usize * TILE_SIZE as usize
                + region.local_x as usize)
                * 4) as vk::DeviceSize;
            let copy = vk::BufferImageCopy::default()
                .buffer_offset(base_offset + local_byte_offset)
                .buffer_row_length(TILE_SIZE)
                .buffer_image_height(TILE_SIZE)
                .image_subresource(
                    vk::ImageSubresourceLayers::default()
                        .aspect_mask(vk::ImageAspectFlags::COLOR)
                        .mip_level(0)
                        .base_array_layer(0)
                        .layer_count(1),
                )
                .image_offset(vk::Offset3D {
                    x: region.image_x as i32,
                    y: region.image_y as i32,
                    z: 0,
                })
                .image_extent(vk::Extent3D {
                    width: region.width as u32,
                    height: region.height as u32,
                    depth: 1,
                });
            copies.push(copy);
        }

        if copies.is_empty() {
            return Ok(());
        }
        debug_assert_eq!(staging_bytes.len() % TILE_BYTES, 0);

        let (buffer, memory) = create_staging_buffer(
            instance,
            device,
            physical_device,
            staging_bytes.len() as vk::DeviceSize,
        )?;
        let write_result = (|| -> Result<(), String> {
            let mapped = unsafe {
                device.map_memory(
                    memory,
                    0,
                    staging_bytes.len() as vk::DeviceSize,
                    vk::MemoryMapFlags::empty(),
                )
            }
            .map_err(|e| format!("vkMapMemory(viewport staging) failed: {e:?}"))?;
            unsafe {
                ptr::copy_nonoverlapping(
                    staging_bytes.as_ptr(),
                    mapped.cast::<u8>(),
                    staging_bytes.len(),
                );
                device.unmap_memory(memory);
            }

            submit_immediate(device, queue, command_pool, |command_buffer| unsafe {
                transition_image(
                    device,
                    command_buffer,
                    self.image,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    vk::AccessFlags::TRANSFER_READ,
                    vk::AccessFlags::TRANSFER_WRITE,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::PipelineStageFlags::TRANSFER,
                );
                device.cmd_copy_buffer_to_image(
                    command_buffer,
                    buffer,
                    self.image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &copies,
                );
                transition_image(
                    device,
                    command_buffer,
                    self.image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    vk::AccessFlags::TRANSFER_WRITE,
                    vk::AccessFlags::TRANSFER_READ,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::PipelineStageFlags::TRANSFER,
                );
            })
        })();
        unsafe {
            device.destroy_buffer(buffer, None);
            device.free_memory(memory, None);
        }
        write_result
    }

    pub(crate) unsafe fn record_copy_to_swapchain(
        &self,
        device: &Device,
        command_buffer: vk::CommandBuffer,
        swapchain_image: vk::Image,
    ) {
        unsafe {
            transition_image(
                device,
                command_buffer,
                swapchain_image,
                vk::ImageLayout::UNDEFINED,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::AccessFlags::empty(),
                vk::AccessFlags::TRANSFER_WRITE,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                vk::PipelineStageFlags::TRANSFER,
            );
            let copy = vk::ImageCopy::default()
                .src_subresource(
                    vk::ImageSubresourceLayers::default()
                        .aspect_mask(vk::ImageAspectFlags::COLOR)
                        .mip_level(0)
                        .base_array_layer(0)
                        .layer_count(1),
                )
                .dst_subresource(
                    vk::ImageSubresourceLayers::default()
                        .aspect_mask(vk::ImageAspectFlags::COLOR)
                        .mip_level(0)
                        .base_array_layer(0)
                        .layer_count(1),
                )
                .extent(vk::Extent3D {
                    width: self.extent.width,
                    height: self.extent.height,
                    depth: 1,
                });
            device.cmd_copy_image(
                command_buffer,
                self.image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                swapchain_image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[copy],
            );
            transition_image(
                device,
                command_buffer,
                swapchain_image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                vk::AccessFlags::TRANSFER_WRITE,
                vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            );
        }
    }

    pub(crate) fn destroy(&self, device: &Device) {
        unsafe {
            device.destroy_image(self.image, None);
            device.free_memory(self.memory, None);
        }
    }
}

pub(crate) unsafe fn transition_swapchain_for_legacy_render(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    image: vk::Image,
) {
    unsafe {
        transition_image(
            device,
            command_buffer,
            image,
            vk::ImageLayout::UNDEFINED,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            vk::AccessFlags::empty(),
            vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
            vk::PipelineStageFlags::TOP_OF_PIPE,
            vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
        );
    }
}

fn find_memory_type(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    type_bits: u32,
    required: vk::MemoryPropertyFlags,
) -> Result<u32, String> {
    let properties = unsafe { instance.get_physical_device_memory_properties(physical_device) };
    for index in 0..properties.memory_type_count {
        let supported = type_bits & (1 << index) != 0;
        let flags = properties.memory_types[index as usize].property_flags;
        if supported && flags.contains(required) {
            return Ok(index);
        }
    }
    Err(format!("no Vulkan memory type satisfies {required:?}"))
}

fn create_staging_buffer(
    instance: &Instance,
    device: &Device,
    physical_device: vk::PhysicalDevice,
    size: vk::DeviceSize,
) -> Result<(vk::Buffer, vk::DeviceMemory), String> {
    let buffer_info = vk::BufferCreateInfo::default()
        .size(size)
        .usage(vk::BufferUsageFlags::TRANSFER_SRC)
        .sharing_mode(vk::SharingMode::EXCLUSIVE);
    let buffer = unsafe { device.create_buffer(&buffer_info, None) }
        .map_err(|e| format!("vkCreateBuffer(viewport staging) failed: {e:?}"))?;
    let requirements = unsafe { device.get_buffer_memory_requirements(buffer) };
    let memory_type = find_memory_type(
        instance,
        physical_device,
        requirements.memory_type_bits,
        vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
    )?;
    let allocation = vk::MemoryAllocateInfo::default()
        .allocation_size(requirements.size)
        .memory_type_index(memory_type);
    let memory = match unsafe { device.allocate_memory(&allocation, None) } {
        Ok(memory) => memory,
        Err(error) => {
            unsafe { device.destroy_buffer(buffer, None) };
            return Err(format!(
                "vkAllocateMemory(viewport staging) failed: {error:?}"
            ));
        }
    };
    if let Err(error) = unsafe { device.bind_buffer_memory(buffer, memory, 0) } {
        unsafe {
            device.free_memory(memory, None);
            device.destroy_buffer(buffer, None);
        }
        return Err(format!(
            "vkBindBufferMemory(viewport staging) failed: {error:?}"
        ));
    }
    Ok((buffer, memory))
}

fn submit_immediate<F>(
    device: &Device,
    queue: vk::Queue,
    command_pool: vk::CommandPool,
    record: F,
) -> Result<(), String>
where
    F: FnOnce(vk::CommandBuffer),
{
    let allocation = vk::CommandBufferAllocateInfo::default()
        .command_pool(command_pool)
        .level(vk::CommandBufferLevel::PRIMARY)
        .command_buffer_count(1);
    let command_buffer = unsafe { device.allocate_command_buffers(&allocation) }
        .map_err(|e| format!("vkAllocateCommandBuffers(immediate) failed: {e:?}"))?[0];
    let begin =
        vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
    unsafe {
        device
            .begin_command_buffer(command_buffer, &begin)
            .map_err(|e| format!("vkBeginCommandBuffer(immediate) failed: {e:?}"))?;
    }
    record(command_buffer);
    unsafe {
        device
            .end_command_buffer(command_buffer)
            .map_err(|e| format!("vkEndCommandBuffer(immediate) failed: {e:?}"))?;
        let command_buffers = [command_buffer];
        let submit = vk::SubmitInfo::default().command_buffers(&command_buffers);
        device
            .queue_submit(queue, &[submit], vk::Fence::null())
            .map_err(|e| format!("vkQueueSubmit(immediate) failed: {e:?}"))?;
        device
            .queue_wait_idle(queue)
            .map_err(|e| format!("vkQueueWaitIdle(immediate) failed: {e:?}"))?;
        device.free_command_buffers(command_pool, &[command_buffer]);
    }
    Ok(())
}

unsafe fn transition_image(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    image: vk::Image,
    old_layout: vk::ImageLayout,
    new_layout: vk::ImageLayout,
    src_access: vk::AccessFlags,
    dst_access: vk::AccessFlags,
    src_stage: vk::PipelineStageFlags,
    dst_stage: vk::PipelineStageFlags,
) {
    let barrier = vk::ImageMemoryBarrier::default()
        .src_access_mask(src_access)
        .dst_access_mask(dst_access)
        .old_layout(old_layout)
        .new_layout(new_layout)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .image(image)
        .subresource_range(COLOR_RANGE);
    unsafe {
        device.cmd_pipeline_barrier(
            command_buffer,
            src_stage,
            dst_stage,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[barrier],
        );
    }
}
