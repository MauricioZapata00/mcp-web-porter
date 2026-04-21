use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageImage {
    pub url:       String,
    pub data:      String,
    pub mime_type: String,
}

impl PageImage {
    pub fn new(url: String, data: String, mime_type: String) -> Self {
        Self { url, data, mime_type }
    }

    pub fn url(&self) -> &str { &self.url }
    pub fn data(&self) -> &str { &self.data }
    pub fn mime_type(&self) -> &str { &self.mime_type }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HtmlContent {
    pub url: String,
    pub html: String,
    pub text: String,
}

impl HtmlContent {
    pub fn new(url: String, html: String, text: String) -> Self {
        Self { url, html, text }
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn html(&self) -> &str {
        &self.html
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn html_length(&self) -> usize {
        self.html.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_page_image_creation() {
        let img = PageImage::new(
            "https://example.com/img.png".to_string(),
            "abc123".to_string(),
            "image/png".to_string(),
        );
        assert_eq!(img.url(), "https://example.com/img.png");
        assert_eq!(img.data(), "abc123");
        assert_eq!(img.mime_type(), "image/png");
    }

    #[test]
    fn test_html_content_creation() {
        let content = HtmlContent::new(
            "https://example.com".to_string(),
            "<html></html>".to_string(),
            "Hello world".to_string(),
        );
        assert_eq!(content.url(), "https://example.com");
        assert_eq!(content.html(), "<html></html>");
        assert_eq!(content.text(), "Hello world");
    }

    #[test]
    fn test_html_content_length() {
        let content = HtmlContent::new(
            "https://example.com".to_string(),
            "<html></html>".to_string(),
            "".to_string(),
        );
        assert_eq!(content.html_length(), 13);
    }
}
