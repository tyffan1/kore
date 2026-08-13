//! Enhanced Tracking Protection (ETP): block known trackers and
//! third-party cookies, mirroring the Standard/Strict modes of desktop
//! browsers.
//!
//! The built-in tracker list is a small curated subset of well-known
//! advertising, analytics, social and fingerprinting domains. A full
//! deployment would ship a Disconnect-style list; this module keeps the
//! structure (category, level gating, shared block log) without the
//! list-size baggage.

use std::sync::{Arc, Mutex};
use url::Url;

/// What kind of tracker a domain is, matching the categories browsers
/// expose in their privacy reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackerCategory {
    Ads,
    Analytics,
    Social,
    Fingerprinting,
}

/// Why a request or cookie was blocked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockReason {
    /// The host is on the tracker list.
    TrackerDomain,
    /// The request is third-party and cookie access is blocked.
    ThirdPartyCookie,
}

/// Outcome of a tracking check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackingDecision {
    Allow,
    Block(BlockReason),
}

/// Cookie handling decision for a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CookieDecision {
    Allow,
    Block,
}

/// ETP strictness. Standard blocks ads/analytics/social trackers; Strict
/// additionally blocks fingerprinting domains.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EtpLevel {
    #[default]
    Standard,
    Strict,
}

/// One blocked-request log entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockedRequest {
    pub url: String,
    pub reason: BlockReason,
    pub category: Option<TrackerCategory>,
    pub top_level: Option<String>,
}

/// Built-in tracker list: registrable domain → category.
const TRACKER_LIST: &[(&str, TrackerCategory)] = &[
    // Advertising
    ("doubleclick.net", TrackerCategory::Ads),
    ("googlesyndication.com", TrackerCategory::Ads),
    ("googleadservices.com", TrackerCategory::Ads),
    ("adnxs.com", TrackerCategory::Ads),
    ("adform.net", TrackerCategory::Ads),
    ("criteo.com", TrackerCategory::Ads),
    ("criteo.net", TrackerCategory::Ads),
    ("taboola.com", TrackerCategory::Ads),
    ("outbrain.com", TrackerCategory::Ads),
    ("rubiconproject.com", TrackerCategory::Ads),
    ("openx.net", TrackerCategory::Ads),
    ("pubmatic.com", TrackerCategory::Ads),
    ("moatads.com", TrackerCategory::Ads),
    ("amazon-adsystem.com", TrackerCategory::Ads),
    ("adroll.com", TrackerCategory::Ads),
    ("quantserve.com", TrackerCategory::Ads),
    ("bidswitch.net", TrackerCategory::Ads),
    ("adsrvr.org", TrackerCategory::Ads),
    ("smartadserver.com", TrackerCategory::Ads),
    ("adcash.com", TrackerCategory::Ads),
    ("mgid.com", TrackerCategory::Ads),
    ("revcontent.com", TrackerCategory::Ads),
    ("bluekai.com", TrackerCategory::Ads),
    ("scorecardresearch.com", TrackerCategory::Ads),
    // Analytics
    ("google-analytics.com", TrackerCategory::Analytics),
    ("googletagmanager.com", TrackerCategory::Analytics),
    ("hotjar.com", TrackerCategory::Analytics),
    ("mixpanel.com", TrackerCategory::Analytics),
    ("amplitude.com", TrackerCategory::Analytics),
    ("segment.io", TrackerCategory::Analytics),
    ("segment.com", TrackerCategory::Analytics),
    ("fullstory.com", TrackerCategory::Analytics),
    ("clarity.ms", TrackerCategory::Analytics),
    ("mouseflow.com", TrackerCategory::Analytics),
    ("crazyegg.com", TrackerCategory::Analytics),
    ("luckyorange.com", TrackerCategory::Analytics),
    ("chartbeat.com", TrackerCategory::Analytics),
    ("newrelic.com", TrackerCategory::Analytics),
    ("logrocket.com", TrackerCategory::Analytics),
    ("snowplowanalytics.com", TrackerCategory::Analytics),
    ("heap.io", TrackerCategory::Analytics),
    ("mc.yandex.ru", TrackerCategory::Analytics),
    // Social
    ("facebook.com", TrackerCategory::Social),
    ("facebook.net", TrackerCategory::Social),
    ("fbcdn.net", TrackerCategory::Social),
    ("instagram.com", TrackerCategory::Social),
    ("twitter.com", TrackerCategory::Social),
    ("twimg.com", TrackerCategory::Social),
    ("t.co", TrackerCategory::Social),
    ("linkedin.com", TrackerCategory::Social),
    ("pinterest.com", TrackerCategory::Social),
    ("vk.com", TrackerCategory::Social),
    ("vk.me", TrackerCategory::Social),
    ("ok.ru", TrackerCategory::Social),
    ("tiktok.com", TrackerCategory::Social),
    ("tiktokcdn.com", TrackerCategory::Social),
    ("snapchat.com", TrackerCategory::Social),
    // Fingerprinting
    ("fingerprintjs.com", TrackerCategory::Fingerprinting),
    ("fingerprint.com", TrackerCategory::Fingerprinting),
    ("seon.io", TrackerCategory::Fingerprinting),
];

