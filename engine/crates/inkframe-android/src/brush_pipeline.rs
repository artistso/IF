use std::mem::{offset_of, size_of};
use std::ptr;

use ash::{Device, Instance, vk};

use crate::brush_geometry::BrushInstance;

pub(crate) const MAX_BRUSH_INSTANCES: usize = 8192;

pub(crate) struct BrushPipeline {
    layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
    instance_buffer: vk::Buffer,
    instance_memory: vk::DeviceMemory,
}

impl BrushPipeline {
    pub(crate) fn new(
        instance: &Instance,
        device: &Device,
        physical_device: vk::PhysicalDevice,
        render_pass: vk::RenderPass,
        extent: vk::Extent2D,
    ) -> Result<Self, String> {
        let (instance_buffer, instance_memory) = create_instance_buffer(
            instance,
            device,
            physical_device,
            (MAX_BRUSH_INSTANCES * size_of::<BrushInstance>()) as vk::DeviceSize,
        )?;

        let layout_info = vk::PipelineLayoutCreateInfo::default();
        let layout = match unsafe { device.create_pipeline_layout(&layout_info, None) } {
            Ok(layout) => layout,
            Err(error) => {
                unsafe {
                    device.destroy_buffer(instance_buffer, None);
                    device.free_memory(instance_memory, None);
                }
                return Err(format!("vkCreatePipelineLayout(brush) failed: {error:?}"));
            }
        };

        let vertex_words = spirv_words(include_bytes!(concat!(env!("OUT_DIR"), "/brush.vert.spv")))?;
        let fragment_words =
            spirv_words(include_bytes!(concat!(env!("OUT_DIR"), "/brush.frag.spv")))?;
        let vertex_info = vk::ShaderModuleCreateInfo::default().code(&vertex_words);
        let fragment_info = vk::ShaderModuleCreateInfo::default().code(&fragment_words);
        let vertex_module = match unsafe { device.create_shader_module(&vertex_info, None) } {
            Ok(module) => module,
            Err(error) => {
                unsafe {
                    device.destroy_pipeline_layout(layout, None);
                    device.destroy_buffer(instance_buffer, None);
                    device.free_memory(instance_memory, None);
                }
                return Err(format!("vkCreateShaderModule(brush vertex) failed: {error:?}"));
            }
        };
        let fragment_module = match unsafe { device.create_shader_module(&fragment_info, None) } {
            Ok(module) => module,
            Err(error) => {
                unsafe {
                    device.destroy_shader_module(vertex_module, None);
                    device.destroy_pipeline_layout(layout, None);
                    device.destroy_buffer(instance_buffer, None);
                    device.free_memory(instance_memory, None);
                }
                return Err(format!(
                    "vkCreateShaderModule(brush fragment) failed: {error:?}"
                ));
            }
        };

        let pipeline_result = (|| -> Result<vk::Pipeline, String> {
            let stages = [
                vk::PipelineShaderStageCreateInfo::default()
                    .stage(vk::ShaderStageFlags::VERTEX)
                    .module(vertex_module)
                    .name(c"vs_main"),
                vk::PipelineShaderStageCreateInfo::default()
                    .stage(vk::ShaderStageFlags::FRAGMENT)
                    .module(fragment_module)
                    .name(c"fs_main"),
            ];
            let bindings = [vk::VertexInputBindingDescription::default()
                .binding(0)
                .stride(size_of::<BrushInstance>() as u32)
                .input_rate(vk::VertexInputRate::INSTANCE)];
            let attributes = [
                vk::VertexInputAttributeDescription::default()
                    .location(0)
                    .binding(0)
                    .format(vk::Format::R32G32_SFLOAT)
                    .offset(offset_of!(BrushInstance, center) as u32),
                vk::VertexInputAttributeDescription::default()
                    .location(1)
                    .binding(0)
                    .format(vk::Format::R32G32_SFLOAT)
                    .offset(offset_of!(BrushInstance, half_size) as u32),
                vk::VertexInputAttributeDescription::default()
                    .location(2)
                    .binding(0)
                    .format(vk::Format::R32G32B32A32_SFLOAT)
                    .offset(offset_of!(BrushInstance, color) as u32),
                vk::VertexInputAttributeDescription::default()
                    .location(3)
                    .binding(0)
                    .format(vk::Format::R32_SFLOAT)
                    .offset(offset_of!(BrushInstance, edge) as u32),
            ];
            let vertex_input = vk::PipelineVertexInputStateCreateInfo::default()
                .vertex_binding_descriptions(&bindings)
                .vertex_attribute_descriptions(&attributes);
            let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
                .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
                .primitive_restart_enable(false);
            let viewports = [vk::Viewport {
                x: 0.0,
                y: 0.0,
                width: extent.width as f32,
                height: extent.height as f32,
                min_depth: 0.0,
                max_depth: 1.0,
            }];
            let scissors = [vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent,
            }];
            let viewport_state = vk::PipelineViewportStateCreateInfo::default()
                .viewports(&viewports)
                .scissors(&scissors);
            let rasterization = vk::PipelineRasterizationStateCreateInfo::default()
                .depth_clamp_enable(false)
                .rasterizer_discard_enable(false)
                .polygon_mode(vk::PolygonMode::FILL)
                .cull_mode(vk::CullModeFlags::NONE)
                .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
                .line_width(1.0);
            let multisample = vk::PipelineMultisampleStateCreateInfo::default()
                .rasterization_samples(vk::SampleCountFlags::TYPE_1);
            let color_write_mask = vk::ColorComponentFlags::R
                | vk::ColorComponentFlags::G
                | vk::ColorComponentFlags::B
                | vk::ColorComponentFlags::A;
            let blend_attachments = [vk::PipelineColorBlendAttachmentState::default()
                .blend_enable(true)
                .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
                .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
                .color_blend_op(vk::BlendOp::ADD)
                .src_alpha_blend_factor(vk::BlendFactor::ONE)
                .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
                .alpha_blend_op(vk::BlendOp::ADD)
                .color_write_mask(color_write_mask)];
            let color_blend =
                vk::PipelineColorBlendStateCreateInfo::default().attachments(&blend_attachments);
            let create_info = vk::GraphicsPipelineCreateInfo::default()
                .stages(&stages)
                .vertex_input_state(&vertex_input)
                .input_assembly_state(&input_assembly)
                .viewport_state(&viewport_state)
                .rasterization_state(&rasterization)
                .multisample_state(&multisample)
                .color_blend_state(&color_blend)
                .layout(layout)
                .render_pass(render_pass)
                .subpass(0);

            match unsafe {
                device.create_graphics_pipelines(vk::PipelineCache::null(), &[create_info], None)
            } {
                Ok(mut pipelines) => pipelines
                    .pop()
                    .ok_or_else(|| "Vulkan returned no brush graphics pipeline".to_string()),
                Err((_pipelines, error)) => {
                    Err(format!("vkCreateGraphicsPipelines(brush) failed: {error:?}"))
                }
            }
        })();

