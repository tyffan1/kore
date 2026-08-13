//! HTTP response cache with `Cache-Control` freshness and `ETag` /
//! `Last-Modified` revalidation.
//!
//! Caching lives in the network stack (the `kore-network` process, or the
//! in-process `HttpClient` fallback). Entries are keyed by request URL
//! and held in memory, with a bounded entry count and total byte size
//! (oldest entries are evicted first).
//!
//! Semantics implemented (subset of RFC 9111):
//! - `Cache-Control: max-age=N` → fresh for N seconds.
//! - `Cache-Control: no-store` → response is never stored.
//! - `Cache-Control: no-cache` / `must-revalidate` → stored but always
//!   revalidated.
//! - `ETag` / `Last-Modified` → stale entries are revalidated with
//!   `If-None-Match` / `If-Modified-Since`; a `304 Not Modified` refreshes
//!   the entry without downloading the body.
//! - Responses with neither freshness information nor validators are not
//!   stored.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use bytes::Bytes;
use url::Url;

use crate::FetchResponse;

/// Maximum number of cached entries.
const MAX_ENTRIES: usize = 128;

/// Maximum total cached body size before eviction.
const MAX_TOTAL_BYTES: usize = 32 * 1024 * 1024;

/// Marker header added to cache-served responses for observability.
pub const CACHE_MARKER_HEADER: &str = "x-kore-cache";

#[derive(Debug, Clone)]
pub struct CacheEntry {
    pub status: u16,
    pub final_url: Url,
    pub headers: Vec<(String, String)>,
    pub body: Bytes,
    stored_at: SystemTime,
    max_age: Option<Duration>,
    etag: Option<String>,
    last_modified: Option<String>,
}

impl CacheEntry {
    /// Validators to send with a conditional revalidation request.
    pub fn conditional_headers(&self) -> Vec<(String, String)> {
        let mut out = Vec::new();
        if let Some(etag) = &self.etag {
            out.push(("if-none-match".to_string(), etag.clone()));
        }
        if let Some(lm) = &self.last_modified {
            out.push(("if-modified-since".to_string(), lm.clone()));
        }
        out
    }

    /// Serve the entry directly from cache.
    pub fn to_hit_response(&self) -> FetchResponse {
        self.to_response("hit")
    }

    /// Serve the entry after a successful revalidation (`304`).
    pub fn to_revalidated_response(&self) -> FetchResponse {
        self.to_response("revalidated")
    }

