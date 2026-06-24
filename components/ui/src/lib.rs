//! Front-end UI resource embedding and theme helpers for Kore.

pub mod resources;
pub mod theme;

pub use resources::{settings_html, tabs_html, toolbar_html, NEWTAB_HTML};
pub use theme::{ModernTheme, Theme};

/// Platform-specific style for window control buttons (close/minimize/maximize).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowControlsStyle {
    /// macOS traffic-light buttons on the left.
    MacOS,
    /// Windows-style buttons on the right.
    Windows,
    /// Linux (same layout as Windows by default).
    Linux,
}

impl WindowControlsStyle {
    /// Detect the current platform's window-control style at compile time.
    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            WindowControlsStyle::MacOS
        } else if cfg!(target_os = "windows") {
            WindowControlsStyle::Windows
        } else {
            WindowControlsStyle::Linux
        }
    }
}