        unsafe {
            device.destroy_shader_module(fragment_module, None);
            device.destroy_shader_module(vertex_module, None);
        }

        match pipeline_result {
            Ok(pipeline) => Ok(Self {
                layout,
                pipeline,
                instance_buffer,
                instance_memory,
            }),
            Err(error) => {
                unsafe {
                    device.destroy_pipeline_layout(layout, None);
                    device.destroy_buffer(instance_buffer, None);
                    device.free_memory(instance_memory, None);
                }
                Err(error)
            }
        }
    }

    pub(crate) fn upload_instances(
        &self,
        device: &Device,
        instances: &[BrushInstance],
    ) -> Result<u32, String> {
        if instances.len() > MAX_BRUSH_INSTANCES {
            return Err(format!(
                "brush instance batch {} exceeds capacity {}",
                instances.len(),
                MAX_BRUSH_INSTANCES
            ));
        }
        if instances.is_empty() {
            return Ok(0);
        }
        let byte_len = std::mem::size_of_val(instances) as vk::DeviceSize;
        let mapped = unsafe {
            device.map_memory(
                self.instance_memory,
                0,
                byte_len,
                vk::MemoryMapFlags::empty(),
            )
        }
        .map_err(|error| format!("vkMapMemory(brush instances) failed: {error:?}"))?;
        unsafe {
            ptr::copy_nonoverlapping(
                instances.as_ptr().cast::<u8>(),
                mapped.cast::<u8>(),
                byte_len as usize,
            );
            device.unmap_memory(self.instance_memory);
        }
        Ok(instances.len() as u32)
    }

    pub(crate) unsafe fn record(
        &self,
        device: &Device,
        command_buffer: vk::CommandBuffer,
        instance_count: u32,
    ) {
        if instance_count == 0 {
            return;
        }
        unsafe {
            device.cmd_bind_pipeline(command_buffer, vk::PipelineBindPoint::GRAPHICS, self.pipeline);
            device.cmd_bind_vertex_buffers(command_buffer, 0, &[self.instance_buffer], &[0]);
            device.cmd_draw(command_buffer, 6, instance_count, 0, 0);
        }
    }

    pub(crate) fn destroy(&self, device: &Device) {
        unsafe {
            device.destroy_pipeline(self.pipeline, None);
            device.destroy_pipeline_layout(self.layout, None);
            device.destroy_buffer(self.instance_buffer, None);
            device.free_memory(self.instance_memory, None);
        }
    }
}

