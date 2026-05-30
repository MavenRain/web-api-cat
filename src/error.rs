//! Bridge error type.

use boa_cat::Error as EngineError;
use html_cat::Error as HtmlError;
use net_cat::Error as NetError;

/// All errors `web-api-cat` can produce.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// A JS engine error from boa-cat.
    Engine(EngineError),
    /// An HTML-parser error.
    Html(HtmlError),
    /// A network error from net-cat.
    Net(NetError),
}

impl From<EngineError> for Error {
    fn from(value: EngineError) -> Self {
        Self::Engine(value)
    }
}

impl From<HtmlError> for Error {
    fn from(value: HtmlError) -> Self {
        Self::Html(value)
    }
}

impl From<NetError> for Error {
    fn from(value: NetError) -> Self {
        Self::Net(value)
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Engine(e) => write!(f, "engine error: {e}"),
            Self::Html(e) => write!(f, "html error: {e}"),
            Self::Net(e) => write!(f, "net error: {e}"),
        }
    }
}

impl std::error::Error for Error {}
