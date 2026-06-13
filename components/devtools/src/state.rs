use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DevToolsPanel {
    Elements,
    Console,
    Network,
    Storage,
}

impl DevToolsPanel {
    pub fn name(&self) -> &'static str {
        match self {
            DevToolsPanel::Elements => "Elements",
            DevToolsPanel::Console => "Console",
            DevToolsPanel::Network => "Network",
            DevToolsPanel::Storage => "Storage",
        }
    }

    pub fn all() -> &'static [DevToolsPanel] {
        &[
            DevToolsPanel::Elements,
            DevToolsPanel::Console,
            DevToolsPanel::Network,
            DevToolsPanel::Storage,
        ]
    }
}

#[derive(Debug, Clone)]
pub struct DevToolsState {
    pub visible: bool,
    pub active_panel: DevToolsPanel,
    pub panel_width: f32,
}

impl DevToolsState {
    pub fn new() -> Self {
        Self {
            visible: false,
            active_panel: DevToolsPanel::Console,
            panel_width: 400.0,
        }
    }

    pub fn toggle(&mut self) {
        self.visible = !self.visible;
    }

    pub fn show(&mut self) {
        self.visible = true;
    }

    pub fn hide(&mut self) {
        self.visible = false;
    }

    pub fn switch_panel(&mut self, panel: DevToolsPanel) {
        self.active_panel = panel;
    }
}

impl Default for DevToolsState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_hidden() {
        let state = DevToolsState::new();
        assert!(!state.visible);
    }

    #[test]
    fn toggle_switches_visibility() {
        let mut state = DevToolsState::new();
        state.toggle();
        assert!(state.visible);
        state.toggle();
        assert!(!state.visible);
    }

    #[test]
    fn show_and_hide() {
        let mut state = DevToolsState::new();
        state.show();
        assert!(state.visible);
        state.hide();
        assert!(!state.visible);
    }

    #[test]
    fn default_panel_is_console() {
        let state = DevToolsState::new();
        assert_eq!(state.active_panel, DevToolsPanel::Console);
    }

    #[test]
    fn switch_panel_changes_active() {
        let mut state = DevToolsState::new();
        state.switch_panel(DevToolsPanel::Network);
        assert_eq!(state.active_panel, DevToolsPanel::Network);
        state.switch_panel(DevToolsPanel::Elements);
        assert_eq!(state.active_panel, DevToolsPanel::Elements);
    }

    #[test]
    fn panel_name_returns_label() {
        assert_eq!(DevToolsPanel::Elements.name(), "Elements");
        assert_eq!(DevToolsPanel::Console.name(), "Console");
        assert_eq!(DevToolsPanel::Network.name(), "Network");
        assert_eq!(DevToolsPanel::Storage.name(), "Storage");
    }

    #[test]
    fn all_returns_four_panels() {
        assert_eq!(DevToolsPanel::all().len(), 4);
    }
}
