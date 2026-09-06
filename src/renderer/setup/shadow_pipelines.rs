//! Shadow pipelines GPU resource construction.

use crate::mesher::Vertex;

pub(super) struct ShadowPipelines {
    pub(super) shadow_pipeline: wgpu::RenderPipeline,
    pub(super) pt_shadow_pipeline: wgpu::RenderPipeline,
    pub(super) pt_tr_pipeline: wgpu::RenderPipeline,
}

pub(super) fn create(device: &wgpu::Device, shadow_layout: &wgpu::PipelineLayout, csm_shader: &wgpu::ShaderModule, pt_shadow_shader: &wgpu::ShaderModule, pt_face_bgl: &wgpu::BindGroupLayout, atlas_bgl: &wgpu::BindGroupLayout) -> ShadowPipelines {
        // Depth-only sun pass: reads only position from the chunk vertex buffer,
        // writes the shadow depth texture. Constant + slope depth bias pushes
        // occluders back to keep shadow acne off lit faces.
        let shadow_vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x3],
        };
        let shadow_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("shadow"),
            layout: Some(shadow_layout),
            vertex: wgpu::VertexState {
                module: csm_shader,
                entry_point: Some("vs_shadow"),
                compilation_options: Default::default(),
                buffers: std::slice::from_ref(&shadow_vertex_layout),
            },
            fragment: None,
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState {
                    constant: 2,
                    slope_scale: 2.5,
                    clamp: 0.0,
                },
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // Point-light cube-face pass: writes distance-to-light into an R32Float
        // face. No culling (robust for 1-block-thin occluders); a depth bias in
        // the compare keeps acne off lit faces.
        let pt_shadow_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pt-shadow-layout"),
            bind_group_layouts: &[pt_face_bgl],
            push_constant_ranges: &[],
        });
        let pt_shadow_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("pt-shadow"),
            layout: Some(&pt_shadow_layout),
            vertex: wgpu::VertexState {
                module: pt_shadow_shader,
                entry_point: Some("vs_pt_shadow"),
                compilation_options: Default::default(),
                buffers: std::slice::from_ref(&shadow_vertex_layout),
            },
            fragment: Some(wgpu::FragmentState {
                module: pt_shadow_shader,
                entry_point: Some("fs_pt_shadow"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::R32Float,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // Glass transmission into the tint cube: multiplicative color
        // (commutative — no sorting), Min alpha keeps the nearest pane's
        // distance. Tests against the distance pass's depth, read-only,
        // so glass behind an opaque wall never tints.
        let pt_tr_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pt-tr-layout"),
            bind_group_layouts: &[pt_face_bgl, atlas_bgl],
            push_constant_ranges: &[],
        });
        let tr_vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2],
        };
        let pt_tr_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("pt-tr"),
            layout: Some(&pt_tr_layout),
            vertex: wgpu::VertexState {
                module: pt_shadow_shader,
                entry_point: Some("vs_pt_tr"),
                compilation_options: Default::default(),
                buffers: std::slice::from_ref(&tr_vertex_layout),
            },
            fragment: Some(wgpu::FragmentState {
                module: pt_shadow_shader,
                entry_point: Some("fs_pt_tr"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba16Float,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::Dst,
                            dst_factor: wgpu::BlendFactor::Zero,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::One,
                            operation: wgpu::BlendOperation::Min,
                        },
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

    ShadowPipelines { shadow_pipeline, pt_shadow_pipeline, pt_tr_pipeline }
}
