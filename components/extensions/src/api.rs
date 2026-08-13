use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabInfo {
    pub id: u64,
    pub url: Url,
    pub title: String,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageArea {
    pub area: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebRequestFilter {
    pub urls: Vec<String>,
    pub types: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextMenuEntry {
    pub id: String,
    pub title: String,
    pub contexts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationOptions {
    pub title: String,
    pub message: String,
    pub icon_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookmarkItem {
    pub id: String,
    pub title: String,
    pub url: Option<String>,
    pub children: Vec<BookmarkItem>,
}

/// Stub for the `chrome.tabs` API.
#[derive(Debug, Clone, Default)]
pub struct TabsApi;

impl TabsApi {
    pub fn query(&self, _active: bool) -> Vec<TabInfo> {
        Vec::new()
    }

    pub fn create(&self, _url: &str) -> Option<u64> {
        None
    }

    pub fn remove(&self, _tab_id: u64) -> bool {
        false
    }

    pub fn update(&self, _tab_id: u64, _url: &str) -> bool {
        false
    }
}

/// Stub for the `chrome.storage` API.
#[derive(Debug, Clone, Default)]
pub struct StorageApi;

impl StorageApi {
    pub fn local(&self) -> StorageArea {
        StorageArea {
            area: "local".to_string(),
        }
    }

    pub fn sync(&self) -> StorageArea {
        StorageArea {
            area: "sync".to_string(),
        }
    }
}

/// Which `chrome.webRequest` event a listener is registered for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebRequestEventKind {
    BeforeRequest,
    Completed,
}

/// Details about a single request passing through the web request pipeline.
#[derive(Debug, Clone)]
pub struct WebRequestInfo {
    pub url: String,
    pub request_type: String,
    /// HTTP status, filled in when the request completes.
    pub status: Option<u16>,
}

/// Outcome of dispatching a `beforeRequest` event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebRequestDecision {
    Allow,
    Block,
}

/// Callback invoked for every request matching a listener's filter.
/// Returning `true` from a `BeforeRequest` listener cancels the request.
pub type WebRequestCallback = Arc<dyn Fn(&WebRequestInfo) -> bool + Send + Sync>;

#[derive(Clone)]
pub struct WebRequestListener {
    pub id: u64,
    pub kind: WebRequestEventKind,
    pub filter: WebRequestFilter,
    pub callback: WebRequestCallback,
}

impl std::fmt::Debug for WebRequestApi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebRequestApi")
            .field("listeners", &self.listeners.len())
            .field("completed", &self.completed.len())
            .finish()
    }
}

/// Real `chrome.webRequest` API: a registry of listeners with URL/type
/// filters. The browser dispatches every request through
/// [`WebRequestApi::dispatch_before_request`] / [`WebRequestApi::dispatch_completed`].
#[derive(Clone, Default)]
pub struct WebRequestApi {
    listeners: Vec<WebRequestListener>,
    next_id: u64,
    completed: Vec<WebRequestInfo>,
}

impl WebRequestApi {
    /// Register a `beforeRequest` listener. The returned id can be passed to
    /// [`WebRequestApi::remove_listener`].
    pub fn on_before_request(
        &mut self,
        filter: WebRequestFilter,
        callback: WebRequestCallback,
    ) -> u64 {
        self.add_listener(WebRequestEventKind::BeforeRequest, filter, callback)
    }

    /// Register an `onCompleted` listener.
    pub fn on_completed(
        &mut self,
        filter: WebRequestFilter,
        callback: WebRequestCallback,
    ) -> u64 {
        self.add_listener(WebRequestEventKind::Completed, filter, callback)
    }

    fn add_listener(
        &mut self,
        kind: WebRequestEventKind,
        filter: WebRequestFilter,
        callback: WebRequestCallback,
    ) -> u64 {
        self.next_id += 1;
        self.listeners.push(WebRequestListener {
            id: self.next_id,
            kind,
            filter,
            callback,
        });
        self.next_id
    }

    pub fn remove_listener(&mut self, id: u64) -> bool {
        let before = self.listeners.len();
        self.listeners.retain(|l| l.id != id);
        self.listeners.len() != before
    }

    pub fn listener_count(&self) -> usize {
        self.listeners.len()
    }

    /// Run all `beforeRequest` listeners whose filter matches. Returns
    /// [`WebRequestDecision::Block`] if any listener cancelled the request.
    pub fn dispatch_before_request(&mut self, url: &str, request_type: &str) -> WebRequestDecision {
        let info = WebRequestInfo {
            url: url.to_string(),
            request_type: request_type.to_string(),
            status: None,
        };
        for listener in &self.listeners {
            if listener.kind == WebRequestEventKind::BeforeRequest
                && filter_matches(&listener.filter, url, request_type)
                && (listener.callback)(&info)
            {
                return WebRequestDecision::Block;
            }
        }
        WebRequestDecision::Allow
    }

    /// Run all `onCompleted` listeners whose filter matches and record the
    /// finished request (kept as a bounded log for inspection).
    pub fn dispatch_completed(&mut self, url: &str, request_type: &str, status: u16) {
        let info = WebRequestInfo {
            url: url.to_string(),
            request_type: request_type.to_string(),
            status: Some(status),
        };
        for listener in &self.listeners {
            if listener.kind == WebRequestEventKind::Completed
                && filter_matches(&listener.filter, url, request_type)
            {
                (listener.callback)(&info);
            }
        }
        self.completed.push(info);
        if self.completed.len() > 100 {
            self.completed.drain(0..self.completed.len() - 100);
        }
    }

    /// Requests that finished, oldest first.
    pub fn completed_requests(&self) -> &[WebRequestInfo] {
        &self.completed
    }
}

/// Match a filter's URL patterns and types against a request. An empty
/// `urls` or `types` list matches everything.
pub fn filter_matches(filter: &WebRequestFilter, url: &str, request_type: &str) -> bool {
    let url_ok = filter.urls.is_empty()
        || filter.urls.iter().any(|p| wildcard_match(p, url));
    let type_ok = filter.types.is_empty()
        || filter.types.iter().any(|t| t == request_type);
    url_ok && type_ok
}

/// Classic greedy wildcard match: `*` matches any run of characters,
/// `?` matches a single character.
pub fn wildcard_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    let (mut pi, mut ti) = (0usize, 0usize);
    let (mut star, mut star_ti) = (usize::MAX, 0usize);
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = pi;
            star_ti = ti;
            pi += 1;
        } else if star != usize::MAX {
            pi = star + 1;
            star_ti += 1;
            ti = star_ti;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

/// Real `chrome.contextMenus` API: menus are stored per owner extension, so
/// `remove_all` really clears only that extension's menus.
#[derive(Debug, Clone, Default)]
pub struct ContextMenusApi {
    menus: HashMap<String, Vec<ContextMenuEntry>>,
}

impl ContextMenusApi {
    /// Register a menu item for `owner`. Re-registering the same id replaces
    /// the previous entry.
    pub fn create(&mut self, owner: &str, entry: ContextMenuEntry) -> bool {
        let menus = self.menus.entry(owner.to_string()).or_default();
        if let Some(existing) = menus.iter_mut().find(|m| m.id == entry.id) {
            *existing = entry;
        } else {
            menus.push(entry);
        }
        true
    }

    /// Remove a single menu item owned by `owner`.
    pub fn remove(&mut self, owner: &str, id: &str) -> bool {
        let Some(menus) = self.menus.get_mut(owner) else {
            return false;
        };
        let before = menus.len();
        menus.retain(|m| m.id != id);
        menus.len() != before
    }

    /// `chrome.contextMenus.removeAll`: drop every menu created by `owner`.
    pub fn remove_all(&mut self, owner: &str) {
        self.menus.remove(owner);
    }

    pub fn entries(&self, owner: &str) -> &[ContextMenuEntry] {
        self.menus.get(owner).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// Total number of registered menu items across all extensions.
    pub fn total_count(&self) -> usize {
        self.menus.values().map(|v| v.len()).sum()
    }
}

/// A notification the browser may display.
#[derive(Debug, Clone)]
pub struct NotificationRecord {
    pub id: String,
    pub options: NotificationOptions,
}

/// Real `chrome.notifications` API: created notifications are kept in a
/// registry with generated ids; `clear` removes them.
#[derive(Debug, Clone, Default)]
pub struct NotificationsApi {
    notifications: Vec<NotificationRecord>,
    next_id: u64,
}

impl NotificationsApi {
    /// Create a notification and return its id.
    pub fn create(&mut self, options: NotificationOptions) -> String {
        self.next_id += 1;
        let id = format!("notif-{}", self.next_id);
        self.notifications.push(NotificationRecord { id: id.clone(), options });
        id
    }

    pub fn clear(&mut self, notification_id: &str) -> bool {
        let before = self.notifications.len();
        self.notifications.retain(|n| n.id != notification_id);
        self.notifications.len() != before
    }

    pub fn clear_all(&mut self) {
        self.notifications.clear();
    }

    pub fn active(&self) -> &[NotificationRecord] {
        &self.notifications
    }

    pub fn count(&self) -> usize {
        self.notifications.len()
    }
}

/// Stub for the `chrome.bookmarks` API.
#[derive(Debug, Clone, Default)]
pub struct BookmarksApi;

impl BookmarksApi {
    pub fn search(&self, _query: &str) -> Vec<BookmarkItem> {
        Vec::new()
    }

    pub fn get_tree(&self) -> Vec<BookmarkItem> {
        Vec::new()
    }

    pub fn create(&self, _parent_id: &str, _title: &str, _url: Option<&str>) -> Option<BookmarkItem> {
        None
    }
}

/// Aggregates all extension APIs.
#[derive(Debug, Clone, Default)]
pub struct ExtensionApi {
    pub tabs: TabsApi,
    pub storage: StorageApi,
    pub web_request: WebRequestApi,
    pub context_menus: ContextMenusApi,
    pub notifications: NotificationsApi,
    pub bookmarks: BookmarksApi,
}

impl ExtensionApi {
    pub fn new() -> Self {
        Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filter(urls: &[&str], types: &[&str]) -> WebRequestFilter {
        WebRequestFilter {
            urls: urls.iter().map(|s| s.to_string()).collect(),
            types: types.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn wildcard_matches_urls() {
        assert!(wildcard_match("https://example.com/*", "https://example.com/a/b"));
        assert!(wildcard_match("https://example.com/*", "https://example.com/"));
        assert!(wildcard_match("https://*/*", "https://cdn.x.com/img.png"));
        assert!(!wildcard_match("https://example.com/*", "http://example.com/a"));
        assert!(!wildcard_match("https://example.com/*", "https://other.com/a"));
        assert!(wildcard_match("*", "anything"));
        assert!(wildcard_match("", ""));
    }

    #[test]
    fn filter_matches_urls_and_types() {
        let f = filter(&["https://example.com/*"], &["main_frame"]);
        assert!(filter_matches(&f, "https://example.com/page", "main_frame"));
        assert!(!filter_matches(&f, "https://example.com/page", "image"));
        assert!(!filter_matches(&f, "https://other.com/page", "main_frame"));

        let f = filter(&[], &[]);
        assert!(filter_matches(&f, "https://anything.invalid/x", "script"));

        let f = filter(&["https://example.com/*"], &[]);
        assert!(filter_matches(&f, "https://example.com/a", "sub_frame"));
    }

    #[test]
    fn before_request_blocks_matching_requests() {
        let mut api = WebRequestApi::default();
        api.on_before_request(
            filter(&["https://example.com/*"], &[]),
            Arc::new(|_| true),
        );
        assert_eq!(
            api.dispatch_before_request("https://example.com/x", "main_frame"),
            WebRequestDecision::Block
        );
        assert_eq!(
            api.dispatch_before_request("https://safe.com/x", "main_frame"),
            WebRequestDecision::Allow
        );
    }

    #[test]
    fn before_request_allows_when_no_listener_blocks() {
        let mut api = WebRequestApi::default();
        api.on_before_request(filter(&["https://example.com/*"], &[]), Arc::new(|_| false));
        assert_eq!(
            api.dispatch_before_request("https://example.com/x", "main_frame"),
            WebRequestDecision::Allow
        );
    }

    #[test]
    fn completed_listener_and_log() {
        let mut api = WebRequestApi::default();
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen2 = seen.clone();
        api.on_completed(
            filter(&["https://example.com/*"], &[]),
            Arc::new(move |info| {
                seen2.lock().unwrap().push((info.url.clone(), info.status));
                false
            }),
        );
        api.dispatch_completed("https://example.com/data.json", "xmlhttprequest", 200);
        api.dispatch_completed("https://other.com/x", "main_frame", 404);

        let log = seen.lock().unwrap();
        assert_eq!(log.len(), 1);
        assert_eq!(log[0], ("https://example.com/data.json".to_string(), Some(200)));
        drop(log);

        let completed = api.completed_requests();
        assert_eq!(completed.len(), 2);
        assert_eq!(completed[1].status, Some(404));
    }

    #[test]
    fn remove_listener_stops_dispatch() {
        let mut api = WebRequestApi::default();
        let id = api.on_before_request(filter(&[], &[]), Arc::new(|_| true));
        assert_eq!(api.listener_count(), 1);
        assert!(api.remove_listener(id));
        assert_eq!(api.listener_count(), 0);
        assert!(!api.remove_listener(id));
        assert_eq!(
            api.dispatch_before_request("https://example.com/x", "main_frame"),
            WebRequestDecision::Allow
        );
    }

    #[test]
    fn context_menus_scoped_per_owner() {
        let mut api = ContextMenusApi::default();
        let entry = |id: &str| ContextMenuEntry {
            id: id.to_string(),
            title: id.to_string(),
            contexts: vec!["all".to_string()],
        };
        api.create("ext-a", entry("menu1"));
        api.create("ext-a", entry("menu2"));
        api.create("ext-b", entry("menu1"));
        assert_eq!(api.total_count(), 3);
        assert_eq!(api.entries("ext-a").len(), 2);

        api.remove_all("ext-a");
        assert_eq!(api.total_count(), 1);
        assert!(api.entries("ext-a").is_empty());
        assert_eq!(api.entries("ext-b").len(), 1);

        assert!(api.remove("ext-b", "menu1"));
        assert!(!api.remove("ext-b", "menu1"));
        assert_eq!(api.total_count(), 0);
    }

    #[test]
    fn context_menu_create_replaces_same_id() {
        let mut api = ContextMenusApi::default();
        api.create("ext-a", ContextMenuEntry {
            id: "m".to_string(),
            title: "first".to_string(),
            contexts: Vec::new(),
        });
        api.create("ext-a", ContextMenuEntry {
            id: "m".to_string(),
            title: "second".to_string(),
            contexts: Vec::new(),
        });
        assert_eq!(api.entries("ext-a").len(), 1);
        assert_eq!(api.entries("ext-a")[0].title, "second");
    }

    #[test]
    fn notifications_create_and_clear() {
        let mut api = NotificationsApi::default();
        let id1 = api.create(NotificationOptions {
            title: "Hello".to_string(),
            message: "World".to_string(),
            icon_url: None,
        });
        let id2 = api.create(NotificationOptions {
            title: "Second".to_string(),
            message: "Note".to_string(),
            icon_url: Some("icon.png".to_string()),
        });
        assert_eq!(api.count(), 2);
        assert_ne!(id1, id2);
        assert_eq!(api.active()[0].options.title, "Hello");

        assert!(api.clear(&id1));
        assert_eq!(api.count(), 1);
        assert!(!api.clear(&id1));

        api.clear_all();
        assert_eq!(api.count(), 0);
    }

    #[test]
    fn extension_api_aggregates_real_apis() {
        let mut api = ExtensionApi::new();
        api.web_request
            .on_before_request(filter(&["https://example.com/*"], &[]), Arc::new(|_| true));
        api.context_menus.create("ext-a", ContextMenuEntry {
            id: "m".to_string(),
            title: "t".to_string(),
            contexts: Vec::new(),
        });
        let nid = api
            .notifications
            .create(NotificationOptions {
                title: "t".to_string(),
                message: "m".to_string(),
                icon_url: None,
            });
        assert!(!nid.is_empty());
        assert_eq!(api.web_request.listener_count(), 1);
        assert_eq!(api.context_menus.total_count(), 1);
        assert_eq!(api.notifications.count(), 1);
    }
}