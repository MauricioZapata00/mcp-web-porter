use base64::Engine as _;
use crate::browser::{fetch_rendered_html, fetch_rendered_html_with_images};
use super::parse::extract_text;
use crate::types::{HtmlContent, HtmlError, PageImage, RequestOptions};

pub async fn fetch_html(url: &str) -> Result<HtmlContent, HtmlError> {
    fetch_html_with_options(url, false, None).await
}

pub async fn fetch_html_with_options(
    url:     &str,
    stealth: bool,
    options: Option<&RequestOptions>,
) -> Result<HtmlContent, HtmlError> {
    let html = fetch_rendered_html(url, 1500, stealth, options).await?;
    let text = extract_text(&html);
    Ok(HtmlContent::new(url.to_string(), html, text))
}

pub async fn fetch_html_with_images_options(
    url:     &str,
    stealth: bool,
    options: Option<&RequestOptions>,
) -> Result<(HtmlContent, Vec<PageImage>), HtmlError> {
    let (html, image_urls) = fetch_rendered_html_with_images(url, 1500, stealth, options).await?;
    let text = extract_text(&html);
    let content = HtmlContent::new(url.to_string(), html, text);
    let images = fetch_page_images(image_urls).await;
    Ok((content, images))
}

async fn fetch_page_images(urls: Vec<String>) -> Vec<PageImage> {
    let client = reqwest::Client::new();
    let mut images = Vec::new();
    for url in urls {
        if let Some(img) = fetch_single_image(&client, &url).await {
            images.push(img);
        }
    }
    images
}

async fn fetch_single_image(client: &reqwest::Client, url: &str) -> Option<PageImage> {
    if let Some(rest) = url.strip_prefix("data:") {
        return parse_data_uri(url, rest);
    }
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return None;
    }
    let response = client.get(url).send().await.ok()?;
    let mime_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(';').next().unwrap_or(s).trim().to_string())
        .unwrap_or_else(|| mime_from_url(url));
    if !mime_type.starts_with("image/") {
        return None;
    }
    let bytes = response.bytes().await.ok()?;
    let data = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Some(PageImage::new(url.to_string(), data, mime_type))
}

fn parse_data_uri(original_url: &str, data_part: &str) -> Option<PageImage> {
    let (meta, data) = data_part.split_once(',')?;
    if !meta.ends_with(";base64") {
        return None;
    }
    let mime_type = meta.strip_suffix(";base64")?.to_string();
    if !mime_type.starts_with("image/") {
        return None;
    }
    Some(PageImage::new(original_url.to_string(), data.to_string(), mime_type))
}

fn mime_from_url(url: &str) -> String {
    let path = url.split('?').next().unwrap_or(url);
    match path.rsplit('.').next().unwrap_or("").to_lowercase().as_str() {
        "jpg" | "jpeg" => "image/jpeg".to_string(),
        "gif"          => "image/gif".to_string(),
        "webp"         => "image/webp".to_string(),
        "svg"          => "image/svg+xml".to_string(),
        _              => "image/png".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mime_from_url_jpeg() {
        assert_eq!(mime_from_url("https://example.com/photo.jpg"), "image/jpeg");
        assert_eq!(mime_from_url("https://example.com/photo.jpeg"), "image/jpeg");
    }

    #[test]
    fn test_mime_from_url_gif() {
        assert_eq!(mime_from_url("https://example.com/anim.gif"), "image/gif");
    }

    #[test]
    fn test_mime_from_url_webp() {
        assert_eq!(mime_from_url("https://example.com/img.webp"), "image/webp");
    }

    #[test]
    fn test_mime_from_url_svg() {
        assert_eq!(mime_from_url("https://example.com/icon.svg"), "image/svg+xml");
    }

    #[test]
    fn test_mime_from_url_unknown_defaults_to_png() {
        assert_eq!(mime_from_url("https://example.com/img.bmp"), "image/png");
        assert_eq!(mime_from_url("https://example.com/img"), "image/png");
    }

    #[test]
    fn test_mime_from_url_ignores_query_string() {
        assert_eq!(mime_from_url("https://example.com/img.gif?v=1"), "image/gif");
    }

    #[test]
    fn test_parse_data_uri_valid_png() {
        let uri = "data:image/png;base64,abc123==";
        let result = parse_data_uri(uri, "image/png;base64,abc123==");
        assert!(result.is_some());
        let img = result.unwrap();
        assert_eq!(img.mime_type(), "image/png");
        assert_eq!(img.data(), "abc123==");
        assert_eq!(img.url(), uri);
    }

    #[test]
    fn test_parse_data_uri_valid_jpeg() {
        let uri = "data:image/jpeg;base64,/9j/xyz";
        let result = parse_data_uri(uri, "image/jpeg;base64,/9j/xyz");
        assert!(result.is_some());
        assert_eq!(result.unwrap().mime_type(), "image/jpeg");
    }

    #[test]
    fn test_parse_data_uri_non_base64_returns_none() {
        let uri = "data:image/png,rawdata";
        let result = parse_data_uri(uri, "image/png,rawdata");
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_data_uri_non_image_returns_none() {
        let uri = "data:text/plain;base64,aGVsbG8=";
        let result = parse_data_uri(uri, "text/plain;base64,aGVsbG8=");
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_data_uri_malformed_returns_none() {
        let result = parse_data_uri("data:bad", "bad");
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_fetch_page_images_empty_list() {
        let result = fetch_page_images(vec![]).await;
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_fetch_single_image_data_uri_png() {
        let client = reqwest::Client::new();
        let tiny_png = "data:image/png;base64,iVBORw0KGgo=";
        let result = fetch_single_image(&client, tiny_png).await;
        assert!(result.is_some());
        let img = result.unwrap();
        assert_eq!(img.mime_type(), "image/png");
        assert_eq!(img.data(), "iVBORw0KGgo=");
    }

    #[tokio::test]
    async fn test_fetch_single_image_non_http_scheme_returns_none() {
        let client = reqwest::Client::new();
        let result = fetch_single_image(&client, "ftp://example.com/img.png").await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_fetch_page_images_skips_non_http() {
        let urls = vec![
            "ftp://example.com/img.png".to_string(),
            "blob:https://example.com/abc".to_string(),
        ];
        let result = fetch_page_images(urls).await;
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_fetch_page_images_includes_data_uris() {
        let urls = vec![
            "data:image/png;base64,iVBORw0KGgo=".to_string(),
            "data:image/jpeg;base64,/9j/abc".to_string(),
        ];
        let result = fetch_page_images(urls).await;
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].mime_type(), "image/png");
        assert_eq!(result[1].mime_type(), "image/jpeg");
    }

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
