use crate::browser::fetch_rendered_html;
use super::parse::extract_text;
use crate::types::{HtmlContent, HtmlError};

pub async fn fetch_html(url: &str) -> Result<HtmlContent, HtmlError> {
    fetch_html_with_options(url, false).await
}

pub async fn fetch_html_with_options(url: &str, stealth: bool) -> Result<HtmlContent, HtmlError> {
    let html = fetch_rendered_html(url, 1500, stealth).await?;
    let text = extract_text(&html);
    Ok(HtmlContent::new(url.to_string(), html, text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[serial_test::serial]
    #[tokio::test(flavor = "multi_thread")]
    async fn test_fetch_html_valid_url() {
        let result = fetch_html("https://httpbin.org/html").await;
        assert!(result.is_ok());
        let content = result.unwrap();
        assert_eq!(content.url(), "https://httpbin.org/html");
        assert!(content.html().contains("<html") || content.html().contains("<HTML"));
        assert!(!content.text().is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_fetch_html_invalid_url() {
        let result = fetch_html("not-a-valid-url").await;
        assert!(result.is_err());
    }
}
