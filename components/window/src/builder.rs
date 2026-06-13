use serde::{Deserialize, Serialize};

/// Configuration describing how a window should be created.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowConfig {
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub resizable: bool,
    pub fullscreen: bool,
    pub min_width: Option<u32>,
    pub min_height: Option<u32>,
    pub max_width: Option<u32>,
    pub max_height: Option<u32>,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            title: String::from("Kore"),
            width: 1280,
            height: 720,
            resizable: true,
            fullscreen: false,
            min_width: None,
            min_height: None,
            max_width: None,
            max_height: None,
        }
    }
}

/// Builder for [`WindowConfig`].
#[derive(Debug, Clone)]
pub struct WindowBuilder {
    title: String,
    width: u32,
    height: u32,
    resizable: bool,
    fullscreen: bool,
    min_width: Option<u32>,
    min_height: Option<u32>,
    max_width: Option<u32>,
    max_height: Option<u32>,
}

impl Default for WindowBuilder {
    fn default() -> Self {
        Self {
            title: String::from("Kore"),
            width: 1280,
            height: 720,
            resizable: true,
            fullscreen: false,
            min_width: None,
            min_height: None,
            max_width: None,
            max_height: None,
        }
    }
}

impl WindowBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn title(mut self, title: &str) -> Self {
        self.title = title.to_string();
        self
    }

    pub fn size(mut self, width: u32, height: u32) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    pub fn resizable(mut self, resizable: bool) -> Self {
        self.resizable = resizable;
        self
    }

    pub fn fullscreen(mut self, fullscreen: bool) -> Self {
        self.fullscreen = fullscreen;
        self
    }

    pub fn min_size(mut self, width: u32, height: u32) -> Self {
        self.min_width = Some(width);
        self.min_height = Some(height);
        self
    }

    pub fn max_size(mut self, width: u32, height: u32) -> Self {
        self.max_width = Some(width);
        self.max_height = Some(height);
        self
    }

    pub fn build(self) -> WindowConfig {
        WindowConfig {
            title: self.title,
            width: self.width,
            height: self.height,
            resizable: self.resizable,
            fullscreen: self.fullscreen,
            min_width: self.min_width,
            min_height: self.min_height,
            max_width: self.max_width,
            max_height: self.max_height,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_1280x720() {
        let cfg = WindowConfig::default();
        assert_eq!(cfg.title, "Kore");
        assert_eq!(cfg.width, 1280);
        assert_eq!(cfg.height, 720);
        assert!(cfg.resizable);
        assert!(!cfg.fullscreen);
    }

    #[test]
    fn builder_customizes_title_and_size() {
        let cfg = WindowBuilder::new()
            .title("Kore Dev")
            .size(1920, 1080)
            .build();

        assert_eq!(cfg.title, "Kore Dev");
        assert_eq!(cfg.width, 1920);
        assert_eq!(cfg.height, 1080);
    }

    #[test]
    fn builder_disables_resizable() {
        let cfg = WindowBuilder::new().resizable(false).build();
        assert!(!cfg.resizable);
    }

    #[test]
    fn builder_sets_fullscreen() {
        let cfg = WindowBuilder::new().fullscreen(true).build();
        assert!(cfg.fullscreen);
    }

    #[test]
    fn builder_sets_min_and_max_size() {
        let cfg = WindowBuilder::new()
            .min_size(400, 300)
            .max_size(2560, 1440)
            .build();

        assert_eq!(cfg.min_width, Some(400));
        assert_eq!(cfg.min_height, Some(300));
        assert_eq!(cfg.max_width, Some(2560));
        assert_eq!(cfg.max_height, Some(1440));
    }

    #[test]
    fn builder_defaults_are_same_as_config_defaults() {
        let from_builder = WindowBuilder::new().build();
        let from_config = WindowConfig::default();
        assert_eq!(from_builder.title, from_config.title);
        assert_eq!(from_builder.width, from_config.width);
        assert_eq!(from_builder.height, from_config.height);
        assert_eq!(from_builder.resizable, from_config.resizable);
    }
}
