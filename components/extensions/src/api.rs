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

/// Stub for the `chrome.webRequest` API.
#[derive(Debug, Clone, Default)]
pub struct WebRequestApi;

impl WebRequestApi {
    pub fn on_before_request(&self, _filter: WebRequestFilter) {
        eprintln!("[extensions] webRequest.onBeforeRequest stub");
    }

    pub fn on_completed(&self, _filter: WebRequestFilter) {
        eprintln!("[extensions] webRequest.onCompleted stub");
    }
}

/// Stub for the `chrome.contextMenus` API.
#[derive(Debug, Clone, Default)]
pub struct ContextMenusApi;

impl ContextMenusApi {
    pub fn create(&self, _entry: ContextMenuEntry) -> bool {
        true
    }

    pub fn remove(&self, _id: &str) -> bool {
        true
    }

    pub fn remove_all(&self) {
        eprintln!("[extensions] contextMenus.removeAll stub");
    }
}

/// Stub for the `chrome.notifications` API.
#[derive(Debug, Clone, Default)]
pub struct NotificationsApi;

impl NotificationsApi {
    pub fn create(&self, _options: NotificationOptions) -> Option<String> {
        eprintln!("[extensions] notifications.create stub");
        None
    }

    pub fn clear(&self, _notification_id: &str) -> bool {
        true
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

/// Aggregates all extension API stubs.
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
