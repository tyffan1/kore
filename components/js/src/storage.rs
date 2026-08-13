use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Backing store for `localStorage`: a plain key/value map shared across all
/// pages of the browser session so it survives navigation.
#[derive(Debug, Clone, Default)]
pub struct WebStorage {
    map: HashMap<String, String>,
}

impl WebStorage {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.map.get(key).map(|s| s.as_str())
    }

    pub fn set(&mut self, key: String, value: String) {
        self.map.insert(key, value);
    }

    pub fn remove(&mut self, key: &str) {
        self.map.remove(key);
    }

    pub fn clear(&mut self) {
        self.map.clear();
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// The entry at `index` in arbitrary (implementation-defined) order.
    pub fn key_at(&self, index: usize) -> Option<&String> {
        self.map.keys().nth(index)
    }

    pub fn entries(&self) -> Vec<(String, String)> {
        let mut entries: Vec<(String, String)> = self
            .map
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        entries
    }
}

/// A single cookie.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cookie {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
}

/// A cookie jar shared across the browser session.
#[derive(Debug, Clone, Default)]
pub struct CookieJar {
    cookies: Vec<Cookie>,
}

impl CookieJar {
    pub fn all(&self) -> &[Cookie] {
        &self.cookies
    }

    /// Insert or update a cookie identified by `(name, domain, path)`.
    pub fn set(&mut self, cookie: Cookie) {
        if let Some(existing) = self
            .cookies
            .iter_mut()
            .find(|c| c.name == cookie.name && c.domain == cookie.domain && c.path == cookie.path)
        {
            *existing = cookie;
        } else {
            self.cookies.push(cookie);
        }
    }

    /// Remove every cookie matching `name` (optionally scoped by domain/path).
    pub fn remove(&mut self, name: &str, domain: Option<&str>, path: Option<&str>) {
        self.cookies.retain(|c| {
            c.name != name
                || domain.is_some_and(|d| d != c.domain)
                || path.is_some_and(|p| p != c.path)
        });
    }

    pub fn clear(&mut self) {
        self.cookies.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.cookies.is_empty()
    }
}

pub type SharedStorage = Arc<Mutex<WebStorage>>;
pub type SharedCookieJar = Arc<Mutex<CookieJar>>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_storage_roundtrip() {
        let mut s = WebStorage::default();
        s.set("theme".to_string(), "dark".to_string());
        assert_eq!(s.get("theme"), Some("dark"));
        assert_eq!(s.len(), 1);
        s.set("theme".to_string(), "light".to_string());
        assert_eq!(s.get("theme"), Some("light"));
        s.remove("theme");
        assert_eq!(s.get("theme"), None);
        assert!(s.is_empty());
    }

    #[test]
    fn web_storage_clear_and_key_at() {
        let mut s = WebStorage::default();
        s.set("a".to_string(), "1".to_string());
        s.set("b".to_string(), "2".to_string());
        assert!(s.key_at(0).is_some());
        assert_eq!(s.key_at(99), None);
        s.clear();
        assert!(s.is_empty());
    }

    #[test]
    fn cookie_jar_upsert_by_identity() {
        let mut jar = CookieJar::default();
        jar.set(Cookie {
            name: "session".to_string(),
            value: "abc".to_string(),
            domain: ".example.com".to_string(),
            path: "/".to_string(),
        });
        assert_eq!(jar.all().len(), 1);
        jar.set(Cookie {
            name: "session".to_string(),
            value: "xyz".to_string(),
            domain: ".example.com".to_string(),
            path: "/".to_string(),
        });
        assert_eq!(jar.all().len(), 1);
        assert_eq!(jar.all()[0].value, "xyz");
    }

    #[test]
    fn cookie_jar_remove_scoped() {
        let mut jar = CookieJar::default();
        for (name, domain) in [("a", ".x.com"), ("a", ".y.com"), ("b", ".x.com")] {
            jar.set(Cookie {
                name: name.to_string(),
                value: "1".to_string(),
                domain: domain.to_string(),
                path: "/".to_string(),
            });
        }
        jar.remove("a", None, None);
        assert_eq!(jar.all().len(), 1);
        assert_eq!(jar.all()[0].name, "b");
    }
}
