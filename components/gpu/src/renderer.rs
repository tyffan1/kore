use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use wgpu::util::DeviceExt;

use crate::{
    display_list::{DisplayCommand, DisplayList},
    error::GpuError,
    pipeline::{RectPipeline, TextPipeline},
    vertex::{rect_vertices, text_quad_vertices, TextVertex, Vertex, RECT_INDICES},
};

use kore_font::{FontCache, FontDescription, FontId};

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
    rect_pipeline: RectPipeline,
    text_pipeline: TextPipeline,
    viewport_buffer: wgpu::Buffer,
    viewport_bind_group: wgpu::BindGroup,
    text_bind_group: wgpu::BindGroup,
    _placeholder_texture: wgpu::Texture,
    _placeholder_sampler: wgpu::Sampler,
    font_cache: RefCell<FontCache>,
    font_id: FontId,
    glyph_texture_cache: RefCell<HashMap<(usize, char, u32), Arc<wgpu::BindGroup>>>,
}

impl Renderer {
    pub async fn new(
        instance: &wgpu::Instance,
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

        let rect_pipeline = RectPipeline::new(&device, surface_format);
        let text_pipeline = TextPipeline::new(&device, surface_format);

        let viewport_data: [f32; 4] = [config.width as f32, config.height as f32, 0.0, 0.0];
        let viewport_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&viewport_data),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let viewport_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &rect_pipeline.viewport_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: viewport_buffer.as_entire_binding(),
            }],
        });

        // Create a 1x1 white placeholder texture for text pipeline
        let placeholder_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let placeholder_view = placeholder_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let placeholder_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: None,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &placeholder_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &[255u8],
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(1),
                rows_per_image: Some(1),
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );

        let text_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &text_pipeline.texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&placeholder_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&placeholder_sampler),
                },
            ],
        });

        #[cfg(target_os = "windows")]
        const FONT_DATA: &[u8] = include_bytes!("C:/Windows/Fonts/arial.ttf");
        #[cfg(target_os = "macos")]
        const FONT_DATA: &[u8] = include_bytes!("/System/Library/Fonts/Helvetica.ttc");
        #[cfg(target_os = "linux")]
        const FONT_DATA: &[u8] = include_bytes!("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf");
        let font_data: &[u8] = FONT_DATA;
        let mut font_cache = FontCache::new();
        let font_desc = FontDescription::new("Arial", false, false);
        let font_id = font_cache
            .load_font_bytes(font_data, font_desc)
            .map_err(|e| GpuError::Font(e.to_string()))?;

        Ok(Self {
            device,
            queue,
            surface,
            surface_config,
            rect_pipeline,
            text_pipeline,
            viewport_buffer,
            viewport_bind_group,
            text_bind_group,
            _placeholder_texture: placeholder_texture,
            _placeholder_sampler: placeholder_sampler,
            font_cache: RefCell::new(font_cache),
            font_id,
            glyph_texture_cache: RefCell::new(HashMap::new()),
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
        let surface_texture = match self.surface.get_current_texture() {
            Err(wgpu::SurfaceError::Outdated) | Err(wgpu::SurfaceError::Lost) => {
                self.surface
                    .configure(&self.device, &self.surface_config);
                self.surface.get_current_texture()?
            }
            other => other?,
        };
        Ok(FrameRenderer {
            surface_texture,
            rect_vertices: Vec::new(),
            rect_indices: Vec::new(),
            text_vertices: Vec::new(),
            text_indices: Vec::new(),
            glyph_draws: Vec::new(),
        })
    }

    /// Submit a display list.
    /// Text commands are rendered as placeholder colored quads positioned
    /// at the correct glyph locations.
    pub fn submit(&self, frame: &mut FrameRenderer, list: &DisplayList) {
        const MAX_COMMANDS: usize = 100_000;
        if list.commands().len() > MAX_COMMANDS {
            eprintln!("Warning: display list has {} commands, > {} max, skipping", list.commands().len(), MAX_COMMANDS);
            return;
        }

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
                    let base = frame.rect_vertices.len() as u16;
                    let color = [r.color.r, r.color.g, r.color.b, r.color.a];
                    let verts = rect_vertices(r.x, r.y, r.width, r.height, color);
                    frame.rect_vertices.extend_from_slice(&verts);
                    for &i in &RECT_INDICES {
                        frame.rect_indices.push(base + i);
                    }
                }
                DisplayCommand::Text(t) => {
                    if let Some(clip) = clip_stack.last() {
                        let approx_w = t.font_size * t.text.len() as f32 * 0.6;
                        let approx_h = t.font_size * 1.2;
                        let rect_clip = crate::display_list::ClipRect {
                            x: t.x,
                            y: t.y,
                            width: approx_w,
                            height: approx_h,
                        };
                        if !clip.intersects(&rect_clip) {
                            continue;
                        }
                    }
                    let color = [t.color.r, t.color.g, t.color.b, t.color.a];
                    let mut cursor_x = t.x;
                    let mut font_cache = self.font_cache.borrow_mut();
                    for ch in t.text.chars() {
                        if let Some(glyph) = font_cache.rasterize_glyph(self.font_id, ch, t.font_size) {
                            if glyph.width > 0 && glyph.height > 0 {
                                let cache_key = (self.font_id.0, ch, t.font_size.to_bits());
                                let bind_group = {
                                    let mut glyph_cache = self.glyph_texture_cache.borrow_mut();
                                    if let Some(cached) = glyph_cache.get(&cache_key) {
                                        cached.clone()
                                    } else {
                                        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                                            label: None,
                                            size: wgpu::Extent3d { width: glyph.width, height: glyph.height, depth_or_array_layers: 1 },
                                            mip_level_count: 1,
                                            sample_count: 1,
                                            dimension: wgpu::TextureDimension::D2,
                                            format: wgpu::TextureFormat::R8Unorm,
                                            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                                            view_formats: &[],
                                        });
                                        self.queue.write_texture(
                                            wgpu::ImageCopyTexture { texture: &texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                                            &glyph.pixels,
                                            wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(glyph.width), rows_per_image: Some(glyph.height) },
                                            wgpu::Extent3d { width: glyph.width, height: glyph.height, depth_or_array_layers: 1 },
                                        );
                                        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
                                        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                                            label: None,
                                            layout: &self.text_pipeline.texture_bind_group_layout,
                                            entries: &[
                                                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                                                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self._placeholder_sampler) },
                                            ],
                                        });
                                        let arc_bg = Arc::new(bind_group);
                                        glyph_cache.insert(cache_key, arc_bg.clone());
                                        arc_bg
                                    }
                                };
                                let dest_x = cursor_x + glyph.x_offset as f32;
                                let dest_y = t.y - glyph.y_offset as f32 - glyph.height as f32;
                                let verts = text_quad_vertices(dest_x, dest_y, glyph.width as f32, glyph.height as f32, 0.0, 0.0, 1.0, 1.0, color);
                                let vertex_base = frame.text_vertices.len() as u16;
                                frame.text_vertices.extend_from_slice(&verts);
                                let index_base = frame.text_indices.len() as u32;
                                for &i in &RECT_INDICES {
                                    frame.text_indices.push(vertex_base as u32 + i as u32);
                                }
                                frame.glyph_draws.push(GlyphDraw { index_start: index_base, index_count: 6, bind_group });
                            }
                            cursor_x += glyph.advance_width;
                        } else {
                            cursor_x += t.font_size * 0.6;
                        }
                    }
                }
                DisplayCommand::PushClip(c) => {
                    clip_stack.push(*c);
                }
                DisplayCommand::PopClip => {
                    clip_stack.pop();
                }
                DisplayCommand::Image(_) => {}
            }
        }
    }

    pub fn end_frame(&self, frame: FrameRenderer) -> Result<(), GpuError> {
        let rect_empty = frame.rect_vertices.is_empty();
        let text_empty = frame.text_vertices.is_empty();

        if rect_empty && text_empty {
            frame.surface_texture.present();
            return Ok(());
        }

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

            if !rect_empty {
                let vertex_buffer =
                    self.device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: None,
                            contents: bytemuck::cast_slice(&frame.rect_vertices),
                            usage: wgpu::BufferUsages::VERTEX,
                        });

                let index_buffer =
                    self.device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: None,
                            contents: bytemuck::cast_slice(&frame.rect_indices),
                            usage: wgpu::BufferUsages::INDEX,
                        });

                pass.set_pipeline(&self.rect_pipeline.pipeline);
                pass.set_bind_group(0, &self.viewport_bind_group, &[]);
                pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                pass.draw_indexed(0..frame.rect_indices.len() as u32, 0, 0..1);
            }

            if !text_empty {
                let vertex_buffer =
                    self.device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: None,
                            contents: bytemuck::cast_slice(&frame.text_vertices),
                            usage: wgpu::BufferUsages::VERTEX,
                        });

                let index_buffer =
                    self.device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: None,
                            contents: bytemuck::cast_slice(&frame.text_indices),
                            usage: wgpu::BufferUsages::INDEX,
                        });

                pass.set_pipeline(&self.text_pipeline.pipeline);
                pass.set_bind_group(0, &self.viewport_bind_group, &[]);
                pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                for draw in &frame.glyph_draws {
                    pass.set_bind_group(1, &*draw.bind_group, &[]);
                    pass.draw_indexed(draw.index_start..draw.index_start + draw.index_count, 0, 0..1);
                }
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        frame.surface_texture.present();

        Ok(())
    }
}

struct GlyphDraw {
    index_start: u32,
    index_count: u32,
    bind_group: Arc<wgpu::BindGroup>,
}

pub struct FrameRenderer {
    pub(crate) surface_texture: wgpu::SurfaceTexture,
    pub(crate) rect_vertices: Vec<Vertex>,
    pub(crate) rect_indices: Vec<u16>,
    pub(crate) text_vertices: Vec<TextVertex>,
    pub(crate) text_indices: Vec<u32>,
    glyph_draws: Vec<GlyphDraw>,
}
