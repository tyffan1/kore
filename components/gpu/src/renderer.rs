use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use wgpu::util::DeviceExt;

use crate::{
    display_list::{DisplayCommand, DisplayList, GpuImage},
    error::GpuError,
    pipeline::{CirclePipeline, ImageQuadPipeline, RectPipeline, TextPipeline},
    vertex::{circle_quad_vertices, rect_vertices, text_quad_vertices, CircleVertex, TextVertex, Vertex, RECT_INDICES},
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

/// An offscreen render target used by the dedicated GPU process.
#[derive(Debug, Clone)]
pub struct OffscreenTarget {
    pub texture: Arc<wgpu::Texture>,
    pub view: Arc<wgpu::TextureView>,
    pub width: u32,
    pub height: u32,
}

/// Where a frame will be presented: an on-screen surface (browser process)
/// or an offscreen texture (GPU process).
pub enum FrameTarget {
    Surface(wgpu::SurfaceTexture),
    Offscreen(OffscreenTarget),
}

/// Outcome of finishing a frame.
pub enum FrameResult {
    /// The frame was presented to the window surface.
    Presented,
    /// The frame was rendered offscreen and read back as RGBA pixels.
    Pixels(GpuImage),
}

/// Device resources shared by surface and offscreen renderers.
struct CommonResources {
    rect_pipeline: RectPipeline,
    circle_pipeline: CirclePipeline,
    text_pipeline: TextPipeline,
    image_pipeline: ImageQuadPipeline,
    viewport_buffer: wgpu::Buffer,
    viewport_bind_group: wgpu::BindGroup,
    placeholder_texture: wgpu::Texture,
    placeholder_sampler: wgpu::Sampler,
    font_cache: FontCache,
    font_id: FontId,
    font_family_map: HashMap<String, FontId>,
}

pub struct Renderer {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub surface: Option<wgpu::Surface<'static>>,
    pub surface_config: Option<wgpu::SurfaceConfiguration>,
    offscreen: Option<OffscreenTarget>,
    rect_pipeline: RectPipeline,
    circle_pipeline: CirclePipeline,
    text_pipeline: TextPipeline,
    image_pipeline: ImageQuadPipeline,
    viewport_buffer: wgpu::Buffer,
    viewport_bind_group: wgpu::BindGroup,
    _placeholder_texture: wgpu::Texture,
    placeholder_sampler: wgpu::Sampler,
    font_cache: RefCell<FontCache>,
    font_id: FontId,
    glyph_texture_cache: RefCell<HashMap<(usize, char, u32), Arc<wgpu::BindGroup>>>,
    image_texture_cache: RefCell<HashMap<u64, Arc<wgpu::BindGroup>>>,
    font_family_map: HashMap<String, FontId>,
}

impl Renderer {
    /// Create a renderer that presents to a window surface.
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

        let common = Self::init_common(&device, &queue, surface_format, config.width, config.height)?;

        let mut renderer = Self::from_common(common, device, queue);
        renderer.surface = Some(surface);
        renderer.surface_config = Some(surface_config);
        Ok(renderer)
    }

    /// Create a renderer with no window surface, targeting an offscreen
    /// texture that can be read back as RGBA pixels. Used by the dedicated
    /// GPU process.
    pub async fn new_offscreen(
        instance: &wgpu::Instance,
        config: RendererConfig,
    ) -> Result<Self, GpuError> {
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
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

        let format = wgpu::TextureFormat::Rgba8UnormSrgb;
        let common = Self::init_common(&device, &queue, format, config.width, config.height)?;

        let mut renderer = Self::from_common(common, device, queue);
        renderer.offscreen = Some(Self::create_offscreen_target(&renderer.device, config.width, config.height));
        Ok(renderer)
    }

    fn from_common(common: CommonResources, device: wgpu::Device, queue: wgpu::Queue) -> Self {
        Self {
            device,
            queue,
            surface: None,
            surface_config: None,
            offscreen: None,
            rect_pipeline: common.rect_pipeline,
            circle_pipeline: common.circle_pipeline,
            text_pipeline: common.text_pipeline,
            image_pipeline: common.image_pipeline,
            viewport_buffer: common.viewport_buffer,
            viewport_bind_group: common.viewport_bind_group,
            _placeholder_texture: common.placeholder_texture,
            placeholder_sampler: common.placeholder_sampler,
            font_cache: RefCell::new(common.font_cache),
            font_id: common.font_id,
            glyph_texture_cache: RefCell::new(HashMap::new()),
            image_texture_cache: RefCell::new(HashMap::new()),
            font_family_map: common.font_family_map,
        }
    }

    /// Create the device-independent resources (pipelines, buffers, fonts).
    fn init_common(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) -> Result<CommonResources, GpuError> {
        let rect_pipeline = RectPipeline::new(device, format);
        let circle_pipeline = CirclePipeline::new(device, format, &rect_pipeline.viewport_bind_group_layout);
        let text_pipeline = TextPipeline::new(device, format);
        let image_pipeline = ImageQuadPipeline::new(device, format);

        let viewport_data: [f32; 4] = [width as f32, height as f32, 0.0, 0.0];
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

        // ── Font loading with platform-specific fonts ──
        let mut font_cache = FontCache::new();
        let (font_id, font_family_map) = Self::load_platform_fonts(&mut font_cache)?;

        Ok(CommonResources {
            rect_pipeline,
            circle_pipeline,
            text_pipeline,
            image_pipeline,
            viewport_buffer,
            viewport_bind_group,
            placeholder_texture,
            placeholder_sampler,
            font_cache,
            font_id,
            font_family_map,
        })
    }

    fn create_offscreen_target(device: &wgpu::Device, width: u32, height: u32) -> OffscreenTarget {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        OffscreenTarget {
            texture: Arc::new(texture),
            view: Arc::new(view),
            width: width.max(1),
            height: height.max(1),
        }
    }

    /// Load the best available font(s) for the current platform.
    ///
    /// Returns the primary font ID and a family→FontId map used for
    /// per-`DrawText` font-family lookups.
    fn load_platform_fonts(font_cache: &mut FontCache) -> Result<(FontId, HashMap<String, FontId>), GpuError> {
        let mut family_map: HashMap<String, FontId> = HashMap::new();
        let primary: FontId;

        #[cfg(target_os = "macos")]
        {
            // Try SF Pro Text first, then SF Pro Display, then Helvetica
            let candidates = [
                ("/System/Library/Fonts/SFNSText.ttf", "SF Pro Text"),
                ("/System/Library/Fonts/SFNS.ttf", "SF Pro Display"),
                ("/System/Library/Fonts/Helvetica.ttc", "Helvetica Neue"),
            ];

            let mut loaded = false;
            for (path, family) in &candidates {
                if let Ok(data) = std::fs::read(path) {
                    let desc = FontDescription::new(family, false, false);
                    if let Ok(id) = font_cache.load_font_bytes(&data, desc) {
                        family_map.insert(family.to_string(), id);
                        if !loaded {
                            primary = id;
                            loaded = true;
                        }
                    }
                }
            }
            if !loaded {
                return Err(GpuError::Font("No fonts found on macOS".to_string()));
            }
        }

        #[cfg(target_os = "windows")]
        {
            let data = include_bytes!("C:/Windows/Fonts/arial.ttf");
            let desc = FontDescription::new("Arial", false, false);
            let id = font_cache
                .load_font_bytes(data, desc)
                .map_err(|e| GpuError::Font(e.to_string()))?;
            family_map.insert("Arial".to_string(), id);
            primary = id;
        }

        #[cfg(target_os = "linux")]
        {
            let data = include_bytes!("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf");
            let desc = FontDescription::new("DejaVu Sans", false, false);
            let id = font_cache
                .load_font_bytes(data, desc)
                .map_err(|e| GpuError::Font(e.to_string()))?;
            family_map.insert("DejaVu Sans".to_string(), id);
            primary = id;
        }

        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        compile_error!("Unsupported platform");

        Ok((primary, family_map))
    }

    /// Resolve the best FontId for a given font-family hint.
    ///
    /// Falls back through the chain: family → "SF Pro Text" → "Helvetica Neue"
    /// → "Arial" → "DejaVu Sans" → first loaded font.
    fn resolve_font(&self, family: Option<&str>) -> FontId {
        let fallback_chain = ["SF Pro Text", "SF Pro Display", "Helvetica Neue", "Arial", "DejaVu Sans"];
        if let Some(f) = family {
            if let Some(&id) = self.font_family_map.get(f) {
                return id;
            }
        }
        for name in &fallback_chain {
            if let Some(&id) = self.font_family_map.get(*name) {
                return id;
            }
        }
        self.font_id
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        if let Some(surface) = &self.surface {
            if let Some(config) = &mut self.surface_config {
                config.width = width;
                config.height = height;
                surface.configure(&self.device, config);
            }
        } else if let Some(target) = &mut self.offscreen {
            *target = Self::create_offscreen_target(&self.device, width, height);
        }

        let viewport_data: [f32; 4] = [width as f32, height as f32, 0.0, 0.0];
        self.queue.write_buffer(
            &self.viewport_buffer,
            0,
            bytemuck::cast_slice(&viewport_data),
        );
    }

    pub fn begin_frame(&self) -> Result<FrameRenderer, GpuError> {
        let target = if let Some(surface) = &self.surface {
            let surface_texture = match surface.get_current_texture() {
                Err(wgpu::SurfaceError::Outdated) | Err(wgpu::SurfaceError::Lost) => {
                    if let Some(config) = &self.surface_config {
                        surface.configure(&self.device, config);
                    }
                    surface.get_current_texture()?
                }
                other => other?,
            };
            FrameTarget::Surface(surface_texture)
        } else if let Some(target) = &self.offscreen {
            FrameTarget::Offscreen(target.clone())
        } else {
            return Err(GpuError::NoSurface);
        };
        Ok(FrameRenderer {
            target,
            rect_vertices: Vec::new(),
            rect_indices: Vec::new(),
            circle_vertices: Vec::new(),
            circle_indices: Vec::new(),
            text_vertices: Vec::new(),
            text_indices: Vec::new(),
            glyph_draws: Vec::new(),
            image_vertices: Vec::new(),
            image_indices: Vec::new(),
            image_draws: Vec::new(),
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
                    let render_x = r.x + r.translate.0;
                    let render_y = r.y + r.translate.1;
                    if let Some(clip) = clip_stack.last() {
                        let rect_clip = crate::display_list::ClipRect {
                            x: render_x,
                            y: render_y,
                            width: r.width,
                            height: r.height,
                        };
                        if !clip.intersects(&rect_clip) {
                            continue;
                        }
                        if render_y + r.height < clip.y || render_y > clip.y + clip.height {
                            continue;
                        }
                    }
                    let base = frame.rect_vertices.len() as u16;
                    let effective_alpha = r.color.a * r.opacity;
                    let color = [r.color.r, r.color.g, r.color.b, effective_alpha];
                    let verts = rect_vertices(render_x, render_y, r.width, r.height, color);
                    frame.rect_vertices.extend_from_slice(&verts);
                    for &i in &RECT_INDICES {
                        frame.rect_indices.push(base + i);
                    }
                }
                DisplayCommand::Text(t) => {
                    let render_x = t.x + t.translate.0;
                    let render_y = t.y + t.translate.1;
                    if let Some(clip) = clip_stack.last() {
                        let approx_w = t.font_size * t.text.len() as f32 * 0.6;
                        let approx_h = t.font_size * 1.2;
                        let rect_clip = crate::display_list::ClipRect {
                            x: render_x,
                            y: render_y,
                            width: approx_w,
                            height: approx_h,
                        };
                        if !clip.intersects(&rect_clip) {
                            continue;
                        }
                        if render_y + t.font_size < clip.y || render_y > clip.y + clip.height {
                            continue;
                        }
                        if render_y < clip.y {
                            continue;
                        }
                    }
                    let effective_alpha = t.color.a * t.opacity;
                    let color = [t.color.r, t.color.g, t.color.b, effective_alpha];
                    let mut cursor_x = render_x;
                    let mut font_cache = self.font_cache.borrow_mut();
                    let font_id = self.resolve_font(t.font_family.as_deref());
                    for ch in t.text.chars() {
                        if let Some(glyph) = font_cache.rasterize_glyph(font_id, ch, t.font_size) {
                            if glyph.width > 0 && glyph.height > 0 {
                                let cache_key = (font_id.0, ch, t.font_size.to_bits());
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
                                                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.placeholder_sampler) },
                                            ],
                                        });
                                        let arc_bg = Arc::new(bind_group);
                                        glyph_cache.insert(cache_key, arc_bg.clone());
                                        arc_bg
                                    }
                                };
                                let dest_x = cursor_x + glyph.x_offset as f32;
                                let dest_y = render_y - glyph.y_offset as f32 - glyph.height as f32;
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
                DisplayCommand::Circle(c) => {
                    let r = c.radius;
                    let base = frame.circle_vertices.len() as u16;
                    let color = [c.color.r, c.color.g, c.color.b, c.color.a];
                    let verts = circle_quad_vertices(c.cx, c.cy, r, color);
                    frame.circle_vertices.extend_from_slice(&verts);
                    for &i in &RECT_INDICES {
                        frame.circle_indices.push(base + i);
                    }
                }
                DisplayCommand::PushClip(c) => {
                    clip_stack.push(*c);
                }
                DisplayCommand::PopClip => {
                    clip_stack.pop();
                }
                DisplayCommand::Image(im) => {
                    if im.width <= 0.0 || im.height <= 0.0 {
                        continue;
                    }
                    if let Some(clip) = clip_stack.last() {
                        let rect_clip = crate::display_list::ClipRect {
                            x: im.x,
                            y: im.y,
                            width: im.width,
                            height: im.height,
                        };
                        if !clip.intersects(&rect_clip) {
                            continue;
                        }
                        if im.y + im.height < clip.y || im.y > clip.y + clip.height {
                            continue;
                        }
                    }
                    let image = &im.image;
                    if image.width == 0 || image.height == 0 {
                        continue;
                    }
                    let expected = image.width as usize * image.height as usize * 4;
                    if image.pixels.len() != expected {
                        continue;
                    }
                    let key = image_cache_key(image);
                    let bind_group = {
                        let mut cache = self.image_texture_cache.borrow_mut();
                        if let Some(cached) = cache.get(&key) {
                            cached.clone()
                        } else {
                            let bind_group = self.upload_image_texture(image);
                            if let Some(bind_group) = bind_group {
                                cache.insert(key, bind_group.clone());
                                bind_group
                            } else {
                                continue;
                            }
                        }
                    };
                    let base = frame.image_vertices.len() as u16;
                    let verts = text_quad_vertices(im.x, im.y, im.width, im.height, 0.0, 0.0, 1.0, 1.0, [1.0, 1.0, 1.0, 1.0]);
                    frame.image_vertices.extend_from_slice(&verts);
                    let index_base = frame.image_indices.len() as u32;
                    for &i in &RECT_INDICES {
                        frame.image_indices.push(base as u32 + i as u32);
                    }
                    frame.image_draws.push(GlyphDraw { index_start: index_base, index_count: 6, bind_group });
                }
            }
        }
    }

    /// Upload RGBA pixels into a texture and return a bind group for it.
    fn upload_image_texture(&self, image: &GpuImage) -> Option<Arc<wgpu::BindGroup>> {
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: image.width,
                height: image.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &image.pixels,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(image.width * 4),
                rows_per_image: Some(image.height),
            },
            wgpu::Extent3d {
                width: image.width,
                height: image.height,
                depth_or_array_layers: 1,
            },
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.image_pipeline.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.placeholder_sampler),
                },
            ],
        });
        Some(Arc::new(bind_group))
    }

    /// Finish a frame. For offscreen renderers the pixels are read back
    /// into a [`GpuImage`].
    pub async fn end_frame(&self, frame: FrameRenderer) -> Result<FrameResult, GpuError> {
        let rect_empty = frame.rect_vertices.is_empty();
        let circle_empty = frame.circle_vertices.is_empty();
        let text_empty = frame.text_vertices.is_empty();

        let is_surface = matches!(&frame.target, FrameTarget::Surface(_));

        if rect_empty && circle_empty && text_empty && is_surface {
            match frame.target {
                FrameTarget::Surface(st) => st.present(),
                FrameTarget::Offscreen(_) => {}
            }
            return Ok(FrameResult::Presented);
        }

        let view = match &frame.target {
            FrameTarget::Surface(st) => {
                st.texture
                    .create_view(&wgpu::TextureViewDescriptor::default())
            }
            FrameTarget::Offscreen(t) => {
                t.texture
                    .create_view(&wgpu::TextureViewDescriptor::default())
            }
        };

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

            if !circle_empty {
                let vertex_buffer =
                    self.device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: None,
                            contents: bytemuck::cast_slice(&frame.circle_vertices),
                            usage: wgpu::BufferUsages::VERTEX,
                        });

                let index_buffer =
                    self.device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: None,
                            contents: bytemuck::cast_slice(&frame.circle_indices),
                            usage: wgpu::BufferUsages::INDEX,
                        });

                pass.set_pipeline(&self.circle_pipeline.pipeline);
                pass.set_bind_group(0, &self.viewport_bind_group, &[]);
                pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                pass.draw_indexed(0..frame.circle_indices.len() as u32, 0, 0..1);
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
                    pass.set_bind_group(1, &draw.bind_group, &[]);
                    pass.draw_indexed(draw.index_start..draw.index_start + draw.index_count, 0, 0..1);
                }
            }

            if !frame.image_vertices.is_empty() {
                let vertex_buffer =
                    self.device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: None,
                            contents: bytemuck::cast_slice(&frame.image_vertices),
                            usage: wgpu::BufferUsages::VERTEX,
                        });

                let index_buffer =
                    self.device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: None,
                            contents: bytemuck::cast_slice(&frame.image_indices),
                            usage: wgpu::BufferUsages::INDEX,
                        });

                pass.set_pipeline(&self.image_pipeline.pipeline);
                pass.set_bind_group(0, &self.viewport_bind_group, &[]);
                pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                for draw in &frame.image_draws {
                    pass.set_bind_group(1, &draw.bind_group, &[]);
                    pass.draw_indexed(draw.index_start..draw.index_start + draw.index_count, 0, 0..1);
                }
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));

        match frame.target {
            FrameTarget::Surface(st) => {
                st.present();
                Ok(FrameResult::Presented)
            }
            FrameTarget::Offscreen(t) => {
                let bytes_per_pixel = 4u32;
                let bytes_per_row = t.width * bytes_per_pixel;
                let padded_row = bytes_per_row.div_ceil(256) * 256;
                let buffer_size = (padded_row * t.height) as u64;

                let output_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: None,
                    size: buffer_size,
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                });

                let mut copy_encoder = self.device.create_command_encoder(
                    &wgpu::CommandEncoderDescriptor { label: None },
                );
                copy_encoder.copy_texture_to_buffer(
                    wgpu::ImageCopyTexture {
                        texture: &t.texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::ImageCopyBuffer {
                        buffer: &output_buffer,
                        layout: wgpu::ImageDataLayout {
                            offset: 0,
                            bytes_per_row: Some(padded_row),
                            rows_per_image: Some(t.height),
                        },
                    },
                    wgpu::Extent3d {
                        width: t.width,
                        height: t.height,
                        depth_or_array_layers: 1,
                    },
                );
                self.queue.submit(std::iter::once(copy_encoder.finish()));

                let slice = output_buffer.slice(..);
                let (map_tx, map_rx) = std::sync::mpsc::channel();
                slice.map_async(wgpu::MapMode::Read, move |result| {
                    let _ = map_tx.send(result);
                });
                self.device.poll(wgpu::Maintain::Wait);

                map_rx
                    .recv()
                    .map_err(|_| GpuError::Readback("map channel closed".into()))?
                    .map_err(|e| GpuError::Readback(e.to_string()))?;

                let mapped = slice.get_mapped_range();
                let mut pixels = Vec::with_capacity((bytes_per_row * t.height) as usize);
                for row in 0..t.height as usize {
                    let start = row * padded_row as usize;
                    pixels.extend_from_slice(&mapped[start..start + bytes_per_row as usize]);
                }

                Ok(FrameResult::Pixels(GpuImage {
                    width: t.width,
                    height: t.height,
                    pixels,
                }))
            }
        }
    }

    /// Render a display list offscreen and return the RGBA pixels.
    pub async fn render_to_image(&self, list: &DisplayList) -> Result<GpuImage, GpuError> {
        let mut frame = self.begin_frame()?;
        self.submit(&mut frame, list);
        match self.end_frame(frame).await? {
            FrameResult::Pixels(image) => Ok(image),
            FrameResult::Presented => Err(GpuError::SurfaceOnly),
        }
    }
}

