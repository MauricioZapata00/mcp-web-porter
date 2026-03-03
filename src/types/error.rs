use std::fmt;

#[derive(Debug)]
pub enum HtmlError {
    FetchError(String),
    InvalidUrl(String),
    HttpError(u16),
    BrowserError(String),
}

impl fmt::Display for HtmlError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            HtmlError::FetchError(msg) => write!(f, "Failed to fetch URL: {}", msg),
            HtmlError::InvalidUrl(msg) => write!(f, "Invalid URL: {}", msg),
            HtmlError::HttpError(status) => write!(f, "HTTP error: {}", status),
            HtmlError::BrowserError(msg) => write!(f, "Browser error: {}", msg),
        }
    }
}

impl std::error::Error for HtmlError {}
