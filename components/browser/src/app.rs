use std::path::PathBuf;

use crate::error::BrowserError;
use crate::renderer::RendererProcess;
use crate::session::SessionManager;
use crate::tab::{Tab, TabManager};

#[derive(Debug)]
pub struct BrowserApp {
    pub tab_manager: TabManager,
    pub session: SessionManager,
}

impl BrowserApp {
    pub fn new(session_path: PathBuf) -> Self {
        Self {
            tab_manager: TabManager::new(),
            session: SessionManager::new(session_path),
        }
    }

    pub fn init(&mut self) -> Result<(), BrowserError> {
        let saved_tabs = self.session.restore_session()?;
        self.tab_manager.set_tabs(saved_tabs);
        Ok(())
    }

    pub fn shutdown(&self) -> Result<(), BrowserError> {
        self.session.save_session(self.tab_manager.all_tabs())?;
        Ok(())
    }

    pub fn open_tab(&mut self, url: url::Url) -> Result<(), BrowserError> {
        // Spawn a renderer process for the new tab
        let tab = self.tab_manager.open_tab(url);

        match RendererProcess::spawn(tab.id) {
            Ok(renderer) => {
                tab.process_id = Some(renderer.process_id());
                // In a real browser the RendererProcess handle is stored in a
                // map so it can be kept alive and messaged later.
                let _ = renderer;
            }
            Err(e) => {
                // If the renderer binary doesn't exist yet the tab still
                // works – it just has no attached renderer.
                tab.process_id = None;
                let _ = e;
            }
        }

        Ok(())
    }

    pub fn close_tab(&mut self, id: u64) -> Result<(), BrowserError> {
        self.tab_manager.close_tab(id)
    }

    pub fn switch_tab(&mut self, id: u64) -> Result<(), BrowserError> {
        self.tab_manager.switch_tab(id)
    }

    pub fn list_tabs(&self) -> &[Tab] {
        self.tab_manager.list_tabs()
    }

    pub fn tab_count(&self) -> usize {
        self.tab_manager.tab_count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_url(s: &str) -> url::Url {
        url::Url::parse(s).expect("valid url")
    }

    #[test]
    fn create_and_shutdown() {
        let session_path = std::env::temp_dir().join("kore_test_app_shutdown.json");
        let mut app = BrowserApp::new(session_path.clone());

        app.init().expect("init");
        app.open_tab(test_url("https://example.com/"))
            .expect("open tab");
        app.shutdown().expect("shutdown");

        // Clean up
        let _ = std::fs::remove_file(&session_path);
    }

    #[test]
    fn open_close_switch_tabs() {
        let session_path = std::env::temp_dir().join("kore_test_app_tabs.json");
        let mut app = BrowserApp::new(session_path);

        app.init().expect("init");
        app.open_tab(test_url("https://a.com/"))
            .expect("open tab a");
        app.open_tab(test_url("https://b.com/"))
            .expect("open tab b");
        assert_eq!(app.tab_count(), 2);

        let first_id = app.list_tabs()[0].id;
        app.switch_tab(first_id).expect("switch to first tab");
        app.close_tab(first_id).expect("close first tab");
        assert_eq!(app.tab_count(), 1);

        let _ = std::fs::remove_file(app.session.session_path());
    }

    #[test]
    fn open_tab_increments_count() {
        let session_path = std::env::temp_dir().join("kore_test_app_count.json");
        let mut app = BrowserApp::new(session_path);

        app.init().expect("init");
        app.open_tab(test_url("https://a.com/"))
            .expect("open tab a");
        app.open_tab(test_url("https://b.com/"))
            .expect("open tab b");
        app.open_tab(test_url("https://c.com/"))
            .expect("open tab c");
        assert_eq!(app.tab_count(), 3);

        let _ = std::fs::remove_file(app.session.session_path());
    }

    #[test]
    fn session_persists_across_restarts() {
        let session_path = std::env::temp_dir().join("kore_test_session_persist.json");

        // First session: open tabs and shut down
        {
            let mut app = BrowserApp::new(session_path.clone());
            app.init().expect("init");
            app.open_tab(test_url("https://a.com/"))
                .expect("open tab");
            app.open_tab(test_url("https://b.com/"))
                .expect("open tab");
            app.shutdown().expect("shutdown");
        }

        // Second session: restore and verify
        {
            let mut app = BrowserApp::new(session_path.clone());
            app.init().expect("init");
            assert_eq!(app.tab_count(), 2);
            assert_eq!(
                app.list_tabs()[0].url.as_str(),
                "https://a.com/"
            );
            assert_eq!(
                app.list_tabs()[1].url.as_str(),
                "https://b.com/"
            );
        }

        let _ = std::fs::remove_file(&session_path);
    }
}