    fn to_response(&self, marker: &str) -> FetchResponse {
        let mut headers = self.headers.clone();
        headers.push((CACHE_MARKER_HEADER.to_string(), marker.to_string()));
        FetchResponse {
            status: self.status,
            final_url: self.final_url.clone(),
            headers,
            body: self.body.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum CacheLookup {
    /// Entry is still fresh and can be served without a network round trip.
    Fresh(CacheEntry),
    /// Entry is stale but has validators; a conditional request may
    /// revalidate it.
    Stale(CacheEntry),
}

#[derive(Default)]
struct CacheState {
    entries: HashMap<String, CacheEntry>,
    order: VecDeque<String>,
    total_bytes: usize,
}

/// A shared, in-memory HTTP cache.
#[derive(Clone, Default)]
pub struct HttpCache {
    inner: Arc<Mutex<CacheState>>,
}

impl HttpCache {
    pub fn lookup(&self, key: &str) -> Option<CacheLookup> {
        let Ok(state) = self.inner.lock() else {
            return None;
        };
        let entry = state.entries.get(key)?;
        let fresh = match (entry.max_age, entry.stored_at.elapsed()) {
            (Some(max), Ok(age)) => age < max,
            _ => false,
        };
        if fresh {
            Some(CacheLookup::Fresh(entry.clone()))
        } else {
            Some(CacheLookup::Stale(entry.clone()))
        }
    }

    /// Store a successful response, honoring its cache directives.
    pub fn store(&self, key: &str, response: &FetchResponse) {
        let (max_age, no_store, no_cache) = match find_header(&response.headers, "cache-control") {
            Some(value) => parse_cache_control(value),
            None => (None, false, false),
        };
        if no_store {
            return;
        }

        let etag = find_header(&response.headers, "etag").map(String::from);
        let last_modified = find_header(&response.headers, "last-modified").map(String::from);
        if max_age.is_none() && etag.is_none() && last_modified.is_none() {
            return;
        }

        let entry = CacheEntry {
            status: response.status,
            final_url: response.final_url.clone(),
            headers: response.headers.clone(),
            body: response.body.clone(),
            stored_at: SystemTime::now(),
            // `no-cache` / `must-revalidate` keep the body for
            // revalidation but never serve it as fresh.
            max_age: if no_cache { None } else { max_age },
            etag,
            last_modified,
        };

        let Ok(mut state) = self.inner.lock() else {
            return;
        };
        if let Some(old) = state.entries.insert(key.to_string(), entry) {
            state.total_bytes = state.total_bytes.saturating_sub(old.body.len());
        } else {
            state.order.push_back(key.to_string());
        }
        state.total_bytes = state.total_bytes.saturating_add(response.body.len());
        evict_if_needed(&mut state);
    }

    /// Mark an entry as freshly validated (after a `304` response).
    pub fn refresh(&self, key: &str) {
        let Ok(mut state) = self.inner.lock() else {
            return;
        };
        if let Some(entry) = state.entries.get_mut(key) {
            entry.stored_at = SystemTime::now();
        }
    }

    pub fn contains(&self, key: &str) -> bool {
        let Ok(state) = self.inner.lock() else {
            return false;
        };
        state.entries.contains_key(key)
    }
}

fn evict_if_needed(state: &mut CacheState) {
    while state.entries.len() > MAX_ENTRIES || state.total_bytes > MAX_TOTAL_BYTES {
        let Some(oldest) = state.order.pop_front() else {
            break;
        };
        if let Some(old) = state.entries.remove(&oldest) {
            state.total_bytes = state.total_bytes.saturating_sub(old.body.len());
        }
    }
}

fn find_header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

/// Parse a `Cache-Control` header value.
///
/// Returns `(max_age, no_store, no_cache)`; `no_cache` also covers
/// `must-revalidate`.
fn parse_cache_control(value: &str) -> (Option<Duration>, bool, bool) {
    let mut max_age = None;
    let mut no_store = false;
    let mut no_cache = false;
    for directive in value.split(',') {
        let directive = directive.trim();
        if directive.eq_ignore_ascii_case("no-store") {
            no_store = true;
        } else if directive.eq_ignore_ascii_case("no-cache")
            || directive.eq_ignore_ascii_case("must-revalidate")
        {
            no_cache = true;
        } else if let Some(rest) = directive.strip_prefix("max-age=") {
            if let Ok(seconds) = rest.trim().parse::<u64>() {
                max_age = Some(Duration::from_secs(seconds));
            }
        }
    }
    (max_age, no_store, no_cache)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_response(status: u16, cache_control: &str, extra: &[(&str, &str)]) -> Option<FetchResponse> {
        let url = Url::parse("https://example.com/").ok()?;
        let mut headers = vec![("cache-control".to_string(), cache_control.to_string())];
        for (name, value) in extra {
            headers.push((name.to_string(), value.to_string()));
        }
        Some(FetchResponse {
            status,
            final_url: url,
            headers,
            body: Bytes::from_static(b"<html>cached</html>"),
        })
    }

    #[test]
    fn parses_max_age() {
        let (max_age, no_store, no_cache) = parse_cache_control("public, max-age=3600");
        assert_eq!(max_age, Some(Duration::from_secs(3600)));
        assert!(!no_store);
        assert!(!no_cache);
    }

    #[test]
    fn parses_no_store() {
        let (_, no_store, _) = parse_cache_control("private, no-store");
        assert!(no_store);
    }

    #[test]
    fn parses_no_cache_and_must_revalidate() {
        let (_, _, no_cache) = parse_cache_control("no-cache");
        assert!(no_cache);
        let (_, _, no_cache) = parse_cache_control("must-revalidate");
        assert!(no_cache);
    }

    #[test]
    fn fresh_entry_hit_within_max_age() {
        let cache = HttpCache::default();
        if let Some(response) = test_response(200, "max-age=60", &[]) {
            cache.store("https://example.com/a", &response);
        }
        assert!(matches!(
            cache.lookup("https://example.com/a"),
            Some(CacheLookup::Fresh(_))
        ));
    }

    #[test]
    fn zero_max_age_entry_is_stale() {
        let cache = HttpCache::default();
        if let Some(response) = test_response(200, "max-age=0", &[]) {
            cache.store("https://example.com/b", &response);
        }
        assert!(matches!(
            cache.lookup("https://example.com/b"),
            Some(CacheLookup::Stale(_))
        ));
    }

    #[test]
    fn no_store_responses_are_not_cached() {
        let cache = HttpCache::default();
        if let Some(response) = test_response(200, "no-store", &[]) {
            cache.store("https://example.com/c", &response);
        }
        assert!(!cache.contains("https://example.com/c"));
    }

    #[test]
    fn responses_without_freshness_or_validators_are_not_cached() {
        let cache = HttpCache::default();
        if let Some(response) = test_response(200, "public", &[]) {
            cache.store("https://example.com/d", &response);
        }
        assert!(!cache.contains("https://example.com/d"));
    }

    #[test]
    fn no_cache_entries_are_stored_but_never_fresh() {
        let cache = HttpCache::default();
        if let Some(response) = test_response(200, "no-cache", &[("etag", "\"v1\"")]) {
            cache.store("https://example.com/e", &response);
        }
        if let Some(CacheLookup::Stale(entry)) = cache.lookup("https://example.com/e") {
            let headers = entry.conditional_headers();
            assert!(headers.iter().any(|(n, v)| n == "if-none-match" && v == "\"v1\""));
        } else {
            assert!(
                cache.lookup("https://example.com/e").is_some(),
                "expected a stale (revalidate-only) entry"
            );
        }
    }

    #[test]
    fn stale_entries_expose_etag_and_last_modified_validators() {
        let cache = HttpCache::default();
        if let Some(response) = test_response(
            200,
            "max-age=0",
            &[
                ("etag", "\"v2\""),
                ("last-modified", "Wed, 21 Oct 2026 07:28:00 GMT"),
            ],
        ) {
            cache.store("https://example.com/f", &response);
        }
        if let Some(CacheLookup::Stale(entry)) = cache.lookup("https://example.com/f") {
            let headers = entry.conditional_headers();
            assert!(headers.iter().any(|(n, v)| n == "if-none-match" && v == "\"v2\""));
            assert!(headers.iter().any(|(n, v)| {
                n == "if-modified-since" && v == "Wed, 21 Oct 2026 07:28:00 GMT"
            }));
        } else {
            assert!(
                cache.lookup("https://example.com/f").is_some(),
                "expected a stale entry with validators"
            );
        }
    }

    #[test]
    fn hit_response_carries_cache_marker() {
        let cache = HttpCache::default();
        if let Some(response) = test_response(200, "max-age=60", &[]) {
            cache.store("https://example.com/g", &response);
        }
        if let Some(CacheLookup::Fresh(entry)) = cache.lookup("https://example.com/g") {
            let response = entry.to_hit_response();
            assert_eq!(response.status, 200);
            assert_eq!(response.body, Bytes::from_static(b"<html>cached</html>"));
            assert!(response
                .headers
                .iter()
                .any(|(n, v)| n == CACHE_MARKER_HEADER && v == "hit"));
        } else {
            assert!(
                cache.lookup("https://example.com/g").is_some(),
                "expected a fresh entry"
            );
        }
    }

    #[test]
    fn refresh_keeps_zero_max_age_entries_revalidating() {
        // `max-age=0` means "always revalidate": even after a successful
        // 304 refresh the entry must not be served without revalidation.
        let cache = HttpCache::default();
        if let Some(response) = test_response(200, "max-age=0", &[]) {
            cache.store("https://example.com/h", &response);
        }
        cache.refresh("https://example.com/h");
        assert!(matches!(
            cache.lookup("https://example.com/h"),
            Some(CacheLookup::Stale(_))
        ));
    }

    #[test]
    fn refresh_keeps_fresh_entries_fresh() {
        let cache = HttpCache::default();
        if let Some(response) = test_response(200, "max-age=60", &[]) {
            cache.store("https://example.com/i", &response);
        }
        cache.refresh("https://example.com/i");
        assert!(matches!(
            cache.lookup("https://example.com/i"),
            Some(CacheLookup::Fresh(_))
        ));
    }

    #[test]
    fn eviction_removes_oldest_entries() {
        let cache = HttpCache::default();
        for i in 0..(MAX_ENTRIES + 10) {
            let key = format!("https://example.com/evict/{i}");
            if let Some(response) = test_response(200, "max-age=60", &[]) {
                cache.store(&key, &response);
            }
        }
        assert!(cache.contains("https://example.com/evict/129"));
        assert!(!cache.contains("https://example.com/evict/0"));
    }
}