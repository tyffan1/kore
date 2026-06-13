/// Returns the toolbar HTML for embedding in the browser chrome.
pub fn toolbar_html() -> &'static str {
    include_str!("../../../ui/toolbar.html")
}

/// Returns the tab strip HTML template.
pub fn tabs_html() -> &'static str {
    include_str!("../../../ui/tabs.html")
}

/// Returns the new tab page HTML.
pub const NEWTAB_HTML: &str = include_str!("../../../ui/newtab.html");

/// Returns the settings page HTML.
pub fn settings_html() -> &'static str {
    include_str!("../../../ui/settings.html")
}

/// Returns the theme CSS.
pub fn theme_css() -> &'static str {
    include_str!("../../../ui/theme.css")
}

/// Returns the omnibox JavaScript.
pub fn omnibox_js() -> &'static str {
    include_str!("../../../ui/omnibox.js")
}

/// Returns the tabs JavaScript.
pub fn tabs_js() -> &'static str {
    include_str!("../../../ui/tabs.js")
}
