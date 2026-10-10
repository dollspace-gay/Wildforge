//! Dedicated GPU fixtures, dispatch and completed readback for lighting tests.

use super::{Case, Input, Output, Uniforms, shader};
use wgpu::util::DeviceExt;

fn buffer_entry(binding: u32, ty: wgpu::BufferBindingType) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn texture_entry(
    binding: u32,
    sample_type: wgpu::TextureSampleType,
    view_dimension: wgpu::TextureViewDimension,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Texture {
            sample_type,
            view_dimension,
            multisampled: false,
        },
        count: None,
    }
}

fn texture(
    device: &wgpu::Device,
    dimension: wgpu::TextureDimension,
    size: wgpu::Extent3d,
    format: wgpu::TextureFormat,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("shadow-guard-fixture"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING
            | if format == wgpu::TextureFormat::Depth32Float {
                wgpu::TextureUsages::RENDER_ATTACHMENT
            } else {
                wgpu::TextureUsages::COPY_DST
            },
        view_formats: &[],
    })
}

pub(super) struct HardwareProbe {
    device: wgpu::Device,
    queue: wgpu::Queue,
    layouts: [wgpu::BindGroupLayout; 4],
    empty: wgpu::BindGroup,
    shadows: wgpu::BindGroup,
    pipelines: [wgpu::ComputePipeline; 2],
    inputs: wgpu::Buffer,
    count: u32,
    _textures: [wgpu::Texture; 4],
}