/// Reduce a host to its registrable domain (eTLD+1) without a public
/// suffix list: handles the common two-part suffixes (`co.uk`, `com.au`,
/// …) and falls back to the last two labels everywhere else.
pub fn registrable_domain(host: &str) -> String {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    let labels: Vec<&str> = host.split('.').collect();
    if labels.len() <= 2 {
        return host;
    }
    let tld = labels[labels.len() - 1];
    let second = labels[labels.len() - 2];
    if is_second_level_suffix(tld, second) {
        labels[labels.len() - 3..].join(".")
    } else {
        labels[labels.len() - 2..].join(".")
    }
}

fn is_second_level_suffix(tld: &str, second: &str) -> bool {
    matches!(
        (tld, second),
        ("uk", "co" | "org" | "ac" | "gov" | "me")
            | ("au", "com" | "net" | "org" | "edu" | "gov")
            | ("nz", "co" | "net" | "org" | "ac" | "govt")
            | ("jp", "co" | "ne" | "or" | "ac" | "go")
            | ("br", "com" | "net" | "org" | "gov")
            | ("mx", "com" | "net" | "org")
            | ("ar", "com" | "net" | "org" | "gob")
            | ("za", "co" | "net" | "org" | "ac" | "gov")
            | ("tr", "com" | "net" | "org")
            | ("kr", "co" | "ne" | "or" | "go")
            | ("tw", "com" | "net" | "org")
            | ("hk", "com" | "net" | "org")
            | ("sg", "com" | "net" | "org")
            | ("in", "co" | "net" | "org" | "gov" | "ac")
            | ("cn", "com" | "net" | "org" | "gov" | "ac")
            | ("ru", "com" | "net" | "org")
            | ("ua", "com" | "net" | "org")
            | ("pl", "com" | "net" | "org")
            | ("de", "co")
    )
}

/// Whether `request_host` belongs to a different site than `top_level`.
pub fn is_third_party(request_host: &str, top_level: &str) -> bool {
    registrable_domain(request_host) != registrable_domain(top_level)
}

/// Decide whether a request may send cookies / store `Set-Cookie`.
///
/// Main-frame requests (`top_level: None`) are always allowed. Third-party
/// requests are blocked when `block_third_party` is enabled — the ETP
/// default.
pub fn cookie_policy(
    top_level: Option<&str>,
    request_host: &str,
    block_third_party: bool,
) -> CookieDecision {
    match top_level {
        Some(site) if block_third_party && is_third_party(request_host, site) => {
            CookieDecision::Block
        }
        _ => CookieDecision::Allow,
    }
}

/// Whether `host` matches `domain` or is a subdomain of it, without
/// overmatching lookalikes (`fakegoogle-analytics.com` stays allowed).
fn host_matches(host: &str, domain: &str) -> bool {
    let host = host.to_ascii_lowercase();
    let domain = domain.to_ascii_lowercase();
    host == domain
        || host.ends_with(&format!(".{domain}"))
}

