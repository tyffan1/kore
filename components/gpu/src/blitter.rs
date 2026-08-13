//! Fullscreen blitter used by the browser process to present frames
//! composited by the dedicated GPU process.
//!
//! The browser process owns the window surface; the GPU process renders
//! into an offscreen texture and ships the pixels back over IPC. The
//! [`Blitter`] uploads those pixels into a texture and draws a fullscreen
//! textured quad on the surface.

use crate::error::GpuError;
use crate::pipeline::ImagePipeline;
use crate::renderer::RendererConfig;

pub struct Blitter {
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    surface_config: wgpu::SurfaceConfiguration,
    image_pipeline: ImagePipeline,
    sampler: wgpu::Sampler,
    texture: Option<wgpu::Texture>,
    bind_group: Option<wgpu::BindGroup>,
    image_width: u32,
    image_height: u32,
}

impl Blitter {
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

        let image_pipeline = ImagePipeline::new(&device, surface_format);

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: None,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        Ok(Self {
            device,
            queue,
            surface,
            surface_config,
            image_pipeline,
            sampler,
            texture: None,
            bind_group: None,
            image_width: 0,
            image_height: 0,
        })
    }

    /// Upload a fresh RGBA frame into the blit texture.
    pub fn update_image(&mut self, pixels: &[u8], width: u32, height: u32) -> Result<(), GpuError> {
        if width == 0 || height == 0 {
            return Err(GpuError::FrameState("image has zero size"));
        }
        let expected = width as usize * height as usize * 4;
        if pixels.len() != expected {
            return Err(GpuError::FrameState("image pixel buffer has wrong length"));
        }

        let need_new_texture = self
            .texture
            .as_ref()
            .map(|_| self.image_width != width || self.image_height != height)
            .unwrap_or(true);

        if need_new_texture {
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: None,
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
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
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            });
            self.texture = Some(texture);
            self.bind_group = Some(bind_group);
            self.image_width = width;
            self.image_height = height;
        }

        let texture = self
            .texture
            .as_ref()
            .ok_or(GpuError::FrameState("blit texture missing"))?;
        self.queue.write_texture(
            wgpu::ImageCopyTexture {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            pixels,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        Ok(())
    }

    /// Present the current image fullscreen on the window surface.
    pub fn present(&self) -> Result<(), GpuError> {
        let bind_group = match &self.bind_group {
            Some(bg) => bg,
            None => return Err(GpuError::FrameState("no image uploaded yet")),
        };

        let surface_texture = match self.surface.get_current_texture() {
            Err(wgpu::SurfaceError::Outdated) | Err(wgpu::SurfaceError::Lost) => {
                self.surface
                    .configure(&self.device, &self.surface_config);
                self.surface.get_current_texture()?
            }
            other => other?,
        };

        let view = surface_texture
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
                            r: 0.0,
                            g: 0.0,
                            b: 0.0,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            pass.set_pipeline(&self.image_pipeline.pipeline);
            pass.set_bind_group(0, bind_group, &[]);
            pass.draw(0..6, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        surface_texture.present();
        Ok(())
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.surface_config.width = width;
        self.surface_config.height = height;
        self.surface.configure(&self.device, &self.surface_config);
    }
}