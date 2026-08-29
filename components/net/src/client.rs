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
                user_agent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36".to_string(),
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
    #[error("unsupported Content-Encoding: {0}")]
    UnsupportedEncoding(String),
    #[error("failed to decode {encoding} response body: {message}")]
    Decode { encoding: String, message: String },
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
                .header(hyper::header::ACCEPT_ENCODING, "gzip, deflate, br")
                .header("Sec-Ch-Ua", "\"Not_A Brand\";v=\"8\", \"Chromium\";v=\"120\", \"Google Chrome\";v=\"120\"")
                .header("Sec-Ch-Ua-Mobile", "?0")
                .header("Sec-Ch-Ua-Platform", "\"Windows\"")
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
        let headers: Vec<(String, String)> = response
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

        let encoding = headers
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case("content-encoding"))
            .map(|(_, value)| value.to_ascii_lowercase())
            .unwrap_or_default();
        let body = decode_body(bytes, &encoding, limit)?;
        let headers: Vec<(String, String)> = headers
            .into_iter()
            .filter(|(name, _)| !name.eq_ignore_ascii_case("content-encoding"))
            .collect();

        Ok(FetchResponse {
            status,
            final_url,
            headers,
            body,
        })
    }
}

/// Decode a response body according to its `Content-Encoding`. The same
/// size limit applies to the decompressed data as to the wire bytes.
fn decode_body(bytes: Vec<u8>, encoding: &str, limit: usize) -> Result<Bytes, HttpError> {
    if encoding.is_empty() || encoding == "identity" {
        return Ok(Bytes::from(bytes));
    }
    let decoded: Vec<u8> = match encoding {
        "gzip" | "x-gzip" => {
            let mut decoder = flate2::read::GzDecoder::new(bytes.as_slice());
            read_limited(&mut decoder, limit).map_err(|e| HttpError::Decode {
                encoding: encoding.to_string(),
                message: e.to_string(),
            })?
        }
        // Servers variously send zlib-wrapped or raw deflate; try both.
        "deflate" => {
            let mut zlib = flate2::read::ZlibDecoder::new(bytes.as_slice());
            match read_limited(&mut zlib, limit) {
                Ok(out) => out,
                Err(_) => {
                    let mut raw = flate2::read::DeflateDecoder::new(bytes.as_slice());
                    read_limited(&mut raw, limit).map_err(|e| HttpError::Decode {
                        encoding: encoding.to_string(),
                        message: e.to_string(),
                    })?
                }
            }
        }
        "br" => {
            let mut decoder = brotli::Decompressor::new(bytes.as_slice(), 4096);
            read_limited(&mut decoder, limit).map_err(|e| HttpError::Decode {
                encoding: encoding.to_string(),
                message: e.to_string(),
            })?
        }
        other => return Err(HttpError::UnsupportedEncoding(other.to_string())),
    };
    Ok(Bytes::from(decoded))
}

fn read_limited<R: std::io::Read>(reader: &mut R, limit: usize) -> std::io::Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            return Ok(out);
        }
        if out.len().saturating_add(n) > limit {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "decoded body exceeds configured limit",
            ));
        }
        out.extend_from_slice(&buf[..n]);
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
    fn default_client_uses_chrome_ua_without_identifying_suffix() {
        let client = HttpClient::default();
        let ua = client.policy().user_agent.as_str();
        assert!(ua.starts_with("Mozilla/5.0"));
        assert!(ua.contains("Chrome/120.0.0.0"));
        assert!(ua.ends_with("Safari/537.36"));
        assert!(!ua.contains("Kore"));
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

    #[test]
    fn decode_gzip_body() {
        use std::io::Write;
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(b"hello gzip").unwrap();
        let compressed = encoder.finish().unwrap();
        let decoded = decode_body(compressed, "gzip", 1_000_000).unwrap();
        assert_eq!(decoded.as_ref(), b"hello gzip");
    }

    #[test]
    fn decode_brotli_body() {
        let mut compressed = Vec::new();
        brotli::enc::BrotliCompress(
            &mut &b"hello brotli"[..],
            &mut compressed,
            &brotli::enc::BrotliEncoderParams::default(),
        )
        .unwrap();
        let decoded = decode_body(compressed, "br", 1_000_000).unwrap();
        assert_eq!(decoded.as_ref(), b"hello brotli");
    }

    #[test]
    fn decode_identity_passes_through() {
        let decoded = decode_body(b"raw bytes".to_vec(), "identity", 1_000_000).unwrap();
        assert_eq!(decoded.as_ref(), b"raw bytes");
    }

    #[test]
    fn decode_unsupported_encoding_errors() {
        let err = decode_body(b"data".to_vec(), "zstd", 1_000_000).unwrap_err();
        assert!(matches!(err, HttpError::UnsupportedEncoding(_)));
    }

    #[test]
    fn decode_enforces_limit_on_decompressed_data() {
        use std::io::Write;
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(b"hello gzip").unwrap();
        let compressed = encoder.finish().unwrap();
        let err = decode_body(compressed, "gzip", 4).unwrap_err();
        assert!(matches!(err, HttpError::Decode { .. }));
    }
}
