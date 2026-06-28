use crate::browser::{ChromeCookieExtractor, CookieExtractor, get_or_init_session};
use crate::operations::extract_text;
use crate::types::{HtmlError, ResourceUri};
use chromiumoxide::cdp::browser_protocol::network::CookieParam as CdpCookieParam;
use chromiumoxide::cdp::browser_protocol::network::{Headers as CdpHeaders, SetExtraHttpHeadersParams};
use std::collections::HashMap;
use serde_json;

#[derive(Clone, Default)]
pub struct SessionHandler;

impl SessionHandler {
    pub async fn handle_read_with_session(
        &self,
        url: &str,
        _stealth: bool,
        include_images: bool,
        _debug_port: Option<u16>,
        headers: Option<HashMap<String, String>>,
    ) -> Result<(String, Vec<String>), HtmlError> {
        ResourceUri::parse(url)?;

        let mut guard = get_or_init_session().await?;
        let session = guard.as_mut()
            .ok_or_else(|| HtmlError::SessionUnavailable("Failed to acquire session".to_string()))?;

        let page = session.browser.new_page("about:blank").await
            .map_err(|e| HtmlError::SessionUnavailable(
                format!("Failed to create new tab in session: {}", e)
            ))?;

        let cookie_extractor = ChromeCookieExtractor;
        let cookies = cookie_extractor.extract(url)?;

        for cookie in cookies {
            let cdp_cookie = CdpCookieParam::builder()
                .name(cookie.name)
                .value(cookie.value)
                .domain(cookie.host_key)
                .path(cookie.path)
                .secure(cookie.is_secure)
                .build()
                .map_err(|e| HtmlError::BrowserError(e.to_string()))?;

            page.set_cookie(cdp_cookie).await
                .map_err(|e| HtmlError::BrowserError(format!("Failed to set cookie: {}", e)))?;
        }

        if let Some(hdrs) = headers {
            if !hdrs.is_empty() {
                let json_obj: serde_json::Map<String, serde_json::Value> = hdrs
                    .iter()
                    .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
                    .collect();
                page.execute(SetExtraHttpHeadersParams {
                    headers: CdpHeaders::new(serde_json::Value::Object(json_obj))
                }).await.ok();
            }
        }

        page.goto(url).await
            .map_err(|e| HtmlError::BrowserError(
                format!("Failed to navigate to URL: {}", e)
            ))?;

        tokio::time::sleep(tokio::time::Duration::from_millis(1500)).await;

        let html = page.content().await
            .map_err(|e| HtmlError::BrowserError(
                format!("Failed to read page content: {}", e)
            ))?;

        let text = extract_text(&html);
        let mut images = Vec::new();

        if include_images {
            let js = r#"Array.from(document.querySelectorAll('img')).map(img => img.src).filter(src => src && src.length > 0)"#;
            if let Ok(result) = page.evaluate(js).await {
                if let Some(v) = result.value() {
                    if let Ok(json_str) = serde_json::to_string(&v) {
                        if let Ok(urls) = serde_json::from_str::<Vec<String>>(&json_str) {
                            images = urls;
                        }
                    }
                }
            }
        }

        page.close().await.ok();

        Ok((text, images))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handler_is_cloneable() {
        let handler1 = SessionHandler::default();
        let handler2 = handler1.clone();
        // This test verifies that SessionHandler can be cloned
        // (the actual browser interaction is tested in integration tests)
        drop((handler1, handler2));
    }

    #[test]
    fn test_handler_defaults() {
        let _handler = SessionHandler::default();
        // SessionHandler holds no state, so just verify it can be created
    }
}
