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

/// Modern dark-theme colour palette inspired by Arc / Zen Browser.
///
/// All values are (r, g, b) in sRGB 0-255.  Alpha is always 255
/// (fully opaque) unless otherwise noted.
pub struct ModernTheme;

#[allow(non_upper_case_globals)]
impl ModernTheme {
    pub const ToolbarBg: (u8, u8, u8) = (30, 30, 46);    // #1E1E2E
    pub const TabBarBg: (u8, u8, u8) = (24, 24, 37);      // #181825
    pub const TabActiveBg: (u8, u8, u8) = (42, 42, 62);   // #2A2A3E
    pub const TabHoverBg: (u8, u8, u8) = (37, 37, 53);    // #252535
    pub const Accent: (u8, u8, u8) = (137, 180, 250);     // #89B4FA
    pub const TextPrimary: (u8, u8, u8) = (205, 214, 244);// #CDD6F4
    pub const TextSecondary: (u8, u8, u8) = (108, 112, 134);// #6C7086
    pub const AddressBarBg: (u8, u8, u8) = (42, 42, 62);  // #2A2A3E
    pub const AddressBarBorder: (u8, u8, u8) = (69, 71, 90);// #45475A
    pub const BorderSubtle: (u8, u8, u8) = (49, 50, 68);  // #313244
    pub const CloseRed: (u8, u8, u8) = (196, 43, 28);     // #C42B1C
    pub const WinBtnHover: (u8, u8, u8) = (49, 50, 68);   // #313244
    pub const DisabledText: (u8, u8, u8) = (69, 71, 90);  // #45475A
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

    #[test]
    fn modern_theme_matches_expected_colors() {
        assert_eq!(ModernTheme::ToolbarBg, (30, 30, 46));
        assert_eq!(ModernTheme::TabBarBg, (24, 24, 37));
        assert_eq!(ModernTheme::TabActiveBg, (42, 42, 62));
        assert_eq!(ModernTheme::Accent, (137, 180, 250));
    }
}
