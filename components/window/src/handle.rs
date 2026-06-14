use std::sync::Arc;

use crate::builder::WindowConfig;
use crate::error::WindowError;

/// A native window with an attached [`wgpu::Surface`].
///
/// Both the window and the wgpu surface are kept alive via shared
/// ownership so the surface can carry a `'static` lifetime.
#[derive(Debug)]
pub struct WindowHandle {
    window: Arc<winit::window::Window>,
    surface: wgpu::Surface<'static>,
}

impl WindowHandle {
    /// Create a new window and its wgpu surface from a config.
    pub fn new(
        target: &winit::event_loop::ActiveEventLoop,
        instance: &wgpu::Instance,
        config: &WindowConfig,
    ) -> Result<Self, WindowError> {
        use winit::dpi::LogicalSize;

        let mut winit_attrs = winit::window::Window::default_attributes()
            .with_title(&config.title)
            .with_inner_size(LogicalSize::new(config.width as f64, config.height as f64))
            .with_resizable(config.resizable)
            .with_decorations(false);

        if config.fullscreen {
            winit_attrs = winit_attrs
                .with_fullscreen(Some(winit::window::Fullscreen::Borderless(None)));
        }

        if let (Some(w), Some(h)) = (config.min_width, config.min_height) {
            winit_attrs = winit_attrs.with_min_inner_size(LogicalSize::new(w as f64, h as f64));
        }
        if let (Some(w), Some(h)) = (config.max_width, config.max_height) {
            winit_attrs = winit_attrs.with_max_inner_size(LogicalSize::new(w as f64, h as f64));
        }

        let window = target
            .create_window(winit_attrs)
            .map_err(|e| WindowError::Create(format!("{e}")))?;

        let window = Arc::new(window);

        // wgpu 22 accepts `Arc<dyn HasWindowHandle>` which keeps the
        // window alive for the surface's entire lifetime.
        let surface = instance
            .create_surface(window.clone())
            .map_err(|e| WindowError::SurfaceCreate(format!("{e}")))?;

        Ok(Self { window, surface })
    }

    /// Access the wgpu surface for rendering.
    pub fn surface(&self) -> &wgpu::Surface<'static> {
        &self.surface
    }

    /// Access the underlying winit window.
    pub fn inner(&self) -> &winit::window::Window {
        &self.window
    }

    /// Current logical size of the window.
    pub fn size(&self) -> (u32, u32) {
        let size = self.window.inner_size();
        (size.width, size.height)
    }

    /// Unique platform window identifier.
    pub fn id_raw(&self) -> u64 {
        self.window.id().into()
    }

    // ── Window operations ──────────────────────────────────────

    pub fn resize(&mut self, width: u32, height: u32) {
        let _ = self
            .window
            .request_inner_size(winit::dpi::LogicalSize::new(width as f64, height as f64));
    }

    pub fn set_title(&mut self, title: &str) {
        self.window.set_title(title);
    }

    pub fn minimize(&mut self) {
        self.window.set_minimized(true);
    }

    pub fn maximize(&mut self) {
        self.window.set_maximized(true);
    }

    pub fn restore(&mut self) {
        self.window.set_minimized(false);
        self.window.set_maximized(false);
    }

    pub fn close(&mut self) {
        // winit does not expose a programmatic close; the application
        // should handle CloseRequested from the event loop instead.
    }

    /// Set the window icon from RGBA pixel data.
    pub fn set_icon(&mut self, rgba: Vec<u8>, width: u32, height: u32) {
        if let Ok(icon) = winit::window::Icon::from_rgba(rgba, width, height) {
            self.window.set_window_icon(Some(icon));
        }
    }

    /// Request a redraw.
    pub fn request_redraw(&self) {
        self.window.request_redraw();
    }

    /// Consume the handle and return the window and surface separately.
    ///
    /// The window is reference-counted so it remains alive for the
    /// surface's `'static` lifetime.
    pub fn into_parts(self) -> (Arc<winit::window::Window>, wgpu::Surface<'static>) {
        (self.window, self.surface)
    }

    /// Whether the window has user focus.
    pub fn has_focus(&self) -> bool {
        self.window.has_focus()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_matches_config() {
        let cfg = WindowConfig {
            width: 1024,
            height: 768,
            ..WindowConfig::default()
        };
        assert_eq!(cfg.width, 1024);
        assert_eq!(cfg.height, 768);
    }

    #[test]
    fn default_handle_api_compiles() {
        // Compile-time check that the WindowHandle API is well-typed.
        let _: fn(&mut WindowHandle) = |h| {
            h.set_title("test");
            h.minimize();
            h.maximize();
            h.restore();
            h.close();
        };
    }
}
