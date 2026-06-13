use std::collections::HashMap;
use std::path::Path;

use crate::api::ExtensionApi;
use crate::extension::{Extension, ExtensionError, ExtensionId};

/// Manages loading, enabling, disabling, and listing extensions.
pub struct ExtensionManager {
    extensions: HashMap<ExtensionId, Extension>,
    api: ExtensionApi,
}

impl ExtensionManager {
    pub fn new() -> Self {
        Self {
            extensions: HashMap::new(),
            api: ExtensionApi::new(),
        }
    }

    pub fn load_extension(&mut self, path: &Path) -> Result<ExtensionId, ExtensionError> {
        let ext = Extension::load(path.to_path_buf())?;
        let id = ext.id.clone();
        self.extensions.insert(id.clone(), ext);
        Ok(id)
    }

    pub fn list_extensions(&self) -> Vec<&Extension> {
        self.extensions.values().collect()
    }

    pub fn get(&self, id: &str) -> Option<&Extension> {
        self.extensions.get(id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut Extension> {
        self.extensions.get_mut(id)
    }

    pub fn enable(&mut self, id: &str) -> Result<(), ExtensionError> {
        let ext = self.extensions.get_mut(id).ok_or_else(|| {
            ExtensionError::NotFound(id.to_string())
        })?;
        ext.enabled = true;
        Ok(())
    }

    pub fn disable(&mut self, id: &str) -> Result<(), ExtensionError> {
        let ext = self.extensions.get_mut(id).ok_or_else(|| {
            ExtensionError::NotFound(id.to_string())
        })?;
        ext.enabled = false;
        Ok(())
    }

    pub fn api(&self) -> &ExtensionApi {
        &self.api
    }

    pub fn count(&self) -> usize {
        self.extensions.len()
    }

    pub fn is_loaded(&self, id: &str) -> bool {
        self.extensions.contains_key(id)
    }
}

impl Default for ExtensionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn test_manifest_path() -> PathBuf {
        let dir = std::env::temp_dir().join("kore_ext_test");
        let _ = std::fs::create_dir_all(&dir);
        let manifest = r#"{
            "manifest_version": 2,
            "name": "Test Ext",
            "version": "1.0",
            "permissions": ["tabs", "storage"]
        }"#;
        let _ = std::fs::write(dir.join("manifest.json"), manifest);
        dir
    }

    #[test]
    fn load_extension_adds_to_list() -> Result<(), ExtensionError> {
        let dir = test_manifest_path();
        let mut mgr = ExtensionManager::new();
        let id = mgr.load_extension(&dir)?;
        assert_eq!(mgr.count(), 1);
        assert!(mgr.is_loaded(&id));
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn list_returns_loaded_extensions() -> Result<(), ExtensionError> {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("kore_ext_test_list_{ts}"));
        let manifest = br#"{
            "manifest_version": 2,
            "name": "Test Ext",
            "version": "1.0",
            "permissions": ["tabs", "storage"]
        }"#;
        std::fs::create_dir_all(&dir).map_err(|e| ExtensionError::Io {
            path: dir.clone(),
            source: e,
        })?;
        std::fs::write(dir.join("manifest.json"), manifest).map_err(|e| ExtensionError::Io {
            path: dir.join("manifest.json"),
            source: e,
        })?;
        let mut mgr = ExtensionManager::new();
        mgr.load_extension(&dir)?;
        let list = mgr.list_extensions();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name(), "Test Ext");
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn enable_disable_toggles_state() -> Result<(), ExtensionError> {
        let dir = test_manifest_path();
        let mut mgr = ExtensionManager::new();
        let id = mgr.load_extension(&dir)?;
        assert!(mgr.get(&id).ok_or(ExtensionError::NotFound(id.clone()))?.enabled);
        mgr.disable(&id)?;
        assert!(!mgr.get(&id).ok_or(ExtensionError::NotFound(id))?.enabled);
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn disable_unknown_id_returns_error() {
        let mut mgr = ExtensionManager::new();
        let result = mgr.disable("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn api_stubs_are_accessible() {
        let mgr = ExtensionManager::new();
        let api = mgr.api();
        assert!(api.tabs.query(true).is_empty());
        assert!(api.bookmarks.search("test").is_empty());
    }
}