fn spirv_words(bytes: &[u8]) -> Result<Vec<u32>, String> {
    if bytes.len() % 4 != 0 {
        return Err("generated brush SPIR-V byte length is not word aligned".into());
    }
    Ok(bytes
        .chunks_exact(4)
        .map(|chunk| u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect())
}

fn create_instance_buffer(
    instance: &Instance,
    device: &Device,
    physical_device: vk::PhysicalDevice,
    size: vk::DeviceSize,
) -> Result<(vk::Buffer, vk::DeviceMemory), String> {
    let buffer_info = vk::BufferCreateInfo::default()
        .size(size)
        .usage(vk::BufferUsageFlags::VERTEX_BUFFER)
        .sharing_mode(vk::SharingMode::EXCLUSIVE);
    let buffer = unsafe { device.create_buffer(&buffer_info, None) }
        .map_err(|error| format!("vkCreateBuffer(brush instances) failed: {error:?}"))?;
    let requirements = unsafe { device.get_buffer_memory_requirements(buffer) };
    let memory_type = match find_memory_type(
        instance,
        physical_device,
        requirements.memory_type_bits,
        vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
    ) {
        Ok(index) => index,
        Err(error) => {
            unsafe { device.destroy_buffer(buffer, None) };
            return Err(error);
        }
    };
    let allocation = vk::MemoryAllocateInfo::default()
        .allocation_size(requirements.size)
        .memory_type_index(memory_type);
    let memory = match unsafe { device.allocate_memory(&allocation, None) } {
        Ok(memory) => memory,
        Err(error) => {
            unsafe { device.destroy_buffer(buffer, None) };
            return Err(format!(
                "vkAllocateMemory(brush instances) failed: {error:?}"
            ));
        }
    };
    if let Err(error) = unsafe { device.bind_buffer_memory(buffer, memory, 0) } {
        unsafe {
            device.free_memory(memory, None);
            device.destroy_buffer(buffer, None);
        }
        return Err(format!(
            "vkBindBufferMemory(brush instances) failed: {error:?}"
        ));
    }
    Ok((buffer, memory))
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