impl HardwareProbe {
    pub(super) fn new(cases: &[Case]) -> Self {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            ..Default::default()
        });
        let adapter = instance
            .enumerate_adapters(wgpu::Backends::VULKAN)
            .into_iter()
            .find(|a| {
                let info = a.get_info();
                info.backend == wgpu::Backend::Vulkan
                    && info.device_type == wgpu::DeviceType::DiscreteGpu
            })
            .expect("hardware gate requires a Vulkan DiscreteGpu; CPU fallback is forbidden");
        eprintln!("shadow guard hardware: {:?}", adapter.get_info());
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("shadow-guard-hardware"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_defaults(),
            memory_hints: Default::default(),
            trace: wgpu::Trace::Off,
        }))
        .expect("create hardware device");
        let layout = |entries: &[wgpu::BindGroupLayoutEntry]| {
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: None,
                entries,
            })
        };
        let nonfilter = wgpu::TextureSampleType::Float { filterable: false };
        let layouts = [
            layout(&[buffer_entry(0, wgpu::BufferBindingType::Uniform)]),
            layout(&[]),
            layout(&[
                texture_entry(
                    0,
                    wgpu::TextureSampleType::Depth,
                    wgpu::TextureViewDimension::D2Array,
                ),
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
                texture_entry(2, nonfilter, wgpu::TextureViewDimension::CubeArray),
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                    count: None,
                },
                texture_entry(4, nonfilter, wgpu::TextureViewDimension::CubeArray),
                texture_entry(
                    5,
                    wgpu::TextureSampleType::Uint,
                    wgpu::TextureViewDimension::D3,
                ),
            ]),
            layout(&[
                buffer_entry(0, wgpu::BufferBindingType::Storage { read_only: true }),
                buffer_entry(1, wgpu::BufferBindingType::Storage { read_only: false }),
            ]),
        ];
        let empty = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &layouts[1],
            entries: &[],
        });
        let size = |width, height, depth_or_array_layers| wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers,
        };
        let depth = texture(
            &device,
            wgpu::TextureDimension::D2,
            size(2, 2, 3),
            wgpu::TextureFormat::Depth32Float,
        );
        let cube = texture(
            &device,
            wgpu::TextureDimension::D2,
            size(1, 1, 12),
            wgpu::TextureFormat::R32Float,
        );
        let tint = texture(
            &device,
            wgpu::TextureDimension::D2,
            size(1, 1, 12),
            wgpu::TextureFormat::Rgba16Float,
        );
        let occupancy = texture(
            &device,
            wgpu::TextureDimension::D3,
            size(16, 16, 16),
            wgpu::TextureFormat::Rgba8Uint,
        );
        let write = |tex: &wgpu::Texture, bytes: &[u8], row, rows| {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: tex,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                bytes,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(rows),
                },
                tex.size(),
            )
        };
        // Light zero is visible through stained transmission; light one is blocked.
        let distances: Vec<f32> = (0..12)
            .map(|face| if face < 6 { 12.0 } else { 6.0 })
            .collect();
        write(&cube, bytemuck::cast_slice(&distances), 4, 1);
        // Exact IEEE binary16 encodings: (1/4, 1/2, 3/4, 1) or white, far alpha.
        let filters: Vec<u16> = (0..12)
            .flat_map(|face| {
                if face < 6 {
                    [0x3400, 0x3800, 0x3a00, 0x3c00]
                } else {
                    [0x3c00, 0x3c00, 0x3c00, 0x7b53]
                }
            })
            .collect();
        write(&tint, bytemuck::cast_slice(&filters), 8, 1);
        let mut cells = vec![0u8; 16 * 16 * 16 * 4];
        let mut set = |x: usize, y: usize, z: usize, rgba: [u8; 4]| {
            let i = ((z * 16 + y) * 16 + x) * 4;
            cells[i..i + 4].copy_from_slice(&rgba);
        };
        set(4, 6, 4, [64, 128, 192, 128]);
        set(4, 10, 4, [0, 0, 0, 255]);
        write(&occupancy, &cells, 64, 16);
        let view = |tex: &wgpu::Texture, dimension| {
            tex.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(dimension),
                ..Default::default()
            })
        };
        let depth_view = view(&depth, wgpu::TextureViewDimension::D2Array);
        let cube_view = view(&cube, wgpu::TextureViewDimension::CubeArray);
        let tint_view = view(&tint, wgpu::TextureViewDimension::CubeArray);
        let occ_view = view(&occupancy, wgpu::TextureViewDimension::D3);
        let compare = device.create_sampler(&wgpu::SamplerDescriptor {
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        let nearest = device.create_sampler(&wgpu::SamplerDescriptor::default());
        let shadows = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &layouts[2],
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&depth_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&compare),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&cube_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&nearest),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&tint_view),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(&occ_view),
                },
            ],
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        for layer in 0..3 {
            let layer_view = depth.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2),
                base_array_layer: layer,
                array_layer_count: Some(1),
                ..Default::default()
            });
            drop(encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &layer_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            }));
        }
        queue.submit([encoder.finish()]);
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&layouts[0], &layouts[1], &layouts[2], &layouts[3]],
            push_constant_ranges: &[],
        });
        let pipelines = [false, true].map(|legacy| {
            let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(if legacy {
                    "legacy-world-light"
                } else {
                    "production-world-light"
                }),
                source: wgpu::ShaderSource::Wgsl(shader(legacy).into()),
            });
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: None,
                layout: Some(&pipeline_layout),
                module: &module,
                entry_point: Some("probe_main"),
                compilation_options: Default::default(),
                cache: None,
            })
        });
        let inputs: Vec<Input> = cases.iter().map(|case| case.input).collect();
        let inputs = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&inputs),
            usage: wgpu::BufferUsages::STORAGE,
        });
        Self {
            device,
            queue,
            layouts,
            empty,
            shadows,
            pipelines,
            inputs,
            count: cases.len().try_into().expect("bounded cases"),
            _textures: [depth, cube, tint, occupancy],
        }
    }

    pub(super) fn run(&self, uniforms: &Uniforms, variant: usize) -> Vec<Output> {
        let bytes = u64::from(self.count) * std::mem::size_of::<Output>() as u64;
        let uniform = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: bytemuck::bytes_of(uniforms),
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let uniform_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.layouts[0],
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            }],
        });
        let result = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: bytes,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: bytes,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let probes = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.layouts[3],
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.inputs.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: result.as_entire_binding(),
                },
            ],
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor::default());
            pass.set_pipeline(&self.pipelines[variant]);
            for (index, group) in [&uniform_group, &self.empty, &self.shadows, &probes]
                .into_iter()
                .enumerate()
            {
                pass.set_bind_group(index as u32, group, &[]);
            }
            pass.dispatch_workgroups(self.count, 1, 1);
        }
        encoder.copy_buffer_to_buffer(&result, 0, &readback, 0, bytes);
        let submitted = self.queue.submit([encoder.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |status| {
                tx.send(status).expect("readback receiver is live")
            });
        self.device
            .poll(wgpu::PollType::WaitForSubmissionIndex(submitted))
            .expect("hardware completion");
        rx.recv_timeout(std::time::Duration::from_secs(5))
            .expect("GPU callback")
            .expect("GPU readback");
        let mapped = readback.slice(..).get_mapped_range();
        let outputs = bytemuck::cast_slice::<u8, Output>(&mapped).to_vec();
        drop(mapped);
        readback.unmap();
        outputs
    }
}
