use std::path::PathBuf;

use crate::tab::Tab;

#[derive(Debug)]
pub struct SessionManager {
    session_path: PathBuf,
}

impl SessionManager {
    pub fn new(path: PathBuf) -> Self {
        Self { session_path: path }
    }

    pub fn session_path(&self) -> &std::path::Path {
        &self.session_path
    }

    pub fn save_session(&self, tabs: &[Tab]) -> Result<(), crate::error::BrowserError> {
        let json = serde_json::to_string_pretty(tabs)?;
        std::fs::write(&self.session_path, json)?;
        Ok(())
    }

    pub fn restore_session(&self) -> Result<Vec<Tab>, crate::error::BrowserError> {
        if !self.session_path.exists() {
            return Ok(Vec::new());
        }
        let json = std::fs::read_to_string(&self.session_path)?;
        let tabs: Vec<Tab> = serde_json::from_str(&json)?;
        Ok(tabs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use url::Url;

    fn make_tab(id: u64, url_str: &str) -> Tab {
        Tab::new(id, Url::parse(url_str).expect("valid url"))
    }

    #[test]
    fn roundtrips_tabs_through_json() {
        let tabs = vec![
            make_tab(0, "https://example.com/"),
            make_tab(1, "https://kore.dev/"),
        ];

        let json = serde_json::to_string(&tabs).expect("serialize");
        let restored: Vec<Tab> = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(restored.len(), 2);
        assert_eq!(restored[0].id, 0);
        assert_eq!(restored[0].url.as_str(), "https://example.com/");
        assert_eq!(restored[1].id, 1);
    }

    #[test]
    fn preserves_active_flag() {
        let mut tabs = vec![
            make_tab(0, "https://a.com/"),
            make_tab(1, "https://b.com/"),
        ];
        tabs[1].is_active = true;

        let json = serde_json::to_string(&tabs).expect("serialize");
        let restored: Vec<Tab> = serde_json::from_str(&json).expect("deserialize");

        assert!(!restored[0].is_active);
        assert!(restored[1].is_active);
    }

    #[test]
    fn empty_session_returns_empty_vec() {
        let mgr = SessionManager::new(PathBuf::from(
            std::env::temp_dir().join("kore_test_empty.json"),
        ));
        let tabs = mgr.restore_session().expect("restore empty");
        assert!(tabs.is_empty());
    }

    #[test]
    fn roundtrips_tab_with_title_update() {
        let mut tab = make_tab(0, "https://example.com/");
        tab.title = "Example".to_string();
        tab.process_id = Some(42);

        let json = serde_json::to_string(&tab).expect("serialize");
        let restored: Tab = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(restored.title, "Example");
        assert_eq!(restored.process_id, Some(42));
    }
}
