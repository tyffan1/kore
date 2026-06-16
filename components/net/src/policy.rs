use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkPolicy {
    pub allow_plain_http: bool,
    pub max_body_bytes: usize,
    pub user_agent: String,
}

impl Default for NetworkPolicy {
    fn default() -> Self {
        Self {
            allow_plain_http: true,
            max_body_bytes: 16 * 1024 * 1024,
            user_agent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyDecision {
    Allowed,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PolicyError {
    #[error("unsupported URL scheme: {0}")]
    UnsupportedScheme(String),
    #[error("plain HTTP is blocked by policy")]
    PlainHttpBlocked,
}

impl NetworkPolicy {
    pub fn validate_url(&self, url: &Url) -> Result<PolicyDecision, PolicyError> {
        match url.scheme() {
            "https" => Ok(PolicyDecision::Allowed),
            "http" if self.allow_plain_http => Ok(PolicyDecision::Allowed),
            "http" => Err(PolicyError::PlainHttpBlocked),
            other => Err(PolicyError::UnsupportedScheme(other.to_string())),
        }
    }
}
