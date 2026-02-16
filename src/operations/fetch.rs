use crate::types::{HtmlContent, HtmlError};

pub async fn fetch_html(url: &str) -> Result<HtmlContent, HtmlError> {
    let client = reqwest::Client::new();

    let response = match client.get(url).send().await {
        Ok(resp) => resp,
        Err(e) => return Err(HtmlError::FetchError(e.to_string())),
    };

    let status = response.status();
    if !status.is_success() {
        return Err(HtmlError::HttpError(status.as_u16()));
    }

    let html = match response.text().await {
        Ok(text) => text,
        Err(e) => return Err(HtmlError::FetchError(e.to_string())),
    };

    Ok(HtmlContent::new(url.to_string(), html))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_fetch_html_valid_url() {
        let result = fetch_html("https://example.com").await;
        assert!(result.is_ok());
        let content = result.unwrap();
        assert_eq!(content.url(), "https://example.com");
        assert!(content.html().contains("Example Domain") || content.html().contains("<html"));
    }

    #[tokio::test]
    async fn test_fetch_html_404() {
        let result = fetch_html("https://httpbin.org/status/404").await;
        assert!(result.is_err());
        match result {
            Err(HtmlError::HttpError(404)) => {}
            _ => panic!("Expected HttpError with 404 status"),
        }
    }

    #[tokio::test]
    async fn test_fetch_html_invalid_url() {
        let result = fetch_html("not-a-valid-url").await;
        assert!(result.is_err());
    }
}
