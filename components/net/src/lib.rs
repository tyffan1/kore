//! Network-process foundation for Kore.

mod cache;
mod client;
mod policy;
mod tracking;

pub use cache::{CacheEntry, CacheLookup, HttpCache};
pub use client::{BoxedFetch, CookieJar, Fetcher, HttpClient, HttpClientConfig, HttpError};
pub use kore_ipc::{FetchRequest, FetchResponse, Method};
pub use policy::{NetworkPolicy, PolicyDecision, PolicyError};
pub use tracking::{
    cookie_policy, is_third_party, registrable_domain, tracker_category, BlockReason,
    BlockedRequest, CookieDecision, EtpLevel, TrackingDecision, TrackingProtection,
    TrackerCategory,
};
