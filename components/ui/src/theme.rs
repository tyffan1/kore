use serde::{Deserialize, Serialize};

/// Browser colour theme preference.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Theme {
    /// Follow the OS or system-wide setting.
    System,
    /// Force the light palette.
    Light,
    /// Force the dark palette.
    Dark,
}

impl Theme {
    /// Return the CSS class name that activates this theme on `<body>`.
    pub fn css_class(&self) -> &'static str {
        match self {
            Theme::Light => "light-theme",
            Theme::Dark => "dark-theme",
            // System is resolved at runtime in JS; fall back to light.
            Theme::System => "light-theme",
        }
    }

    /// True when the theme is dark.
    pub fn is_dark(&self) -> bool {
        matches!(self, Theme::Dark)
    }

    /// Return the opposite theme (for toggling).
    pub fn toggle(&self) -> Self {
        match self {
            Theme::Light => Theme::Dark,
            Theme::Dark => Theme::Light,
            Theme::System => Theme::Dark,
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Theme::System
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_system() {
        assert_eq!(Theme::default(), Theme::System);
    }

    #[test]
    fn light_css_class() {
        assert_eq!(Theme::Light.css_class(), "light-theme");
    }

    #[test]
    fn dark_css_class() {
        assert_eq!(Theme::Dark.css_class(), "dark-theme");
    }

    #[test]
    fn is_dark_returns_true_for_dark() {
        assert!(Theme::Dark.is_dark());
        assert!(!Theme::Light.is_dark());
        assert!(!Theme::System.is_dark());
    }

    #[test]
    fn toggle_switches_theme() {
        assert_eq!(Theme::Light.toggle(), Theme::Dark);
        assert_eq!(Theme::Dark.toggle(), Theme::Light);
        assert_eq!(Theme::System.toggle(), Theme::Dark);
    }
}
