use wgpu::util::DeviceExt;

use crate::{
    display_list::{DisplayCommand, DisplayList},
    error::GpuError,
    pipeline::RectPipeline,
    vertex::{rect_vertices, Vertex, RECT_INDICES},
};

#[derive(Debug, Clone)]
pub struct RendererConfig {
    pub width: u32,
    pub height: u32,
    pub present_mode: wgpu::PresentMode,
}

impl Default for RendererConfig {
    fn default() -> Self {
        Self {
            width: 800,
            height: 600,
            present_mode: wgpu::PresentMode::Fifo,
        }
    }
}

pub struct Renderer {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub surface: wgpu::Surface<'static>,
    pub surface_config: wgpu::SurfaceConfiguration,
    pipeline: RectPipeline,
    viewport_buffer: wgpu::Buffer,
    viewport_bind_group: wgpu::BindGroup,
}

impl Renderer {
    pub async fn new(
        instance: wgpu::Instance,
        surface: wgpu::Surface<'static>,
        config: RendererConfig,
    ) -> Result<Self, GpuError> {
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .ok_or(GpuError::NoAdapter)?;

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: None,
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    memory_hints: wgpu::MemoryHints::default(),
                },
                None,
            )
            .await?;

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);

        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: config.width,
            height: config.height,
            present_mode: config.present_mode,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &surface_config);

        let pipeline = RectPipeline::new(&device, surface_format);

        let viewport_data: [f32; 4] = [config.width as f32, config.height as f32, 0.0, 0.0];
        let viewport_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&viewport_data),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let viewport_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.viewport_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: viewport_buffer.as_entire_binding(),
            }],
        });

        Ok(Self {
            device,
            queue,
            surface,
            surface_config,
            pipeline,
            viewport_buffer,
            viewport_bind_group,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.surface_config.width = width;
        self.surface_config.height = height;
        self.surface.configure(&self.device, &self.surface_config);

        let viewport_data: [f32; 4] = [width as f32, height as f32, 0.0, 0.0];
        self.queue.write_buffer(
            &self.viewport_buffer,
            0,
            bytemuck::cast_slice(&viewport_data),
        );
    }

    pub fn begin_frame(&self) -> Result<FrameRenderer, GpuError> {
        let surface_texture = self.surface.get_current_texture()?;
        Ok(FrameRenderer {
            surface_texture,
            vertices: Vec::new(),
            indices: Vec::new(),
        })
    }

    pub fn submit(&self, frame: &mut FrameRenderer, list: &DisplayList) {
        let mut clip_stack: Vec<crate::display_list::ClipRect> = Vec::new();

        for cmd in list.commands() {
            match cmd {
                DisplayCommand::Rect(r) => {
                    if let Some(clip) = clip_stack.last() {
                        let rect_clip = crate::display_list::ClipRect {
                            x: r.x,
                            y: r.y,
                            width: r.width,
                            height: r.height,
                        };
                        if !clip.intersects(&rect_clip) {
                            continue;
                        }
                    }
                    let base = frame.vertices.len() as u16;
                    let color = [r.color.r, r.color.g, r.color.b, r.color.a];
                    let verts = rect_vertices(r.x, r.y, r.width, r.height, color);
                    frame.vertices.extend_from_slice(&verts);
                    for &i in &RECT_INDICES {
                        frame.indices.push(base + i);
                    }
                }
                DisplayCommand::PushClip(c) => {
                    clip_stack.push(*c);
                }
                DisplayCommand::PopClip => {
                    clip_stack.pop();
                }
                DisplayCommand::Text(_) | DisplayCommand::Image(_) => {}
            }
        }
    }

    pub fn end_frame(&self, frame: FrameRenderer) -> Result<(), GpuError> {
        if frame.vertices.is_empty() {
            frame.surface_texture.present();
            return Ok(());
        }

        let vertex_buffer =
            self.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: None,
                    contents: bytemuck::cast_slice(&frame.vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                });

        let index_buffer =
            self.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: None,
                    contents: bytemuck::cast_slice(&frame.indices),
                    usage: wgpu::BufferUsages::INDEX,
                });

        let view = frame
            .surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder =
            self.device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 1.0,
                            g: 1.0,
                            b: 1.0,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            pass.set_pipeline(&self.pipeline.pipeline);
            pass.set_bind_group(0, &self.viewport_bind_group, &[]);
            pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            pass.draw_indexed(0..frame.indices.len() as u32, 0, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        frame.surface_texture.present();

        Ok(())
    }
}

pub struct FrameRenderer {
    pub(crate) surface_texture: wgpu::SurfaceTexture,
    pub(crate) vertices: Vec<Vertex>,
    pub(crate) indices: Vec<u16>,
}
