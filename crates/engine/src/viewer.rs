use crate::{Camera, DeviceScene, GpuContext, HostScene, Result, scene::Vertex};
use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Uniform {
    view_projection: [[f32; 4]; 4],
}

struct SurfaceState {
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    depth: wgpu::TextureView,
    pipeline: wgpu::RenderPipeline,
    uniform: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

/// Script-owned OBJ renderer. A platform host attaches its window or canvas
/// surface when that platform allows it. No camera or callbacks are retained.
pub struct ObjViewer {
    context: GpuContext,
    surface: Option<SurfaceState>,
    scene: Option<DeviceScene>,
    background: [f32; 4],
    title: String,
    size: (u32, u32),
    closed: bool,
    surface_lost: bool,
}

impl ObjViewer {
    pub fn new(context: GpuContext) -> Self {
        Self {
            context,
            surface: None,
            scene: None,
            background: [0.08, 0.09, 0.12, 1.0],
            title: "OBJ viewer".into(),
            size: (640, 480),
            closed: false,
            surface_lost: false,
        }
    }
    pub fn attach_surface(
        &mut self,
        surface: wgpu::Surface<'static>,
        width: u32,
        height: u32,
    ) -> Result<()> {
        if self.closed {
            return Err("viewer is closed".into());
        }
        let context = &self.context;
        let config = surface
            .get_default_config(&context.adapter, width.max(1), height.max(1))
            .ok_or("surface is incompatible with the selected GPU adapter")?;
        surface.configure(context.device(), &config);
        let depth = Self::create_depth(context, &config);
        let shader = context
            .device()
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("OBJ shader"),
                source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
            });
        let uniform = context
            .device()
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("camera uniform"),
                contents: bytemuck::bytes_of(&Uniform {
                    view_projection: Camera::default()
                        .view_projection(width, height)
                        .to_cols_array_2d(),
                }),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });
        let layout = context
            .device()
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("camera layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });
        let bind_group = context
            .device()
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("camera bind group"),
                layout: &layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                }],
            });
        let pipeline_layout =
            context
                .device()
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("OBJ pipeline layout"),
                    bind_group_layouts: &[Some(&layout)],
                    immediate_size: 0,
                });
        let pipeline = context
            .device()
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("OBJ pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[Some(Vertex::layout())],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: config.format,
                        blend: Some(wgpu::BlendState::REPLACE),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    cull_mode: Some(wgpu::Face::Back),
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth24Plus,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::Less),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            });
        self.size = (config.width, config.height);
        self.surface = Some(SurfaceState {
            surface,
            config,
            depth,
            pipeline,
            uniform,
            bind_group,
        });
        self.surface_lost = false;
        Ok(())
    }
    fn create_depth(
        context: &GpuContext,
        config: &wgpu::SurfaceConfiguration,
    ) -> wgpu::TextureView {
        context
            .device()
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("depth"),
                size: wgpu::Extent3d {
                    width: config.width,
                    height: config.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Depth24Plus,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            })
            .create_view(&Default::default())
    }
    pub fn context(&self) -> &GpuContext {
        &self.context
    }
    pub fn size(&self) -> (u32, u32) {
        self.size
    }
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.size = (width, height);
        if let Some(state) = &mut self.surface {
            state.config.width = width;
            state.config.height = height;
            state
                .surface
                .configure(self.context.device(), &state.config);
            state.depth = Self::create_depth(&self.context, &state.config);
        }
    }
    pub fn set_host_scene(&mut self, scene: &HostScene) -> Result<()> {
        self.scene = Some(scene.upload(&self.context)?);
        Ok(())
    }
    pub fn set_device_scene(&mut self, scene: DeviceScene) -> Result<()> {
        if !scene.compatible(&self.context) {
            return Err("device scene belongs to a different GPU context".into());
        }
        self.scene = Some(scene);
        Ok(())
    }
    pub fn scene(&self) -> Option<&DeviceScene> {
        self.scene.as_ref()
    }
    pub fn set_background_color(&mut self, color: [f32; 4]) {
        self.background = color;
    }
    pub fn background_color(&self) -> [f32; 4] {
        self.background
    }
    pub fn set_title(&mut self, title: impl Into<String>) {
        self.title = title.into();
    }
    pub fn title(&self) -> &str {
        &self.title
    }
    pub fn is_attached(&self) -> bool {
        self.surface.is_some()
    }
    pub fn is_closed(&self) -> bool {
        self.closed
    }
    pub fn needs_surface_recreation(&self) -> bool {
        self.surface_lost
    }
    pub fn close(&mut self) {
        self.closed = true;
        self.surface_lost = false;
        self.surface = None;
        self.scene = None;
    }
    pub fn render(&mut self, camera: &Camera) -> Result<()> {
        if !camera.is_valid() {
            return Err("camera parameters are invalid".into());
        }
        let state = self
            .surface
            .as_mut()
            .ok_or("viewer has no attached surface")?;
        let frame = match state.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            wgpu::CurrentSurfaceTexture::Lost => {
                self.surface_lost = true;
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                state
                    .surface
                    .configure(self.context.device(), &state.config);
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                return Err("surface validation failed".into());
            }
        };
        let matrix = Uniform {
            view_projection: camera
                .view_projection(state.config.width, state.config.height)
                .to_cols_array_2d(),
        };
        self.context
            .queue()
            .write_buffer(&state.uniform, 0, bytemuck::bytes_of(&matrix));
        let view = frame.texture.create_view(&Default::default());
        let mut encoder =
            self.context
                .device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("OBJ frame"),
                });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("OBJ pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: self.background[0] as f64,
                            g: self.background[1] as f64,
                            b: self.background[2] as f64,
                            a: self.background[3] as f64,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &state.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                occlusion_query_set: None,
                timestamp_writes: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&state.pipeline);
            pass.set_bind_group(0, &state.bind_group, &[]);
            if let Some(scene) = &self.scene {
                for (vertex, index, count) in scene.meshes() {
                    pass.set_vertex_buffer(0, vertex.slice(..));
                    pass.set_index_buffer(index.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..count, 0, 0..1);
                }
            }
        }
        self.context.queue().submit(Some(encoder.finish()));
        self.context.queue().present(frame);
        Ok(())
    }
}