struct GlyphDraw {
    index_start: u32,
    index_count: u32,
    bind_group: Arc<wgpu::BindGroup>,
}

pub struct FrameRenderer {
    pub(crate) target: FrameTarget,
    pub(crate) rect_vertices: Vec<Vertex>,
    pub(crate) rect_indices: Vec<u16>,
    pub(crate) circle_vertices: Vec<CircleVertex>,
    pub(crate) circle_indices: Vec<u16>,
    pub(crate) text_vertices: Vec<TextVertex>,
    pub(crate) text_indices: Vec<u32>,
    glyph_draws: Vec<GlyphDraw>,
    image_vertices: Vec<TextVertex>,
    image_indices: Vec<u32>,
    image_draws: Vec<GlyphDraw>,
}

/// FNV-1a hash of the RGBA payload; identical pixel buffers share one texture.
fn image_cache_key(image: &GpuImage) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    let mut mix = |byte: u8| {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    };
    mix((image.width & 0xff) as u8);
    mix(((image.width >> 8) & 0xff) as u8);
    mix((image.width >> 16) as u8);
    mix((image.width >> 24) as u8);
    mix((image.height & 0xff) as u8);
    mix(((image.height >> 8) & 0xff) as u8);
    mix((image.height >> 16) as u8);
    mix((image.height >> 24) as u8);
    for byte in &image.pixels {
        mix(*byte);
    }
    hash
}