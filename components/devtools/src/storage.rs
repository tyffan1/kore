use serde::{Deserialize, Serialize};

/// Stub for a cookie entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CookieStub {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
}

/// Stub for a localStorage entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageEntry {
    pub key: String,
    pub value: String,
}

/// Storage inspector with stub data.
#[derive(Debug, Clone)]
pub struct StorageInspector {
    pub cookies: Vec<CookieStub>,
    pub local_storage: Vec<StorageEntry>,
}

impl StorageInspector {
    pub fn new() -> Self {
        Self {
            cookies: Vec::new(),
            local_storage: Vec::new(),
        }
    }

    pub fn cookies(&self) -> &[CookieStub] {
        &self.cookies
    }

    pub fn local_storage(&self) -> &[StorageEntry] {
        &self.local_storage
    }

    pub fn set_cookie(&mut self, cookie: CookieStub) {
        if let Some(existing) = self.cookies.iter_mut().find(|c| c.name == cookie.name) {
            *existing = cookie;
        } else {
            self.cookies.push(cookie);
        }
    }

    pub fn remove_cookie(&mut self, name: &str) {
        self.cookies.retain(|c| c.name != name);
    }

    pub fn set_local_storage(&mut self, entry: StorageEntry) {
        if let Some(existing) = self.local_storage.iter_mut().find(|e| e.key == entry.key) {
            *existing = entry;
        } else {
            self.local_storage.push(entry);
        }
    }

    pub fn remove_local_storage(&mut self, key: &str) {
        self.local_storage.retain(|e| e.key != key);
    }

    pub fn clear_all(&mut self) {
        self.cookies.clear();
        self.local_storage.clear();
    }
}

impl Default for StorageInspector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_empty() {
        let si = StorageInspector::new();
        assert!(si.cookies().is_empty());
        assert!(si.local_storage().is_empty());
    }

    #[test]
    fn set_cookie_adds_or_updates() {
        let mut si = StorageInspector::new();
        si.set_cookie(CookieStub {
            name: "session".to_string(),
            value: "abc".to_string(),
            domain: ".example.com".to_string(),
            path: "/".to_string(),
        });
        assert_eq!(si.cookies().len(), 1);
        si.set_cookie(CookieStub {
            name: "session".to_string(),
            value: "xyz".to_string(),
            domain: ".example.com".to_string(),
            path: "/".to_string(),
        });
        assert_eq!(si.cookies().len(), 1);
        assert_eq!(si.cookies()[0].value, "xyz");
    }

    #[test]
    fn remove_cookie_deletes() {
        let mut si = StorageInspector::new();
        si.set_cookie(CookieStub {
            name: "test".to_string(),
            value: "1".to_string(),
            domain: ".x.com".to_string(),
            path: "/".to_string(),
        });
        si.remove_cookie("test");
        assert!(si.cookies().is_empty());
    }

    #[test]
    fn set_local_storage_adds_or_updates() {
        let mut si = StorageInspector::new();
        si.set_local_storage(StorageEntry {
            key: "theme".to_string(),
            value: "dark".to_string(),
        });
        assert_eq!(si.local_storage().len(), 1);
        si.set_local_storage(StorageEntry {
            key: "theme".to_string(),
            value: "light".to_string(),
        });
        assert_eq!(si.local_storage().len(), 1);
        assert_eq!(si.local_storage()[0].value, "light");
    }

    #[test]
    fn remove_local_storage_deletes() {
        let mut si = StorageInspector::new();
        si.set_local_storage(StorageEntry {
            key: "key1".to_string(),
            value: "val1".to_string(),
        });
        si.remove_local_storage("key1");
        assert!(si.local_storage().is_empty());
    }

    #[test]
    fn clear_all_empties_everything() {
        let mut si = StorageInspector::new();
        si.set_cookie(CookieStub {
            name: "c".to_string(),
            value: "1".to_string(),
            domain: ".x.com".to_string(),
            path: "/".to_string(),
        });
        si.set_local_storage(StorageEntry {
            key: "k".to_string(),
            value: "v".to_string(),
        });
        si.clear_all();
        assert!(si.cookies().is_empty());
        assert!(si.local_storage().is_empty());
    }
}