/// The category of a host per the built-in list, if any.
pub fn tracker_category(host: &str) -> Option<TrackerCategory> {
    TRACKER_LIST
        .iter()
        .find(|(domain, _)| host_matches(host, domain))
        .map(|(_, category)| *category)
}

/// Shared, configurable tracking protection with a block log. Cloneable —
/// the pipeline keeps one and hands it to DevTools-style observers.
#[derive(Clone, Default)]
pub struct TrackingProtection {
    inner: Arc<Mutex<TrackingProtectionState>>,
}

#[derive(Default)]
struct TrackingProtectionState {
    enabled: bool,
    level: EtpLevel,
    blocked: Vec<BlockedRequest>,
}

impl TrackingProtection {
    /// Create an enabled instance at the Standard level.
    pub fn new() -> Self {
        let protection = Self::default();
        protection.set_enabled(true);
        protection
    }

    pub fn set_enabled(&self, enabled: bool) {
        if let Ok(mut state) = self.inner.lock() {
            state.enabled = enabled;
        }
    }

    pub fn enabled(&self) -> bool {
        self.inner.lock().map(|s| s.enabled).unwrap_or(false)
    }

    pub fn set_level(&self, level: EtpLevel) {
        if let Ok(mut state) = self.inner.lock() {
            state.level = level;
        }
    }

    pub fn level(&self) -> EtpLevel {
        self.inner.lock().map(|s| s.level).unwrap_or_default()
    }

    /// Check a subresource URL against the tracker list, logging any block.
    /// `top_level` is the registrable domain of the top document.
    pub fn check(&self, url: &Url, top_level: Option<&str>) -> TrackingDecision {
        let Some(host) = url.host_str() else {
            return TrackingDecision::Allow;
        };
        let Ok(mut state) = self.inner.lock() else {
            return TrackingDecision::Allow;
        };
        if !state.enabled {
            return TrackingDecision::Allow;
        }
        let category = tracker_category(host);
        let blocked = match category {
            Some(TrackerCategory::Fingerprinting) if state.level == EtpLevel::Standard => false,
            Some(_) => true,
            None => false,
        };
        if !blocked {
            return TrackingDecision::Allow;
        }
        let entry = BlockedRequest {
            url: url.as_str().to_string(),
            reason: BlockReason::TrackerDomain,
            category,
            top_level: top_level.map(|t| t.to_string()),
        };
        state.blocked.push(entry);
        if state.blocked.len() > 200 {
            let excess = state.blocked.len() - 200;
            state.blocked.drain(0..excess);
        }
        TrackingDecision::Block(BlockReason::TrackerDomain)
    }

    /// Every blocked request so far, oldest first.
    pub fn blocked(&self) -> Vec<BlockedRequest> {
        self.inner.lock().map(|s| s.blocked.clone()).unwrap_or_default()
    }

    pub fn blocked_count(&self) -> usize {
        self.inner.lock().map(|s| s.blocked.len()).unwrap_or(0)
    }

