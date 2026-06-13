use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tab {
    pub id: u64,
    pub url: Url,
    pub title: String,
    pub is_active: bool,
    pub process_id: Option<u32>,
}

impl Tab {
    pub fn new(id: u64, url: Url) -> Self {
        let title = url.as_str().to_string();
        Self {
            id,
            url,
            title,
            is_active: false,
            process_id: None,
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
}
