use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HtmlContent {
    pub url: String,
    pub html: String,
}

impl HtmlContent {
    pub fn new(url: String, html: String) -> Self {
        Self { url, html }
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn html(&self) -> &str {
        &self.html
    }

    pub fn html_length(&self) -> usize {
        self.html.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_html_content_creation() {
        let content = HtmlContent::new(
            "https://example.com".to_string(),
            "<html></html>".to_string(),
        );
        assert_eq!(content.url(), "https://example.com");
        assert_eq!(content.html(), "<html></html>");
    }

    #[test]
    fn test_html_content_length() {
        let content = HtmlContent::new(
            "https://example.com".to_string(),
            "<html></html>".to_string(),
        );
        assert_eq!(content.html_length(), 13);
    }
}
