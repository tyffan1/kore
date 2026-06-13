use std::time::SystemTime;

use url::Url;

#[derive(Debug, Clone, PartialEq)]
pub enum RequestMethod {
    Get,
    Head,
    Post,
    Put,
    Delete,
    Other(String),
}

impl RequestMethod {
    pub fn name(&self) -> &str {
        match self {
            RequestMethod::Get => "GET",
            RequestMethod::Head => "HEAD",
            RequestMethod::Post => "POST",
            RequestMethod::Put => "PUT",
            RequestMethod::Delete => "DELETE",
            RequestMethod::Other(s) => s.as_str(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct NetworkEntry {
    pub id: u64,
    pub url: Url,
    pub method: RequestMethod,
    pub status: Option<u16>,
    pub start_time: u128,
    pub end_time: Option<u128>,
    pub duration_ms: Option<u128>,
}

impl NetworkEntry {
    pub fn new(id: u64, url: Url, method: RequestMethod) -> Self {
        let start_time = now_millis();
        Self {
            id,
            url,
            method,
            status: None,
            start_time,
            end_time: None,
            duration_ms: None,
        }
    }

    pub fn complete(&mut self, status: u16) {
        let end = now_millis();
        self.status = Some(status);
        self.end_time = Some(end);
        self.duration_ms = Some(end.saturating_sub(self.start_time));
    }
}

fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

/// Captures network request/response entries for the Network panel.
#[derive(Debug, Clone)]
pub struct NetworkLog {
    entries: Vec<NetworkEntry>,
    max_entries: usize,
    next_id: u64,
}

impl NetworkLog {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            max_entries: 500,
            next_id: 1,
        }
    }

    pub fn with_max_entries(max: usize) -> Self {
        Self {
            entries: Vec::new(),
            max_entries: max,
            next_id: 1,
        }
    }

    pub fn begin_request(&mut self, url: Url, method: RequestMethod) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let entry = NetworkEntry::new(id, url, method);
        if self.entries.len() >= self.max_entries {
            self.entries.remove(0);
        }
        self.entries.push(entry);
        id
    }

    pub fn complete_request(&mut self, id: u64, status: u16) {
        if let Some(entry) = self.entries.iter_mut().find(|e| e.id == id) {
            entry.complete(status);
        }
    }

    pub fn entries(&self) -> &[NetworkEntry] {
        &self.entries
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Default for NetworkLog {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_empty() {
        let log = NetworkLog::new();
        assert!(log.is_empty());
    }

    #[test]
    fn begin_request_adds_entry() {
        let mut log = NetworkLog::new();
        let url = Url::parse("https://example.com/").ok();
        let id = log.begin_request(url.unwrap(), RequestMethod::Get);
        assert_eq!(log.len(), 1);
        assert_eq!(id, 1);
    }

    #[test]
    fn complete_request_sets_status_and_duration() {
        let mut log = NetworkLog::new();
        let url = Url::parse("https://example.com/").ok();
        let id = log.begin_request(url.unwrap(), RequestMethod::Get);
        log.complete_request(id, 200);
        let entry = &log.entries()[0];
        assert_eq!(entry.status, Some(200));
        assert!(entry.duration_ms.is_some());
    }

    #[test]
    fn clear_removes_all() {
        let mut log = NetworkLog::new();
        let url = Url::parse("https://example.com/").ok();
        log.begin_request(url.unwrap(), RequestMethod::Get);
        log.clear();
        assert!(log.is_empty());
    }

    #[test]
    fn respects_max_entries() {
        let mut log = NetworkLog::with_max_entries(2);
        for i in 0..4 {
            let url = Url::parse(&format!("https://example.com/{i}")).ok();
            log.begin_request(url.unwrap(), RequestMethod::Get);
        }
        assert_eq!(log.len(), 2);
        assert_eq!(log.entries()[0].url.as_str(), "https://example.com/2");
    }

    #[test]
    fn method_names() {
        assert_eq!(RequestMethod::Get.name(), "GET");
        assert_eq!(RequestMethod::Post.name(), "POST");
    }
}
