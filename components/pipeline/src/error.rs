use thiserror::Error;

#[derive(Debug, Error)]
pub enum PipelineError {
    #[error("HTTP fetch error: {0}")]
    Http(#[from] kore_net::HttpError),

    #[error("HTML parse error: {0:?}")]
    Html(#[from] kore_html::TokenizerError),

    #[error("CSS parse error: {0:?}")]
    Css(#[from] kore_css::ParserError),

    #[error("Layout error: {0:?}")]
    Layout(#[from] kore_layout::LayoutError),

    #[error("JS error: {0}")]
    Js(String),

    #[error("Response body is not valid UTF-8")]
    InvalidUtf8,

    #[error("URL error: {0}")]
    Url(#[from] url::ParseError),

    #[error("Missing linked stylesheet href")]
    MissingStylesheetHref,

    #[error("Too many redirects")]
    RedirectLimit,
}
