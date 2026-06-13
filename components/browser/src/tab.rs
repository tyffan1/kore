use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tab {
    pub id: u64,
    pub url: Url,
    pub title: String,
    pub is_active: bool,
    pub process_id: Option<u32>,
    #[serde(default)]
    pub history: Vec<Url>,
    #[serde(default)]
    pub history_index: usize,
}

impl Tab {
    pub fn new(id: u64, url: Url) -> Self {
        let title = url.as_str().to_string();
        let mut history = Vec::new();
        history.push(url.clone());
        Self {
            id,
            url,
            title,
            is_active: false,
            process_id: None,
            history,
            history_index: 0,
        }
    }

    pub fn navigate(&mut self, url: Url) {
        if self.history.is_empty() {
            self.history.push(self.url.clone());
            self.history_index = 0;
        }
        self.history.truncate(self.history_index + 1);
        self.history.push(url.clone());
        self.history_index = self.history.len() - 1;
        self.url = url;
    }

    pub fn can_go_back(&self) -> bool {
        self.history_index > 0
    }

    pub fn can_go_forward(&self) -> bool {
        self.history_index + 1 < self.history.len()
    }

    pub fn go_back(&mut self) -> Option<Url> {
        if self.can_go_back() {
            self.history_index -= 1;
            self.url = self.history[self.history_index].clone();
            Some(self.url.clone())
        } else {
            None
        }
    }

    pub fn go_forward(&mut self) -> Option<Url> {
        if self.can_go_forward() {
            self.history_index += 1;
            self.url = self.history[self.history_index].clone();
            Some(self.url.clone())
        } else {
            None
        }
    }
}

#[derive(Debug, Default)]
pub struct TabManager {
    tabs: Vec<Tab>,
    next_id: u64,
    active_id: Option<u64>,
}

impl TabManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn open_tab(&mut self, url: Url) -> &mut Tab {
        let id = self.next_id;
        self.next_id += 1;

        // Deactivate the current active tab
        if let Some(active) = self.active_id {
            if let Some(t) = self.tabs.iter_mut().find(|t| t.id == active) {
                t.is_active = false;
            }
        }

        let mut tab = Tab::new(id, url);
        tab.is_active = true;
        self.active_id = Some(id);
        self.tabs.push(tab);

