use std::collections::HashMap;

use chromiumoxide::cdp::browser_protocol::network::{
    CookieParam as CdpCookieParam, Headers as CdpHeaders, SetExtraHttpHeadersParams,
};

use crate::browser::{
    apply_request_options, detect_form_on_page, get_or_init_session, BrowserOps,
    ChromeCookieExtractor, ChromiumBrowser, CookieExtractor,
};
use crate::operations::act::{execute_actions_concurrent, validate_actions};
use crate::operations::extract_text;
use crate::types::{
    ActResult, FormStep, HtmlError, PageAction, RequestOptions, ResourceUri,
    DEFAULT_MAX_ACTIONS,
};

const POST_ACTION_SETTLE_MS: u64 = 1500;
const DEFAULT_FIELD_TIMEOUT_MS: u64 = 2_000;

#[derive(Clone, Default)]
pub struct ActHandler;

impl ActHandler {
    pub async fn handle_act(
        &self,
        url:              &str,
        actions:          Vec<PageAction>,
        max_actions:      Option<usize>,
        field_timeout_ms: Option<u64>,
        options:          Option<RequestOptions>,
    ) -> Result<ActResult, HtmlError> {
        ResourceUri::parse(url)?;
        let max     = max_actions.unwrap_or(DEFAULT_MAX_ACTIONS);
        let timeout = field_timeout_ms.unwrap_or(DEFAULT_FIELD_TIMEOUT_MS);
        validate_actions(&actions, max, timeout)?;

        let mut driver = ChromiumBrowser::new(false);
        let result = run_stateless(url, &actions, timeout, options.as_ref(), &mut driver).await;
        driver.cleanup().await;
        result
    }

    pub async fn handle_act_with_session(
        &self,
        url:              &str,
        actions:          Vec<PageAction>,
        max_actions:      Option<usize>,
        field_timeout_ms: Option<u64>,
        _debug_port:      Option<u16>,
        headers:          Option<HashMap<String, String>>,
    ) -> Result<ActResult, HtmlError> {
        ResourceUri::parse(url)?;
        let max     = max_actions.unwrap_or(DEFAULT_MAX_ACTIONS);
        let timeout = field_timeout_ms.unwrap_or(DEFAULT_FIELD_TIMEOUT_MS);
        validate_actions(&actions, max, timeout)?;

        let mut guard = get_or_init_session().await?;
        let session = guard.as_mut().ok_or_else(|| {
            HtmlError::SessionUnavailable("Failed to acquire session".to_string())
        })?;

        let page = session
            .browser
            .new_page("about:blank")
            .await
            .map_err(|e| HtmlError::SessionUnavailable(format!(
                "Failed to create new tab in session: {}", e
            )))?;

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
                    headers: CdpHeaders::new(serde_json::Value::Object(json_obj)),
                })
                .await
                .ok();
            }
        }

        page.goto(url).await.map_err(|e| {
            HtmlError::BrowserError(format!("Failed to navigate to URL: {}", e))
        })?;
        tokio::time::sleep(std::time::Duration::from_millis(POST_ACTION_SETTLE_MS)).await;

        let result = run_on_page(&page, &actions, timeout).await;
        page.close().await.ok();
        result
    }
}

async fn run_stateless(
    url:              &str,
    actions:          &[PageAction],
    field_timeout_ms: u64,
    options:          Option<&RequestOptions>,
    ops:              &mut ChromiumBrowser,
) -> Result<ActResult, HtmlError> {
    ops.start().await?;
    apply_request_options(url, options, ops).await?;
    ops.open(url).await?;
    ops.read(POST_ACTION_SETTLE_MS).await?;
    let page = ops
        .page()
        .ok_or_else(|| HtmlError::BrowserError("page not opened".into()))?;
    run_on_page(page, actions, field_timeout_ms).await
}

async fn run_on_page(
    page:             &chromiumoxide::Page,
    actions:          &[PageAction],
    field_timeout_ms: u64,
) -> Result<ActResult, HtmlError> {
    let (detected, _) = detect_form_on_page(page).await?;
    let action_results =
        execute_actions_concurrent(page, actions, &detected, field_timeout_ms).await;

    tokio::time::sleep(std::time::Duration::from_millis(POST_ACTION_SETTLE_MS)).await;

    let html = page
        .content()
        .await
        .map_err(|e| HtmlError::BrowserError(e.to_string()))?;
    let page_text_after = extract_text(&html);

    let (next_fields, next_buttons) = detect_form_on_page(page).await?;
    let next_step = if next_fields.is_empty() {
        None
    } else {
        Some(FormStep { fields: next_fields, buttons: next_buttons })
    };

    Ok(ActResult { action_results, page_text_after, next_step })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handler_is_cloneable() {
        let a = ActHandler::default();
        let b = a.clone();
        drop((a, b));
    }

    #[tokio::test]
    async fn handle_act_rejects_empty_actions_before_browser() {
        let handler = ActHandler::default();
        let result = handler
            .handle_act("https://example.com", vec![], None, None, None)
            .await;
        assert!(matches!(result, Err(HtmlError::NoActions)));
    }

    #[tokio::test]
    async fn handle_act_rejects_too_many_actions_before_browser() {
        let handler = ActHandler::default();
        let actions = vec![
            PageAction::Fill { identifier: "a".into(), value: "x".into() };
            3
        ];
        let result = handler
            .handle_act("https://example.com", actions, Some(2), None, None)
            .await;
        assert!(matches!(result, Err(HtmlError::TooManyActions { count: 3, max: 2 })));
    }

    #[tokio::test]
    async fn handle_act_rejects_invalid_url_before_browser() {
        let handler = ActHandler::default();
        let actions = vec![PageAction::Fill { identifier: "a".into(), value: "x".into() }];
        let result = handler
            .handle_act("not-a-url", actions, None, None, None)
            .await;
        assert!(matches!(result, Err(HtmlError::InvalidUrl(_))));
    }

    #[tokio::test]
    async fn handle_act_with_session_rejects_invalid_url_before_session() {
        let handler = ActHandler::default();
        let actions = vec![PageAction::Fill { identifier: "a".into(), value: "x".into() }];
        let result = handler
            .handle_act_with_session("not-a-url", actions, None, None, None, None)
            .await;
        assert!(matches!(result, Err(HtmlError::InvalidUrl(_))));
    }
}
