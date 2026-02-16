use crate::operations::fetch_html;
use crate::types::{HtmlContent, HtmlError, ResourceUri};

#[derive(Clone)]
pub struct HtmlResourceHandler;

impl HtmlResourceHandler {
    pub fn new() -> Self {
        Self
    }

    pub async fn handle_read(&self, uri: &str) -> Result<HtmlContent, HtmlError> {
        let resource_uri = ResourceUri::parse(uri)?;
        let url = resource_uri.url();
        fetch_html(url).await
    }
}

impl Default for HtmlResourceHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_handle_read_valid_https_url() {
        let handler = HtmlResourceHandler::new();
        let result = handler.handle_read("https://example.com").await;
        assert!(result.is_ok());
        let content = result.unwrap();
        assert_eq!(content.url(), "https://example.com");
        assert!(!content.html().is_empty());
    }

    #[tokio::test]
    async fn test_handle_read_valid_http_url() {
        let handler = HtmlResourceHandler::new();
        let result = handler.handle_read("http://example.com").await;
        assert!(result.is_ok());
        let content = result.unwrap();
        assert_eq!(content.url(), "http://example.com");
        assert!(!content.html().is_empty());
    }

    #[tokio::test]
    async fn test_handle_read_invalid_url_scheme() {
        let handler = HtmlResourceHandler::new();
        let result = handler.handle_read("ftp://example.com").await;
        assert!(result.is_err());
        match result {
            Err(HtmlError::InvalidUrl(_)) => {}
            _ => panic!("Expected InvalidUrl error for unsupported scheme"),
        }
    }

    #[tokio::test]
    async fn test_handle_read_empty_url() {
        let handler = HtmlResourceHandler::new();
        let result = handler.handle_read("").await;
        assert!(result.is_err());
        match result {
            Err(HtmlError::InvalidUrl(_)) => {}
            _ => panic!("Expected InvalidUrl error for empty URL"),
        }
    }

    #[tokio::test]
    async fn test_handle_read_404() {
        let handler = HtmlResourceHandler::new();
        let result = handler
            .handle_read("https://httpbin.org/status/404")
            .await;
        assert!(result.is_err());
        match result {
            Err(HtmlError::HttpError(404)) => {}
            _ => panic!("Expected HttpError with 404 status"),
        }
    }
}
