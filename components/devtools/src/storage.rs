use kore_js::{Cookie, CookieJar, SharedCookieJar, SharedStorage, WebStorage};

/// Display copy of a cookie entry.
#[derive(Debug, Clone)]
pub struct CookieStub {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
}

/// Display copy of a localStorage entry.
#[derive(Debug, Clone)]
pub struct StorageEntry {
    pub key: String,
    pub value: String,
}

/// Storage inspector: a live view over the browser's real `localStorage`
/// and cookie jar (the same shared stores that back `localStorage` and
/// `document.cookie` in the JS engine).
#[derive(Debug, Clone)]
pub struct StorageInspector {
    storage: SharedStorage,
    cookies: SharedCookieJar,
}

impl StorageInspector {
    pub fn new() -> Self {
        Self::from_shared(
            std::sync::Arc::new(std::sync::Mutex::new(WebStorage::default())),
            std::sync::Arc::new(std::sync::Mutex::new(CookieJar::default())),
        )
    }

    /// Attach to the shared stores owned by the render pipeline / session so
    /// the inspector reflects real page data.
    pub fn from_shared(storage: SharedStorage, cookies: SharedCookieJar) -> Self {
        Self { storage, cookies }
    }

    pub fn cookies(&self) -> Vec<CookieStub> {
        let jar = self.cookies.lock().unwrap();
        jar.all()
            .iter()
            .map(|c| CookieStub {
                name: c.name.clone(),
                value: c.value.clone(),
                domain: c.domain.clone(),
                path: c.path.clone(),
            })
            .collect()
    }

    pub fn local_storage(&self) -> Vec<StorageEntry> {
        let s = self.storage.lock().unwrap();
        s.entries()
            .into_iter()
            .map(|(key, value)| StorageEntry { key, value })
            .collect()
    }

    pub fn set_cookie(&mut self, cookie: CookieStub) {
        let mut jar = self.cookies.lock().unwrap();
        jar.set(Cookie {
            name: cookie.name,
            value: cookie.value,
            domain: cookie.domain,
            path: cookie.path,
        });
    }

    pub fn remove_cookie(&mut self, name: &str) {
        self.cookies.lock().unwrap().remove(name, None, None);
    }

    pub fn set_local_storage(&mut self, entry: StorageEntry) {
        self.storage.lock().unwrap().set(entry.key, entry.value);
    }

    pub fn remove_local_storage(&mut self, key: &str) {
        self.storage.lock().unwrap().remove(key);
    }

    pub fn clear_all(&mut self) {
        self.storage.lock().unwrap().clear();
        self.cookies.lock().unwrap().clear();
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

    #[test]
    fn shared_view_reflects_external_writes() {
        let storage = std::sync::Arc::new(std::sync::Mutex::new(WebStorage::default()));
        let cookies = std::sync::Arc::new(std::sync::Mutex::new(CookieJar::default()));
        let mut si = StorageInspector::from_shared(storage.clone(), cookies.clone());

        storage.lock().unwrap().set("k".to_string(), "v".to_string());
        cookies
            .lock()
            .unwrap()
            .set(Cookie {
                name: "sid".to_string(),
                value: "1".to_string(),
                domain: ".x.com".to_string(),
                path: "/".to_string(),
            });

        assert_eq!(si.local_storage().len(), 1);
        assert_eq!(si.local_storage()[0].key, "k");
        assert_eq!(si.cookies().len(), 1);
        assert_eq!(si.cookies()[0].name, "sid");

        si.remove_local_storage("k");
        assert!(storage.lock().unwrap().get("k").is_none());
    }
}