    pub fn clear_blocked(&self) {
        if let Ok(mut state) = self.inner.lock() {
            state.blocked.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registrable_domain_handles_simple_hosts() {
        assert_eq!(registrable_domain("example.com"), "example.com");
        assert_eq!(registrable_domain("www.example.com"), "example.com");
        assert_eq!(registrable_domain("a.b.c.example.com"), "example.com");
        assert_eq!(registrable_domain("example.com."), "example.com");
        assert_eq!(registrable_domain("LOCALHOST"), "localhost");
    }

    #[test]
    fn registrable_domain_handles_two_part_suffixes() {
        assert_eq!(registrable_domain("shop.example.co.uk"), "example.co.uk");
        assert_eq!(registrable_domain("example.co.uk"), "example.co.uk");
        assert_eq!(registrable_domain("mail.example.com.au"), "example.com.au");
        assert_eq!(registrable_domain("app.example.co.jp"), "example.co.jp");
        assert_eq!(registrable_domain("blog.example.com.br"), "example.com.br");
    }

    #[test]
    fn is_third_party_compares_sites() {
        assert!(is_third_party("ads.example.net", "example.com"));
        assert!(is_third_party("example.org", "example.com"));
        assert!(!is_third_party("www.example.com", "example.com"));
        assert!(!is_third_party("example.com", "example.com"));
        assert!(!is_third_party("a.example.co.uk", "b.example.co.uk"));
    }

    #[test]
    fn cookie_policy_blocks_third_party_only_when_enabled() {
        assert_eq!(
            cookie_policy(Some("example.com"), "ads.example.net", true),
            CookieDecision::Block
        );
        assert_eq!(
            cookie_policy(Some("example.com"), "www.example.com", true),
            CookieDecision::Allow
        );
        assert_eq!(
            cookie_policy(Some("example.com"), "ads.example.net", false),
            CookieDecision::Allow
        );
        assert_eq!(
            cookie_policy(None, "anywhere.com", true),
            CookieDecision::Allow
        );
    }

    #[test]
    fn tracker_category_matches_subdomains_not_lookalikes() {
        assert_eq!(
            tracker_category("google-analytics.com"),
            Some(TrackerCategory::Analytics)
        );
        assert_eq!(
            tracker_category("ssl.google-analytics.com"),
            Some(TrackerCategory::Analytics)
        );
        assert_eq!(tracker_category("www.doubleclick.net"), Some(TrackerCategory::Ads));
        assert_eq!(tracker_category("fakegoogle-analytics.com"), None);
        assert_eq!(tracker_category("example.com"), None);
        assert_eq!(tracker_category("myinstagram.com"), None);
    }

    #[test]
    fn standard_level_blocks_ads_but_not_fingerprinting() {
        let tp = TrackingProtection::new();
        let ads = Url::parse("https://cdn.doubleclick.net/ad.js").unwrap();
        let fp = Url::parse("https://fpcdn.fingerprintjs.com/api.js").unwrap();
        assert_eq!(tp.check(&ads, Some("example.com")), TrackingDecision::Block(BlockReason::TrackerDomain));
        assert_eq!(tp.check(&fp, Some("example.com")), TrackingDecision::Allow);
    }

    #[test]
    fn strict_level_blocks_fingerprinting() {
        let tp = TrackingProtection::new();
        tp.set_level(EtpLevel::Strict);
        let fp = Url::parse("https://fingerprintjs.com/api.js").unwrap();
        assert_eq!(tp.check(&fp, Some("example.com")), TrackingDecision::Block(BlockReason::TrackerDomain));
    }

    #[test]
    fn disabled_protection_allows_everything_and_logs_nothing() {
        let tp = TrackingProtection::new();
        tp.set_enabled(false);
        let url = Url::parse("https://google-analytics.com/ga.js").unwrap();
        assert_eq!(tp.check(&url, Some("example.com")), TrackingDecision::Allow);
        assert_eq!(tp.blocked_count(), 0);
        assert!(!tp.enabled());
    }

    #[test]
    fn blocked_log_records_url_reason_and_context() {
        let tp = TrackingProtection::new();
        let url = Url::parse("https://analytics.twitter.com/t.js").unwrap();
        assert_eq!(tp.check(&url, Some("example.com")), TrackingDecision::Block(BlockReason::TrackerDomain));
        let blocked = tp.blocked();
        assert_eq!(blocked.len(), 1);
        assert_eq!(blocked[0].url, "https://analytics.twitter.com/t.js");
        assert_eq!(blocked[0].reason, BlockReason::TrackerDomain);
        assert_eq!(blocked[0].category, Some(TrackerCategory::Social));
        assert_eq!(blocked[0].top_level.as_deref(), Some("example.com"));
        assert_eq!(tp.blocked_count(), 1);
        tp.clear_blocked();
        assert_eq!(tp.blocked_count(), 0);
    }

    #[test]
    fn same_site_requests_are_not_blocked() {
        let tp = TrackingProtection::new();
        let url = Url::parse("https://example.com/assets/app.js").unwrap();
        assert_eq!(tp.check(&url, Some("example.com")), TrackingDecision::Allow);
    }
}