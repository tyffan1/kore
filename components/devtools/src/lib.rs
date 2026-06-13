pub mod console;
pub mod elements;
pub mod network;
pub mod state;
pub mod storage;

pub use console::{ConsoleCapture, ConsoleLevel, ConsoleMessage};
pub use elements::DomNode;
pub use network::{NetworkEntry, NetworkLog, RequestMethod};
pub use state::{DevToolsPanel, DevToolsState};
pub use storage::{CookieStub, StorageEntry, StorageInspector};

/// Top-level DevTools container that owns all panel state.
#[derive(Debug, Clone)]
pub struct DevTools {
    pub state: DevToolsState,
    pub console: ConsoleCapture,
    pub network: NetworkLog,
    pub storage: StorageInspector,
}

impl DevTools {
    pub fn new() -> Self {
        Self {
            state: DevToolsState::new(),
            console: ConsoleCapture::new(),
            network: NetworkLog::new(),
            storage: StorageInspector::new(),
        }
    }

    pub fn toggle(&mut self) {
        self.state.toggle();
    }

    pub fn is_visible(&self) -> bool {
        self.state.visible
    }

    pub fn active_panel(&self) -> DevToolsPanel {
        self.state.active_panel
    }

    pub fn switch_panel(&mut self, panel: DevToolsPanel) {
        self.state.switch_panel(panel);
    }
}

impl Default for DevTools {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn devtools_initial_state() {
        let dt = DevTools::new();
        assert!(!dt.is_visible());
        assert_eq!(dt.active_panel(), DevToolsPanel::Console);
        assert!(dt.console.is_empty());
        assert!(dt.network.is_empty());
    }

    #[test]
    fn devtools_captures_console_message() {
        let mut dt = DevTools::new();
        dt.console.push(ConsoleLevel::Log, "hello devtools");
        assert_eq!(dt.console.len(), 1);
        assert_eq!(dt.console.messages()[0].text, "hello devtools");
    }

    #[test]
    fn devtools_captures_network_request() {
        let mut dt = DevTools::new();
        let url = url::Url::parse("https://example.com/api").ok();
        let id = dt.network.begin_request(url.unwrap(), RequestMethod::Get);
        dt.network.complete_request(id, 200);
        assert_eq!(dt.network.len(), 1);
        assert_eq!(dt.network.entries()[0].status, Some(200));
    }

    #[test]
    fn devtools_toggle_and_panel_switch() {
        let mut dt = DevTools::new();
        dt.toggle();
        assert!(dt.is_visible());
        dt.switch_panel(DevToolsPanel::Network);
        assert_eq!(dt.active_panel(), DevToolsPanel::Network);
        dt.toggle();
        assert!(!dt.is_visible());
    }
}