        self.tabs.last_mut().expect("tab just pushed")
    }

    pub fn close_tab(&mut self, id: u64) -> Result<(), crate::error::BrowserError> {
        let pos = self
            .tabs
            .iter()
            .position(|t| t.id == id)
            .ok_or(crate::error::BrowserError::TabNotFound(id))?;

        self.tabs.remove(pos);

        if self.active_id == Some(id) {
            self.active_id = self.tabs.first().map(|t| t.id);
            if let Some(active) = self.active_id {
                if let Some(t) = self.tabs.iter_mut().find(|t| t.id == active) {
                    t.is_active = true;
                }
            }
        }

        Ok(())
    }

    pub fn switch_tab(&mut self, id: u64) -> Result<(), crate::error::BrowserError> {
        if !self.tabs.iter().any(|t| t.id == id) {
            return Err(crate::error::BrowserError::TabNotFound(id));
        }

        if let Some(active) = self.active_id {
            if let Some(t) = self.tabs.iter_mut().find(|t| t.id == active) {
                t.is_active = false;
            }
        }

        if let Some(t) = self.tabs.iter_mut().find(|t| t.id == id) {
            t.is_active = true;
        }
        self.active_id = Some(id);

        Ok(())
    }

    pub fn list_tabs(&self) -> &[Tab] {
        &self.tabs
    }

    pub fn active_tab(&self) -> Option<&Tab> {
        self.active_id.and_then(|id| self.tabs.iter().find(|t| t.id == id))
    }

    pub fn active_tab_mut(&mut self) -> Option<&mut Tab> {
        let active = self.active_id?;
        self.tabs.iter_mut().find(|t| t.id == active)
    }

    pub fn get(&self, id: u64) -> Option<&Tab> {
        self.tabs.iter().find(|t| t.id == id)
    }

    pub fn get_mut(&mut self, id: u64) -> Option<&mut Tab> {
        self.tabs.iter_mut().find(|t| t.id == id)
    }

    pub fn tab_count(&self) -> usize {
        self.tabs.len()
    }

    pub fn set_tabs(&mut self, tabs: Vec<Tab>) {
        if let Some(max_id) = tabs.iter().map(|t| t.id).max() {
            self.next_id = max_id.saturating_add(1);
        }
        self.tabs = tabs;
        self.active_id = self.tabs.iter().find(|t| t.is_active).map(|t| t.id);
    }

    pub fn all_tabs(&self) -> &[Tab] {
        &self.tabs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_url(s: &str) -> Url {
        Url::parse(s).expect("valid url")
    }

    #[test]
    fn opens_tab_with_incremental_ids() {
        let mut mgr = TabManager::new();
        mgr.open_tab(test_url("https://example.com/"));
        mgr.open_tab(test_url("https://kore.dev/"));
        assert_eq!(mgr.tab_count(), 2);
        assert_eq!(mgr.list_tabs()[0].id, 0);
        assert_eq!(mgr.list_tabs()[1].id, 1);
    }

    #[test]
    fn opening_tab_marks_it_active() {
        let mut mgr = TabManager::new();
        let tab = mgr.open_tab(test_url("https://example.com/"));
        assert!(tab.is_active);
    }

    #[test]
    fn new_tab_deactivates_previous() {
        let mut mgr = TabManager::new();
        mgr.open_tab(test_url("https://a.com/"));
        mgr.open_tab(test_url("https://b.com/"));
        assert!(!mgr.list_tabs()[0].is_active);
        assert!(mgr.list_tabs()[1].is_active);
    }

    #[test]
    fn switch_tab_changes_active() {
        let mut mgr = TabManager::new();
        mgr.open_tab(test_url("https://a.com/"));
        mgr.open_tab(test_url("https://b.com/"));
        mgr.switch_tab(0).expect("switch to tab 0");
        assert!(mgr.list_tabs()[0].is_active);
        assert!(!mgr.list_tabs()[1].is_active);
    }

    #[test]
    fn switch_tab_fails_for_unknown_id() {
        let mut mgr = TabManager::new();
        mgr.open_tab(test_url("https://a.com/"));
        let result = mgr.switch_tab(99);
        assert!(result.is_err());
    }

    #[test]
    fn close_tab_removes_and_activates_next() {
        let mut mgr = TabManager::new();
        mgr.open_tab(test_url("https://a.com/"));
        mgr.open_tab(test_url("https://b.com/"));
        mgr.close_tab(1).expect("close tab 1");
        assert_eq!(mgr.tab_count(), 1);
        assert_eq!(mgr.list_tabs()[0].id, 0);
        assert!(mgr.list_tabs()[0].is_active);
    }

    #[test]
    fn close_tab_fails_for_missing() {
        let mut mgr = TabManager::new();
        let result = mgr.close_tab(0);
        assert!(result.is_err());
    }

    #[test]
    fn active_tab_returns_none_when_empty() {
        let mgr = TabManager::new();
        assert!(mgr.active_tab().is_none());
    }

    #[test]
    fn tab_navigate_updates_history() {
        let mut tab = Tab::new(0, test_url("https://a.com/"));
        tab.navigate(test_url("https://b.com/"));
        tab.navigate(test_url("https://c.com/"));
        assert_eq!(tab.history.len(), 3);
        assert_eq!(tab.history_index, 2);
        assert_eq!(tab.url.as_str(), "https://c.com/");
    }

    #[test]
    fn tab_go_back_forward() {
        let mut tab = Tab::new(0, test_url("https://a.com/"));
        tab.navigate(test_url("https://b.com/"));
        tab.navigate(test_url("https://c.com/"));

        assert!(tab.can_go_back());
        assert!(!tab.can_go_forward());

        let back_url = tab.go_back().expect("go back");
        assert_eq!(back_url.as_str(), "https://b.com/");
        assert_eq!(tab.history_index, 1);
        assert!(tab.can_go_back());
        assert!(tab.can_go_forward());

        let forward_url = tab.go_forward().expect("go forward");
        assert_eq!(forward_url.as_str(), "https://c.com/");
        assert_eq!(tab.history_index, 2);
        assert!(tab.can_go_back());
        assert!(!tab.can_go_forward());
    }

    #[test]
    fn tab_go_back_at_start_returns_none() {
        let mut tab = Tab::new(0, test_url("https://a.com/"));
        assert!(!tab.can_go_back());
        assert!(tab.go_back().is_none());
    }

    #[test]
    fn tab_go_forward_at_end_returns_none() {
        let mut tab = Tab::new(0, test_url("https://a.com/"));
        assert!(!tab.can_go_forward());
        assert!(tab.go_forward().is_none());
    }

    #[test]
    fn tab_navigate_after_go_back_truncates_forward_history() {
        let mut tab = Tab::new(0, test_url("https://a.com/"));
        tab.navigate(test_url("https://b.com/"));
        tab.navigate(test_url("https://c.com/"));
        tab.go_back().expect("back");
        tab.go_back().expect("back");

        tab.navigate(test_url("https://d.com/"));
        assert_eq!(tab.history.len(), 2);
        assert_eq!(tab.history_index, 1);
        assert_eq!(tab.history[0].as_str(), "https://a.com/");
        assert_eq!(tab.history[1].as_str(), "https://d.com/");
    }
}
