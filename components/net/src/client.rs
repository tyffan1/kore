use crate::{NetworkPolicy, PolicyError};
use bytes::Bytes;
use http_body_util::{BodyExt, Empty};
use hyper::body::Incoming;
use hyper::{Request, Uri};
use hyper_rustls::HttpsConnectorBuilder;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use std::time::Duration;
use thiserror::Error;
use url::Url;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Method {
    Get,
    Head,
}

impl From<Method> for hyper::Method {
    fn from(method: Method) -> Self {
        match method {
            Method::Get => hyper::Method::GET,
            Method::Head => hyper::Method::HEAD,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FetchRequest {
    pub url: Url,
    pub method: Method,
}

impl FetchRequest {
    pub fn get(url: &str) -> Result<Self, HttpError> {
        Ok(Self {
            url: Url::parse(url)?,
            method: Method::Get,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchResponse {
    pub status: u16,
    pub final_url: Url,
    pub headers: Vec<(String, String)>,
    pub body: Bytes,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpClientConfig {
    pub policy: NetworkPolicy,
    pub connect_timeout: Duration,
}

impl Default for HttpClientConfig {
    fn default() -> Self {
        Self {
            policy: NetworkPolicy::default(),
            connect_timeout: Duration::from_secs(10),
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
}

type HttpsClient = Client<hyper_rustls::HttpsConnector<HttpConnector>, Empty<Bytes>>;

#[derive(Clone)]
pub struct HttpClient {
    config: HttpClientConfig,
    inner: HttpsClient,
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
        Self { config, inner }
    }

    pub fn policy(&self) -> &NetworkPolicy {
        &self.config.policy
    }

    pub async fn fetch(&self, request: FetchRequest) -> Result<FetchResponse, HttpError> {
        self.config.policy.validate_url(&request.url)?;

        let uri = Uri::from_str(request.url.as_str())?;
        let hyper_request = Request::builder()
            .method(hyper::Method::from(request.method))
            .uri(uri)
            .header(
                hyper::header::USER_AGENT,
                self.config.policy.user_agent.as_str(),
            )
            .body(Empty::<Bytes>::new())?;

        let response = self.inner.request(hyper_request).await?;
        self.collect_response(request.url, response).await
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
        assert_eq!(client.policy().user_agent, "Kore/0.1.0");
        assert!(client.policy().max_body_bytes > 0);
    }
}
