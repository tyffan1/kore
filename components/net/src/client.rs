use crate::{cache::CacheLookup, NetworkPolicy, PolicyError};
use bytes::Bytes;
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Empty, Full};
use hyper::body::Incoming;
use hyper::{Request, Uri};
use hyper_rustls::HttpsConnectorBuilder;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use kore_ipc::{FetchRequest, FetchResponse, Method};
use std::collections::HashMap;
use std::convert::Infallible;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use thiserror::Error;
use url::Url;

use crate::cache::HttpCache;
use crate::tracking::{cookie_policy, CookieDecision};

fn hyper_method(method: &Method) -> hyper::Method {
    match method {
        Method::Get => hyper::Method::GET,
        Method::Head => hyper::Method::HEAD,
        Method::Post => hyper::Method::POST,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpClientConfig {
    pub policy: NetworkPolicy,
    pub connect_timeout: Duration,
    /// Block `Cookie`/`Set-Cookie` on requests whose site differs from the
    /// top-level document's (third-party cookie blocking, ETP default).
    pub block_third_party_cookies: bool,
}

impl Default for HttpClientConfig {
    fn default() -> Self {
        Self {
            policy: NetworkPolicy {
                user_agent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36 Kore/0.1.0".to_string(),
                ..NetworkPolicy::default()
            },
            connect_timeout: Duration::from_secs(10),
            block_third_party_cookies: true,
        }
    }
}

#[derive(Debug, Error)]
pub enum HttpError {
    #[error(transparent)]
    InvalidUrl(#[from] url::ParseError),
    #[error(transparent)]
    Policy(#[from] PolicyError),
    #[error("invalid request URI")]
    InvalidUri(#[from] hyper::http::uri::InvalidUri),
    #[error("invalid request")]
    InvalidRequest(#[from] hyper::http::Error),
    #[error("request failed")]
    Request(#[from] hyper_util::client::legacy::Error),
    #[error("response body failed")]
    Body(#[from] hyper::Error),
    #[error("response body exceeded configured limit of {limit} bytes")]
    BodyTooLarge { limit: usize },
    #[error("exceeded maximum number of redirects ({0})")]
    TooManyRedirects(u8),
    #[error("redirect location header is invalid")]
    InvalidRedirectLocation,
}

type HttpsClient = Client<hyper_rustls::HttpsConnector<HttpConnector>, BoxBody<Bytes, hyper::Error>>;

#[derive(Clone, Default)]
pub struct CookieJar {
    inner: Arc<Mutex<HashMap<String, String>>>,
}

impl CookieJar {
    pub fn store(&self, domain: &str, cookies: &[String]) {
        let Ok(mut jar) = self.inner.lock() else {
            return;
        };
        for cookie in cookies {
            if let Some((name, value)) = cookie.split_once('=') {
                let key = format!("{}:{}", domain, name.trim());
                let value = value.split(';').next().unwrap_or("").trim().to_string();
                jar.insert(key, value);
            }
        }
    }

    pub fn get_header(&self, domain: &str) -> String {
        let Ok(jar) = self.inner.lock() else {
            return String::new();
        };
        jar.iter()
            .filter(|(k, _)| k.starts_with(&format!("{}:", domain)))
            .map(|(k, v)| {
                let name = k.splitn(2, ':').nth(1).unwrap_or("");
                format!("{}={}", name, v)
            })
            .collect::<Vec<_>>()
            .join("; ")
    }
}

#[derive(Clone)]
pub struct HttpClient {
    config: HttpClientConfig,
    inner: HttpsClient,
    pub cookie_jar: CookieJar,
    /// Shared HTTP cache (Cache-Control / ETag revalidation).
    pub cache: HttpCache,
}

impl HttpClient {
    pub fn new(config: HttpClientConfig) -> Self {
        let mut http = HttpConnector::new();
        http.enforce_http(false);
        http.set_connect_timeout(Some(config.connect_timeout));

        let https = HttpsConnectorBuilder::new()
            .with_webpki_roots()
            .https_or_http()
            .enable_http1()
            .enable_http2()
            .wrap_connector(http);

        let inner = Client::builder(TokioExecutor::new()).build(https);
        Self {
            config,
            inner,
            cookie_jar: CookieJar::default(),
            cache: HttpCache::default(),
        }
    }

    pub fn policy(&self) -> &NetworkPolicy {
        &self.config.policy
    }

    pub async fn fetch(&self, request: FetchRequest) -> Result<FetchResponse, HttpError> {
        self.config.policy.validate_url(&request.url)?;

        let mut url = request.url;
        let method = hyper_method(&request.method);
        let cacheable = matches!(request.method, Method::Get);
        let mut remaining = 10u8;

        loop {
            let key = url.as_str().to_string();
            let cached_entry = if cacheable {
                match self.cache.lookup(&key) {
                    Some(CacheLookup::Fresh(entry)) => return Ok(entry.to_hit_response()),
                    Some(CacheLookup::Stale(entry)) => Some(entry),
                    None => None,
                }
            } else {
                None
            };

            let uri = Uri::from_str(url.as_str())?;

            let mut builder = Request::builder()
                .method(method.clone())
                .uri(uri)
                .header(hyper::header::USER_AGENT, self.config.policy.user_agent.as_str())
                .header(hyper::header::ACCEPT, "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8")
                .header(hyper::header::ACCEPT_LANGUAGE, "en-US,en;q=0.9")
                .header(hyper::header::ACCEPT_ENCODING, "identity")
                .header("Sec-Fetch-Dest", "document")
                .header("Sec-Fetch-Mode", "navigate")
                .header("Sec-Fetch-Site", "none")
                .header("Sec-Fetch-User", "?1")
                .header("Upgrade-Insecure-Requests", "1");

            for (name, value) in &request.headers {
                builder = builder.header(name.as_str(), value.as_str());
            }

            if let Some(entry) = &cached_entry {
                for (name, value) in entry.conditional_headers() {
                    builder = builder.header(name.as_str(), value.as_str());
                }
            }

            let third_party_blocked = cookie_policy(
                request.top_level.as_deref(),
                url.host_str().unwrap_or(""),
                self.config.block_third_party_cookies,
            ) == CookieDecision::Block;
            // ETP: third-party requests get no cookies at all.
            let cookie_header = if third_party_blocked {
                String::new()
            } else {
                self.cookie_jar.get_header(url.host_str().unwrap_or(""))
            };
            if !cookie_header.is_empty() {
                builder = builder.header(hyper::header::COOKIE, cookie_header.as_str());
            }

            let body: BoxBody<Bytes, hyper::Error> = match &request.body {
                Some(b) => Full::new(b.clone())
                    .map_err(|_: Infallible| -> hyper::Error { unreachable!() })
                    .boxed(),
                None => Empty::<Bytes>::new()
                    .map_err(|_: Infallible| -> hyper::Error { unreachable!() })
                    .boxed(),
            };

            let hyper_request = builder.body(body)?;
            let response = self.inner.request(hyper_request).await?;
            let status = response.status();

            // Store Set-Cookie headers
            let set_cookies: Vec<String> = response.headers()
                .get_all(hyper::header::SET_COOKIE)
                .iter()
                .filter_map(|v| v.to_str().ok())
                .map(|s| s.to_string())
                .collect();
            if !set_cookies.is_empty() {
                let domain = url.host_str().unwrap_or("");
                let third_party_blocked = cookie_policy(
                    request.top_level.as_deref(),
                    domain,
                    self.config.block_third_party_cookies,
                ) == CookieDecision::Block;
                if !third_party_blocked {
                    self.cookie_jar.store(domain, &set_cookies);
                }
            }

            if status.is_redirection() && remaining > 0 {
                if let Some(location) = response.headers().get(hyper::header::LOCATION) {
                    let location_str = location
                        .to_str()
                        .map_err(|_| HttpError::InvalidRedirectLocation)?;
                    let new_url = url
                        .join(location_str)
                        .map_err(|_| HttpError::InvalidRedirectLocation)?;
                    url = new_url;
                    remaining -= 1;
                    continue;
                }
            }

            // 304 Not Modified: revalidation succeeded, serve the cached
            // body without downloading it again.
            if status == hyper::StatusCode::NOT_MODIFIED {
                if let Some(entry) = cached_entry {
                    self.cache.refresh(&key);
                    return Ok(entry.to_revalidated_response());
                }
            }

            let fetched = self.collect_response(url, response).await?;
            if cacheable && fetched.status >= 200 && fetched.status < 300 {
                self.cache.store(&key, &fetched);
            }
            return Ok(fetched);
        }
    }

    async fn collect_response(
        &self,
        final_url: Url,
        response: hyper::Response<Incoming>,
    ) -> Result<FetchResponse, HttpError> {
        let status = response.status().as_u16();
        let headers = response
            .headers()
            .iter()
            .map(|(name, value)| {
                (
                    name.as_str().to_string(),
                    value.to_str().unwrap_or_default().to_string(),
                )
            })
            .collect();

        let limit = self.config.policy.max_body_bytes;
        let mut body = response.into_body();
        let mut bytes = Vec::new();
        while let Some(frame) = body.frame().await {
            let frame = frame?;
            if let Some(chunk) = frame.data_ref() {
                if bytes.len().saturating_add(chunk.len()) > limit {
                    return Err(HttpError::BodyTooLarge { limit });
                }
                bytes.extend_from_slice(chunk);
            }
        }

        Ok(FetchResponse {
            status,
            final_url,
            headers,
            body: Bytes::from(bytes),
        })
    }
}

impl Default for HttpClient {
    fn default() -> Self {
        Self::new(HttpClientConfig::default())
    }
}

/// A boxed, `Send` future returned by [`Fetcher::fetch`].
pub type BoxedFetch<'a> =
    std::pin::Pin<Box<dyn std::future::Future<Output = Result<FetchResponse, String>> + Send + 'a>>;

/// Abstraction over "where does the network stack run".
///
/// In-process callers use [`HttpClient`]; when the network stack is moved
/// into a dedicated process, a remote implementation forwards requests
/// over IPC. `FetchResponse`/`HttpClient` are serializable so the same
/// types cross the process boundary unchanged.
pub trait Fetcher: Send + Sync {
    fn fetch(&self, request: FetchRequest) -> BoxedFetch<'_>;
}

impl Fetcher for HttpClient {
    fn fetch(&self, request: FetchRequest) -> BoxedFetch<'_> {
        Box::pin(async move {
            HttpClient::fetch(self, request).await.map_err(|e| e.to_string())
        })
    }
}

#[cfg(test)]
mod fetcher_tests {
    use super::*;

    #[test]
    fn http_client_implements_fetcher() {
        fn requires_fetcher(_f: &dyn Fetcher) {}
        requires_fetcher(&HttpClient::default());
    }

    #[test]
    fn fetch_request_serde_roundtrip() {
        if let Ok(request) = FetchRequest::get("https://example.com/index.html") {
            if let Ok(encoded) = bincode::serialize(&request) {
                if let Ok(decoded) = bincode::deserialize::<FetchRequest>(&encoded) {
                    assert_eq!(decoded, request);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_get_requests_from_urls() {
        let request = FetchRequest::get("https://example.com/index.html").unwrap();
        assert_eq!(request.method, Method::Get);
        assert_eq!(request.url.host_str(), Some("example.com"));
    }

    #[test]
    fn policy_blocks_unknown_schemes() {
        let policy = NetworkPolicy::default();
        let url = Url::parse("file:///etc/passwd").unwrap();
        assert!(matches!(
            policy.validate_url(&url),
            Err(PolicyError::UnsupportedScheme(_))
        ));
    }

    #[test]
    fn policy_can_block_plain_http() {
        let policy = NetworkPolicy {
            allow_plain_http: false,
            ..NetworkPolicy::default()
        };
        let url = Url::parse("http://example.com/").unwrap();
        assert_eq!(
            policy.validate_url(&url),
            Err(PolicyError::PlainHttpBlocked)
        );
    }

    #[test]
    fn default_client_uses_privacy_preserving_policy_surface() {
        let client = HttpClient::default();
        assert!(client.policy().user_agent.contains("Mozilla/5.0"));
        assert!(client.policy().user_agent.contains("Kore"));
        assert!(client.policy().max_body_bytes > 0);
    }

    #[test]
    fn cookie_jar_stores_and_retrieves() {
        let jar = CookieJar::default();
        jar.store("example.com", &[
            "session=abc123; Path=/; HttpOnly".to_string(),
            "theme=dark; Path=/".to_string(),
        ]);
        let header = jar.get_header("example.com");
        assert!(header.contains("session=abc123"));
        assert!(header.contains("theme=dark"));
    }

    #[test]
    fn cookie_jar_isolates_domains() {
        let jar = CookieJar::default();
        jar.store("example.com", &["token=xyz".to_string()]);
        let header = jar.get_header("other.com");
        assert!(!header.contains("token=xyz"));
    }

    #[test]
    fn fetch_request_supports_post() {
        let mut req = FetchRequest::get("https://example.com/").unwrap();
        req.method = Method::Post;
        req.body = Some(Bytes::from("key=value"));
        assert_eq!(req.method, Method::Post);
        assert!(req.body.is_some());
    }
}
