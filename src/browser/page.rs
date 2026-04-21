use chromiumoxide::{Browser, BrowserConfig};
use chromiumoxide::cdp::browser_protocol::network::CookieParam as CdpCookieParam;
use futures::StreamExt;
use std::collections::HashMap;
use crate::types::{
    ButtonResolvedBy, Cookie, FieldValue, FormButton, FormField, FormFieldType, HtmlError,
    RequestOptions, SelectOption,
};

const DETECT_JS: &str = r#"(() => {
  const toSel = (el, i) => {
    if (el.id) return '#' + CSS.escape(el.id);
    if (el.name) return '[name="' + el.name + '"]';
    return el.tagName.toLowerCase() + ':nth-of-type(' + (i+1) + ')';
  };
  const fields = Array.from(
    document.querySelectorAll('input:not([type="hidden"]),select,textarea')
  ).map((el, i) => {
    let label = null;
    if (el.id) {
      const lbl = document.querySelector('label[for="' + el.id + '"]');
      if (lbl) label = lbl.textContent.trim();
    }
    if (!label) {
      const lbl = el.closest('label');
      if (lbl) label = lbl.textContent.trim();
    }
    if (!label) label = el.getAttribute('aria-label');
    const options = el.tagName === 'SELECT'
      ? Array.from(el.options).map(o => ({value: o.value, label: o.text.trim()}))
      : [];
    const tp = el.tagName === 'TEXTAREA' ? 'textarea'
      : el.tagName === 'SELECT' ? 'select'
      : (el.type || 'text').toLowerCase();
    return {
      selector: toSel(el, i), field_type: tp,
      name: el.name || null, id: el.id || null,
      label, placeholder: el.placeholder || null,
      required: el.required, options,
    };
  });
  const btnLabel = el => {
    const txt = el.textContent.trim().replace(/\s+/g,' ');
    if (txt) return txt;
    const aria = el.getAttribute('aria-label');
    if (aria) return aria;
    const title = el.getAttribute('title');
    if (title) return title;
    const svgTitle = el.querySelector('svg title');
    if (svgTitle && svgTitle.textContent.trim()) return svgTitle.textContent.trim();
    const img = el.querySelector('img[alt]');
    if (img && img.alt) return img.alt;
    return null;
  };
  const buttons = Array.from(
    document.querySelectorAll('button,input[type="submit"],input[type="button"],a[href]')
  ).map((el, i) => {
    const lbl = btnLabel(el);
    if (!lbl) return null;
    return {
      label: lbl,
      selector: el.id ? '#' + CSS.escape(el.id) : el.tagName.toLowerCase() + ':nth-of-type(' + (i+1) + ')',
    };
  }).filter(b => b !== null);
  return JSON.stringify({fields, buttons});
})()"#;

const GET_IMAGES_JS: &str = r#"(() => {
  return JSON.stringify(
    Array.from(document.querySelectorAll('img'))
      .map(img => img.src)
      .filter(src => src && src.length > 0)
  );
})()"#;

/// Abstracts the three browser operations that can independently fail,
/// enabling mock injection to cover each error path in tests.
#[cfg_attr(test, mockall::automock)]
pub(crate) trait BrowserOps {
    /// Build config and launch browser.
    async fn start(&mut self) -> Result<(), HtmlError>;

    /// Navigate the launched browser to the given URL.
    async fn open(&mut self, url: &str) -> Result<(), HtmlError>;

    /// Wait `wait_ms` ms, then retrieve the rendered page HTML.
    async fn read(&mut self, wait_ms: u64) -> Result<String, HtmlError>;

    /// Detect all form fields and buttons on the current page.
    async fn detect_fields(&mut self) -> Result<(Vec<FormField>, Vec<FormButton>), HtmlError>;

    /// Fill a form field identified by CSS selector with the given value.
    async fn fill_field(&mut self, selector: &str, value: &FieldValue) -> Result<(), HtmlError>;

    /// Read the current DOM value of a field (returns None if element not found).
    async fn read_field_value(&mut self, selector: &str) -> Result<Option<String>, HtmlError>;

    /// Submit the form on the current page.
    async fn submit_form(&mut self) -> Result<(), HtmlError>;

    /// Click a button matching the given label (case-insensitive partial match).
    async fn click_button(&mut self, label: &str) -> Result<(), HtmlError>;

    /// Find and click a button by exact id first, falling back to label partial match.
    /// Returns the matched button and which strategy resolved it.
    async fn click_button_by_id_or_label(
        &mut self,
        button_id: Option<String>,
        label:     Option<String>,
    ) -> Result<(FormButton, ButtonResolvedBy), HtmlError>;

    /// Extract the absolute src URL of every <img> element on the current page.
    async fn get_image_urls(&mut self) -> Result<Vec<String>, HtmlError>;

    /// Set cookies on the current page via CDP before navigation.
    async fn set_cookies(&mut self, url: &str, cookies: &[Cookie]) -> Result<(), HtmlError>;

    /// Set extra HTTP headers on the current page via CDP before navigation.
    async fn set_extra_headers(&mut self, headers: &HashMap<String, String>) -> Result<(), HtmlError>;
}

// ─── Real implementation ──────────────────────────────────────────────────────

pub(crate) struct ChromiumBrowser {
    stealth: bool,
    browser: Option<Browser>,
    handler_task: Option<tokio::task::JoinHandle<()>>,
    page: Option<chromiumoxide::Page>,
}

impl ChromiumBrowser {
    pub(crate) fn new(stealth: bool) -> Self {
        Self { stealth, browser: None, handler_task: None, page: None }
    }

    pub(crate) async fn cleanup(&mut self) {
        drop(self.page.take());
        if let Some(mut browser) = self.browser.take() {
            browser.close().await.ok();
        }
        if let Some(task) = self.handler_task.take() {
            task.abort();
        }
    }
}

#[derive(serde::Deserialize)]
struct DetectPayload {
    fields: Vec<RawField>,
    buttons: Vec<FormButton>,
}

#[derive(serde::Deserialize)]
struct RawField {
    selector:    String,
    field_type:  String,
    name:        Option<String>,
    id:          Option<String>,
    label:       Option<String>,
    placeholder: Option<String>,
    required:    bool,
    options:     Vec<SelectOption>,
}

fn map_field_type(s: &str) -> FormFieldType {
    match s {
        "email"    => FormFieldType::Email,
        "password" => FormFieldType::Password,
        "select"   => FormFieldType::Select,
        "checkbox" => FormFieldType::Checkbox,
        "textarea" => FormFieldType::Textarea,
        "text"     => FormFieldType::Text,
        _          => FormFieldType::Unknown,
    }
}

impl BrowserOps for ChromiumBrowser {
    async fn start(&mut self) -> Result<(), HtmlError> {
        let unique_id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);

        let config = BrowserConfig::builder()
            .no_sandbox()
            .arg("--disable-dev-shm-usage")
            .arg("--disable-gpu")
            .arg(format!("--user-data-dir=/tmp/mcp-chrome-{}", unique_id))
            .build()
            .map_err(|e| HtmlError::BrowserError(e.to_string()))?;

        let (browser, mut handler) = Browser::launch(config).await
            .map_err(|e| HtmlError::BrowserError(e.to_string()))?;

        self.handler_task = Some(tokio::spawn(async move {
            while handler.next().await.is_some() {}
        }));
        self.browser = Some(browser);
        Ok(())
    }

    async fn open(&mut self, url: &str) -> Result<(), HtmlError> {
        let browser = self.browser.as_ref()
            .ok_or_else(|| HtmlError::BrowserError("browser not started".to_string()))?;
        let page = if self.stealth {
            let p = browser.new_page("about:blank").await
                .map_err(|e| HtmlError::BrowserError(e.to_string()))?;
            p.enable_stealth_mode().await
                .map_err(|e| HtmlError::BrowserError(e.to_string()))?;
            p.goto(url).await
                .map_err(|e| HtmlError::BrowserError(e.to_string()))?;
            p
        } else {
            browser.new_page(url).await
                .map_err(|e| HtmlError::BrowserError(e.to_string()))?
        };
        self.page = Some(page);
        Ok(())
    }

    async fn read(&mut self, wait_ms: u64) -> Result<String, HtmlError> {
        tokio::time::sleep(std::time::Duration::from_millis(wait_ms)).await;
        let page = self.page.as_ref()
            .ok_or_else(|| HtmlError::BrowserError("page not opened".to_string()))?;
        page.content().await
            .map_err(|e| HtmlError::BrowserError(e.to_string()))
    }

    async fn detect_fields(&mut self) -> Result<(Vec<FormField>, Vec<FormButton>), HtmlError> {
        let page = self.page.as_ref()
            .ok_or_else(|| HtmlError::BrowserError("page not opened".into()))?;
        let response = page.evaluate(DETECT_JS).await
            .map_err(|e| HtmlError::BrowserError(e.to_string()))?;
        let json = response.value()
            .and_then(|v| v.as_str().map(|s| s.to_owned()))
            .ok_or_else(|| HtmlError::BrowserError("detect JS returned no string".into()))?;
        let payload: DetectPayload = serde_json::from_str(&json)
            .map_err(|e| HtmlError::BrowserError(format!("parse detect result: {}", e)))?;
        let fields = payload.fields.into_iter().map(|r| FormField {
            selector:    r.selector,
            field_type:  map_field_type(&r.field_type),
            name:        r.name,
            id:          r.id,
            label:       r.label,
            placeholder: r.placeholder,
            required:    r.required,
            options:     r.options,
        }).collect();
        Ok((fields, payload.buttons))
    }

    async fn fill_field(&mut self, selector: &str, value: &FieldValue) -> Result<(), HtmlError> {
        let page = self.page.as_ref()
            .ok_or_else(|| HtmlError::BrowserError("page not opened".into()))?;

        match value {
            FieldValue::Text(s) => {
                let sel_json = serde_json::to_string(selector).unwrap_or_default();
                let clear_js = format!(
                    "(function(sel) {{ var el = document.querySelector(sel); \
                     if (!el) return false; \
                     el.value = ''; \
                     el.dispatchEvent(new Event('input', {{bubbles:true}})); \
                     return true; }})({sel_json})"
                );
                page.evaluate(clear_js).await
                    .map_err(|e| HtmlError::BrowserError(e.to_string()))?;
                let element = page.find_element(selector).await
                    .map_err(|e| HtmlError::BrowserError(e.to_string()))?;
                element.type_str(s).await
                    .map_err(|e| HtmlError::BrowserError(e.to_string()))?;
            }
            FieldValue::Selected(opt) => {
                let sel_json = serde_json::to_string(selector).unwrap_or_default();
                let opt_json = serde_json::to_string(opt).unwrap_or_default();
                let js = format!(
                    "(function(sel, opt) {{ \
                       var el = document.querySelector(sel); \
                       if (!el) return false; \
                       var found = Array.from(el.options).find(function(o) {{ \
                         return o.value.toLowerCase() === opt.toLowerCase() \
                             || o.text.trim().toLowerCase() === opt.toLowerCase(); \
                       }}); \
                       if (!found) return false; \
                       el.value = found.value; \
                       el.dispatchEvent(new Event('change', {{bubbles:true}})); \
                       return true; \
                     }})({sel_json}, {opt_json})"
                );
                let result = page.evaluate(js).await
                    .map_err(|e| HtmlError::BrowserError(e.to_string()))?;
                let ok = result.value()
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                if !ok {
                    return Err(HtmlError::SubmitFailed(
                        format!("could not select option '{}' in '{}'", opt, selector)
                    ));
                }
            }
            FieldValue::Checked(b) => {
                let sel_json = serde_json::to_string(selector).unwrap_or_default();
                let target = if *b { "true" } else { "false" };
                let js = format!(
                    "(function(sel, target) {{ \
                       var el = document.querySelector(sel); \
                       if (!el) return; \
                       if (el.checked !== target) el.click(); \
                     }})({sel_json}, {target})"
                );
                page.evaluate(js).await
                    .map_err(|e| HtmlError::BrowserError(e.to_string()))?;
            }
        }
        Ok(())
    }

    async fn read_field_value(&mut self, selector: &str) -> Result<Option<String>, HtmlError> {
        let page = self.page.as_ref()
            .ok_or_else(|| HtmlError::BrowserError("page not opened".into()))?;
        let sel_json = serde_json::to_string(selector).unwrap_or_default();
        let js = format!(
            "(function(sel) {{ \
               var el = document.querySelector(sel); \
               if (!el) return null; \
               if (el.type === 'checkbox') return String(el.checked); \
               if (el.tagName === 'SELECT') {{ \
                 return el.options[el.selectedIndex] \
                   ? (el.options[el.selectedIndex].text || el.value) \
                   : el.value; \
               }} \
               return el.value; \
             }})({sel_json})"
        );
        let result = page.evaluate(js).await
            .map_err(|e| HtmlError::BrowserError(e.to_string()))?;
        let val = result.value().and_then(|v| {
            if v.is_null() { None } else { v.as_str().map(|s| s.to_owned()) }
        });
        Ok(val)
    }

    async fn submit_form(&mut self) -> Result<(), HtmlError> {
        let page = self.page.as_ref()
            .ok_or_else(|| HtmlError::BrowserError("page not opened".into()))?;
        let js = r#"(function() {
          var sub = document.querySelector('input[type="submit"]')
                 || document.querySelector('button[type="submit"]');
          if (sub) { sub.click(); return true; }
          var form = document.querySelector('form');
          if (form) { form.submit(); return true; }
          return false;
        })()"#;
        let result = page.evaluate(js).await
            .map_err(|e| HtmlError::BrowserError(e.to_string()))?;
        let ok = result.value()
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if !ok {
            return Err(HtmlError::SubmitFailed("no submit button or form found".into()));
        }
        tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
        Ok(())
    }

    async fn click_button(&mut self, label: &str) -> Result<(), HtmlError> {
        let page = self.page.as_ref()
            .ok_or_else(|| HtmlError::BrowserError("page not opened".into()))?;
        let label_json = serde_json::to_string(label).unwrap_or_default();
        let js = format!(
            "(function(lbl) {{ \
               var els = Array.from(document.querySelectorAll(\
                 'button,input[type=\"submit\"],input[type=\"button\"],a[href]')); \
               var found = els.find(function(el) {{ \
                 return el.textContent.toLowerCase().includes(lbl.toLowerCase()); \
               }}); \
               if (!found) return false; \
               found.click(); \
               return true; \
             }})({label_json})"
        );
        let result = page.evaluate(js).await
            .map_err(|e| HtmlError::BrowserError(e.to_string()))?;
        let ok = result.value()
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if !ok {
            return Err(HtmlError::ButtonNotFound(label.to_string()));
        }
        tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
        Ok(())
    }

    async fn get_image_urls(&mut self) -> Result<Vec<String>, HtmlError> {
        let page = self.page.as_ref()
            .ok_or_else(|| HtmlError::BrowserError("page not opened".into()))?;
        let result = page.evaluate(GET_IMAGES_JS).await
            .map_err(|e| HtmlError::BrowserError(e.to_string()))?;
        let json = result.value()
            .and_then(|v| v.as_str().map(|s| s.to_owned()))
            .unwrap_or_else(|| "[]".to_string());
        let urls: Vec<String> = serde_json::from_str(&json).unwrap_or_default();
        Ok(urls)
    }

    async fn set_cookies(&mut self, url: &str, cookies: &[Cookie]) -> Result<(), HtmlError> {
        if cookies.is_empty() {
            return Ok(());
        }
        let page = self.page.as_ref()
            .ok_or_else(|| HtmlError::BrowserError("page not open".into()))?;
        for cookie in cookies {
            let domain = match &cookie.domain {
                Some(d) => d.clone(),
                None    => extract_host(url)?,
            };
            let path = cookie.path.clone().unwrap_or_else(|| "/".to_string());
            let cdp_cookie = CdpCookieParam::builder()
                .name(cookie.name.clone())
                .value(cookie.value.clone())
                .domain(domain)
                .path(path)
                .build()
                .map_err(|e| HtmlError::BrowserError(e.to_string()))?;
            page.set_cookie(cdp_cookie).await
                .map_err(|e| HtmlError::BrowserError(e.to_string()))?;
        }
        Ok(())
    }

    async fn set_extra_headers(&mut self, headers: &HashMap<String, String>) -> Result<(), HtmlError> {
        if headers.is_empty() {
            return Ok(());
        }
        let page = self.page.as_ref()
            .ok_or_else(|| HtmlError::BrowserError("page not open".into()))?;
        use chromiumoxide::cdp::browser_protocol::network::{
            Headers as CdpHeaders, SetExtraHttpHeadersParams,
        };
        let json_obj: serde_json::Map<String, serde_json::Value> = headers
            .iter()
            .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
            .collect();
        page.execute(SetExtraHttpHeadersParams { headers: CdpHeaders::new(serde_json::Value::Object(json_obj)) })
            .await
            .map_err(|e| HtmlError::BrowserError(e.to_string()))?;
        Ok(())
    }

    async fn click_button_by_id_or_label(
        &mut self,
        button_id: Option<String>,
        label:     Option<String>,
    ) -> Result<(FormButton, ButtonResolvedBy), HtmlError> {
        let page = self.page.as_ref()
            .ok_or_else(|| HtmlError::BrowserError("page not opened".into()))?;

        if let Some(id) = button_id.as_deref() {
            let id_json = serde_json::to_string(id).unwrap_or_default();
            let js = format!(
                "(function(id) {{ \
                   var el = document.getElementById(id); \
                   if (!el) return null; \
                   var tag = el.tagName.toLowerCase(); \
                   var tp = (el.type || '').toLowerCase(); \
                   var clickable = tag === 'button' || tag === 'a' \
                     || (tag === 'input' && (tp === 'submit' || tp === 'button')); \
                   if (!clickable) return null; \
                   el.click(); \
                   return el.textContent.trim().replace(/\\s+/g,' ') || id; \
                 }})({id_json})"
            );
            let result = page.evaluate(js).await
                .map_err(|e| HtmlError::BrowserError(e.to_string()))?;
            match result.value().and_then(|v| v.as_str().map(|s| s.to_owned())) {
                Some(text) => {
                    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
                    return Ok((
                        FormButton { label: text, selector: format!("#{}", id) },
                        ButtonResolvedBy::Id,
                    ));
                }
                None if label.is_none() => {
                    return Err(HtmlError::ButtonNotFound(format!("by id '{}'", id)));
                }
                None => {}
            }
        }

        if let Some(lbl) = label {
            let lbl_json = serde_json::to_string(&lbl).unwrap_or_default();
            let js = format!(
                "(function(lbl) {{ \
                   var els = Array.from(document.querySelectorAll(\
                     'button,input[type=\"submit\"],input[type=\"button\"],a[href]')); \
                   var found = els.find(function(el) {{ \
                     return el.textContent.toLowerCase().includes(lbl.toLowerCase()); \
                   }}); \
                   if (!found) return null; \
                   var text = found.textContent.trim().replace(/\\s+/g,' '); \
                   var sel = found.id ? '#' + CSS.escape(found.id) \
                     : found.tagName.toLowerCase(); \
                   found.click(); \
                   return JSON.stringify({{ label: text, selector: sel }}); \
                 }})({lbl_json})"
            );
            let result = page.evaluate(js).await
                .map_err(|e| HtmlError::BrowserError(e.to_string()))?;
            match result.value().and_then(|v| v.as_str().map(|s| s.to_owned())) {
                Some(json) => {
                    let btn: FormButton = serde_json::from_str(&json)
                        .map_err(|e| HtmlError::BrowserError(e.to_string()))?;
                    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
                    return Ok((btn, ButtonResolvedBy::Label));
                }
                None => {
                    let msg = match button_id {
                        Some(id) => format!("by id '{}' or label '{}'", id, lbl),
                        None     => format!("by label '{}'", lbl),
                    };
                    return Err(HtmlError::ButtonNotFound(msg));
                }
            }
        }

        Err(HtmlError::ButtonNotFound("button_id and label are both None".into()))
    }
}

// ─── Public API ───────────────────────────────────────────────────────────────

fn extract_host(url: &str) -> Result<String, HtmlError> {
    let without_scheme = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .ok_or_else(|| HtmlError::InvalidUrl("URL missing http/https scheme".to_string()))?;
    let host = without_scheme
        .split('/')
        .next()
        .unwrap_or(without_scheme)
        .split('?')
        .next()
        .unwrap_or(without_scheme)
        .split(':')
        .next()
        .unwrap_or(without_scheme);
    if host.is_empty() {
        return Err(HtmlError::InvalidUrl("URL has no host".to_string()));
    }
    Ok(host.to_string())
}

pub(crate) async fn apply_request_options(
    url:     &str,
    options: Option<&RequestOptions>,
    ops:     &mut impl BrowserOps,
) -> Result<(), HtmlError> {
    let Some(opts) = options else { return Ok(()) };
    if let Some(cookies) = &opts.cookies {
        ops.set_cookies(url, cookies).await?;
    }
    if let Some(headers) = &opts.headers {
        ops.set_extra_headers(headers).await?;
    }
    Ok(())
}

pub async fn fetch_rendered_html(
    url:     &str,
    wait_ms: u64,
    stealth: bool,
    options: Option<&RequestOptions>,
) -> Result<String, HtmlError> {
    let mut driver = ChromiumBrowser::new(stealth);
    let result = fetch_with_ops(url, wait_ms, options, &mut driver).await;
    driver.cleanup().await;
    result
}

/// Testable core: drives `ops` through start → apply_request_options → open → read.
pub(crate) async fn fetch_with_ops(
    url:     &str,
    wait_ms: u64,
    options: Option<&RequestOptions>,
    ops:     &mut impl BrowserOps,
) -> Result<String, HtmlError> {
    ops.start().await?;
    apply_request_options(url, options, ops).await?;
    ops.open(url).await?;
    ops.read(wait_ms).await
}

pub async fn fetch_rendered_html_with_images(
    url:     &str,
    wait_ms: u64,
    stealth: bool,
    options: Option<&RequestOptions>,
) -> Result<(String, Vec<String>), HtmlError> {
    let mut driver = ChromiumBrowser::new(stealth);
    let result = fetch_with_images_ops(url, wait_ms, options, &mut driver).await;
    driver.cleanup().await;
    result
}

/// Testable core: start → apply_request_options → open → read → get_image_urls.
pub(crate) async fn fetch_with_images_ops(
    url:     &str,
    wait_ms: u64,
    options: Option<&RequestOptions>,
    ops:     &mut impl BrowserOps,
) -> Result<(String, Vec<String>), HtmlError> {
    ops.start().await?;
    apply_request_options(url, options, ops).await?;
    ops.open(url).await?;
    let html = ops.read(wait_ms).await?;
    let image_urls = ops.get_image_urls().await?;
    Ok((html, image_urls))
}

/// Testable core: start → open → read → detect_fields.
#[cfg(test)]
pub(crate) async fn form_with_ops(
    url:    &str,
    wait_ms: u64,
    ops:     &mut impl BrowserOps,
) -> Result<(Vec<FormField>, Vec<FormButton>), HtmlError> {
    ops.start().await?;
    ops.open(url).await?;
    ops.read(wait_ms).await?;
    ops.detect_fields().await
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_start_error_propagates() {
        let mut mock = MockBrowserOps::new();
        mock.expect_start()
            .returning(|| Err(HtmlError::BrowserError("config build failed".to_string())));

        let result = fetch_with_ops("https://example.com", 0, None, &mut mock).await;

        assert!(matches!(result, Err(HtmlError::BrowserError(_))));
    }

    #[tokio::test]
    async fn test_open_error_propagates() {
        let mut mock = MockBrowserOps::new();
        mock.expect_start().returning(|| Ok(()));
        mock.expect_open()
            .returning(|_| Err(HtmlError::BrowserError("new_page failed".to_string())));

        let result = fetch_with_ops("https://example.com", 0, None, &mut mock).await;

        assert!(matches!(result, Err(HtmlError::BrowserError(_))));
    }

    #[tokio::test]
    async fn test_read_error_propagates() {
        let mut mock = MockBrowserOps::new();
        mock.expect_start().returning(|| Ok(()));
        mock.expect_open().returning(|_| Ok(()));
        mock.expect_read()
            .returning(|_| Err(HtmlError::BrowserError("page content failed".to_string())));

        let result = fetch_with_ops("https://example.com", 0, None, &mut mock).await;

        assert!(matches!(result, Err(HtmlError::BrowserError(_))));
    }

    #[tokio::test]
    async fn test_detect_fields_error_propagates() {
        let mut mock = MockBrowserOps::new();
        mock.expect_start().returning(|| Ok(()));
        mock.expect_open().returning(|_| Ok(()));
        mock.expect_read().returning(|_| Ok("<html/>".to_string()));
        mock.expect_detect_fields()
            .returning(|| Err(HtmlError::BrowserError("detect failed".to_string())));

        let result = form_with_ops("https://example.com", 0, &mut mock).await;

        assert!(matches!(result, Err(HtmlError::BrowserError(_))));
    }

    #[tokio::test]
    async fn test_fill_field_error_propagates() {
        let mut mock = MockBrowserOps::new();
        mock.expect_fill_field()
            .returning(|_, _| Err(HtmlError::FieldNotFound("email".to_string())));

        let result = mock.fill_field("#email", &FieldValue::Text("x".to_string())).await;

        assert!(matches!(result, Err(HtmlError::FieldNotFound(_))));
    }

    #[tokio::test]
    async fn test_submit_form_error_propagates() {
        let mut mock = MockBrowserOps::new();
        mock.expect_submit_form()
            .returning(|| Err(HtmlError::SubmitFailed("no form".to_string())));

        let result = mock.submit_form().await;

        assert!(matches!(result, Err(HtmlError::SubmitFailed(_))));
    }

    #[tokio::test]
    async fn test_click_button_error_propagates() {
        let mut mock = MockBrowserOps::new();
        mock.expect_click_button()
            .returning(|_| Err(HtmlError::ButtonNotFound("Next".to_string())));

        let result = mock.click_button("Next").await;

        assert!(matches!(result, Err(HtmlError::ButtonNotFound(_))));
    }

    #[tokio::test]
    async fn test_read_field_value_error_propagates() {
        let mut mock = MockBrowserOps::new();
        mock.expect_read_field_value()
            .returning(|_| Err(HtmlError::BrowserError("evaluate failed".to_string())));

        let result = mock.read_field_value("#email").await;

        assert!(matches!(result, Err(HtmlError::BrowserError(_))));
    }

    #[tokio::test]
    async fn test_click_by_id_error_propagates() {
        let mut mock = MockBrowserOps::new();
        mock.expect_click_button_by_id_or_label()
            .returning(|_, _| Err(HtmlError::ButtonNotFound("by id 'submit-btn'".to_string())));

        let result = mock.click_button_by_id_or_label(Some("submit-btn".to_string()), None).await;

        assert!(matches!(result, Err(HtmlError::ButtonNotFound(_))));
    }

    #[tokio::test]
    async fn test_click_by_label_fallback_error_propagates() {
        let mut mock = MockBrowserOps::new();
        mock.expect_click_button_by_id_or_label()
            .returning(|_, _| Err(HtmlError::ButtonNotFound("by label 'Continue'".to_string())));

        let result = mock.click_button_by_id_or_label(None, Some("Continue".to_string())).await;

        assert!(matches!(result, Err(HtmlError::ButtonNotFound(_))));
    }

    #[tokio::test]
    async fn test_click_both_none_error_propagates() {
        let mut mock = MockBrowserOps::new();
        mock.expect_click_button_by_id_or_label()
            .returning(|_, _| Err(HtmlError::ButtonNotFound("button_id and label are both None".to_string())));

        let result = mock.click_button_by_id_or_label(None::<String>, None::<String>).await;

        assert!(matches!(result, Err(HtmlError::ButtonNotFound(_))));
    }

    #[tokio::test]
    async fn test_detect_aria_label_button_flows_through() {
        let mut mock = MockBrowserOps::new();
        mock.expect_start().returning(|| Ok(()));
        mock.expect_open().returning(|_| Ok(()));
        mock.expect_read().returning(|_| Ok("<html/>".to_string()));
        mock.expect_detect_fields().returning(|| Ok((
            vec![],
            vec![FormButton { label: "Search".to_string(), selector: "#search-btn".to_string() }],
        )));

        let result = form_with_ops("https://example.com", 0, &mut mock).await;

        assert!(result.is_ok());
        let (_, buttons) = result.unwrap();
        assert_eq!(buttons.len(), 1);
        assert_eq!(buttons[0].label, "Search");
        assert_eq!(buttons[0].selector, "#search-btn");
    }

    #[tokio::test]
    async fn test_detect_svg_title_button_flows_through() {
        let mut mock = MockBrowserOps::new();
        mock.expect_start().returning(|| Ok(()));
        mock.expect_open().returning(|_| Ok(()));
        mock.expect_read().returning(|_| Ok("<html/>".to_string()));
        mock.expect_detect_fields().returning(|| Ok((
            vec![],
            vec![FormButton { label: "Close".to_string(), selector: "button:nth-of-type(1)".to_string() }],
        )));

        let result = form_with_ops("https://example.com", 0, &mut mock).await;

        assert!(result.is_ok());
        let (_, buttons) = result.unwrap();
        assert_eq!(buttons.len(), 1);
        assert_eq!(buttons[0].label, "Close");
    }

    #[tokio::test]
    async fn test_detect_img_alt_button_flows_through() {
        let mut mock = MockBrowserOps::new();
        mock.expect_start().returning(|| Ok(()));
        mock.expect_open().returning(|_| Ok(()));
        mock.expect_read().returning(|_| Ok("<html/>".to_string()));
        mock.expect_detect_fields().returning(|| Ok((
            vec![],
            vec![FormButton { label: "Cart".to_string(), selector: "a:nth-of-type(1)".to_string() }],
        )));

        let result = form_with_ops("https://example.com", 0, &mut mock).await;

        assert!(result.is_ok());
        let (_, buttons) = result.unwrap();
        assert_eq!(buttons.len(), 1);
        assert_eq!(buttons[0].label, "Cart");
    }

    #[tokio::test]
    async fn test_detect_no_label_button_excluded() {
        let mut mock = MockBrowserOps::new();
        mock.expect_start().returning(|| Ok(()));
        mock.expect_open().returning(|_| Ok(()));
        mock.expect_read().returning(|_| Ok("<html/>".to_string()));
        mock.expect_detect_fields().returning(|| Ok((vec![], vec![])));

        let result = form_with_ops("https://example.com", 0, &mut mock).await;

        assert!(result.is_ok());
        let (_, buttons) = result.unwrap();
        assert!(buttons.is_empty());
    }

    #[tokio::test]
    async fn test_set_cookies_error_propagates_before_open() {
        let mut mock = MockBrowserOps::new();
        mock.expect_start().returning(|| Ok(()));
        mock.expect_set_cookies()
            .returning(|_, _| Err(HtmlError::BrowserError("set_cookies failed".to_string())));

        let opts = RequestOptions {
            cookies: Some(vec![Cookie {
                name:   "s".to_string(),
                value:  "v".to_string(),
                domain: None,
                path:   None,
            }]),
            headers: None,
        };
        let result = fetch_with_ops("https://example.com", 0, Some(&opts), &mut mock).await;
        assert!(matches!(result, Err(HtmlError::BrowserError(_))));
    }

    #[tokio::test]
    async fn test_set_extra_headers_error_propagates_before_open() {
        let mut mock = MockBrowserOps::new();
        mock.expect_start().returning(|| Ok(()));
        mock.expect_set_extra_headers()
            .returning(|_| Err(HtmlError::BrowserError("set_headers failed".to_string())));

        let opts = RequestOptions {
            cookies: None,
            headers: Some({
                let mut m = HashMap::new();
                m.insert("Authorization".to_string(), "Bearer tok".to_string());
                m
            }),
        };
        let result = fetch_with_ops("https://example.com", 0, Some(&opts), &mut mock).await;
        assert!(matches!(result, Err(HtmlError::BrowserError(_))));
    }

    #[tokio::test]
    async fn test_apply_request_options_none_calls_nothing() {
        let mut mock = MockBrowserOps::new();
        mock.expect_set_cookies().never();
        mock.expect_set_extra_headers().never();

        let result = apply_request_options("https://example.com", None, &mut mock).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_apply_request_options_only_cookies() {
        let mut mock = MockBrowserOps::new();
        mock.expect_set_cookies().returning(|_, _| Ok(()));
        mock.expect_set_extra_headers().never();

        let opts = RequestOptions {
            cookies: Some(vec![Cookie {
                name:   "k".to_string(),
                value:  "v".to_string(),
                domain: None,
                path:   None,
            }]),
            headers: None,
        };
        let result = apply_request_options("https://example.com", Some(&opts), &mut mock).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_apply_request_options_only_headers() {
        let mut mock = MockBrowserOps::new();
        mock.expect_set_cookies().never();
        mock.expect_set_extra_headers().returning(|_| Ok(()));

        let opts = RequestOptions {
            cookies: None,
            headers: Some({
                let mut m = HashMap::new();
                m.insert("Accept-Language".to_string(), "fr".to_string());
                m
            }),
        };
        let result = apply_request_options("https://example.com", Some(&opts), &mut mock).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_apply_request_options_both_called_in_order() {
        let mut mock = MockBrowserOps::new();
        let mut seq = mockall::Sequence::new();
        mock.expect_set_cookies()
            .times(1)
            .in_sequence(&mut seq)
            .returning(|_, _| Ok(()));
        mock.expect_set_extra_headers()
            .times(1)
            .in_sequence(&mut seq)
            .returning(|_| Ok(()));

        let opts = RequestOptions {
            cookies: Some(vec![Cookie {
                name:   "k".to_string(),
                value:  "v".to_string(),
                domain: None,
                path:   None,
            }]),
            headers: Some({
                let mut m = HashMap::new();
                m.insert("X-Api-Key".to_string(), "secret".to_string());
                m
            }),
        };
        let result = apply_request_options("https://example.com", Some(&opts), &mut mock).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_get_image_urls_error_propagates() {
        let mut mock = MockBrowserOps::new();
        mock.expect_get_image_urls()
            .returning(|| Err(HtmlError::BrowserError("evaluate failed".to_string())));

        let result = mock.get_image_urls().await;

        assert!(matches!(result, Err(HtmlError::BrowserError(_))));
    }

    #[tokio::test]
    async fn test_fetch_with_images_ops_success() {
        let mut mock = MockBrowserOps::new();
        mock.expect_start().returning(|| Ok(()));
        mock.expect_open().returning(|_| Ok(()));
        mock.expect_read().returning(|_| Ok("<html><img src='https://example.com/a.png'/></html>".to_string()));
        mock.expect_get_image_urls()
            .returning(|| Ok(vec!["https://example.com/a.png".to_string()]));

        let result = fetch_with_images_ops("https://example.com", 0, None, &mut mock).await;

        assert!(result.is_ok());
        let (html, urls) = result.unwrap();
        assert!(html.contains("<img"));
        assert_eq!(urls, vec!["https://example.com/a.png"]);
    }

    #[tokio::test]
    async fn test_fetch_with_images_ops_no_images() {
        let mut mock = MockBrowserOps::new();
        mock.expect_start().returning(|| Ok(()));
        mock.expect_open().returning(|_| Ok(()));
        mock.expect_read().returning(|_| Ok("<html><p>No images here.</p></html>".to_string()));
        mock.expect_get_image_urls().returning(|| Ok(vec![]));

        let result = fetch_with_images_ops("https://example.com", 0, None, &mut mock).await;

        assert!(result.is_ok());
        let (_, urls) = result.unwrap();
        assert!(urls.is_empty());
    }

    #[tokio::test]
    async fn test_fetch_with_images_ops_get_image_urls_error() {
        let mut mock = MockBrowserOps::new();
        mock.expect_start().returning(|| Ok(()));
        mock.expect_open().returning(|_| Ok(()));
        mock.expect_read().returning(|_| Ok("<html/>".to_string()));
        mock.expect_get_image_urls()
            .returning(|| Err(HtmlError::BrowserError("evaluate failed".to_string())));

        let result = fetch_with_images_ops("https://example.com", 0, None, &mut mock).await;

        assert!(matches!(result, Err(HtmlError::BrowserError(_))));
    }

    #[tokio::test]
    async fn test_fetch_with_images_ops_read_error() {
        let mut mock = MockBrowserOps::new();
        mock.expect_start().returning(|| Ok(()));
        mock.expect_open().returning(|_| Ok(()));
        mock.expect_read()
            .returning(|_| Err(HtmlError::BrowserError("content failed".to_string())));

        let result = fetch_with_images_ops("https://example.com", 0, None, &mut mock).await;

        assert!(matches!(result, Err(HtmlError::BrowserError(_))));
    }

    #[test]
    fn extract_host_https() {
        assert_eq!(extract_host("https://example.com/path?q=1").unwrap(), "example.com");
    }

    #[test]
    fn extract_host_http_with_port() {
        assert_eq!(extract_host("http://example.com:8080/path").unwrap(), "example.com");
    }

    #[test]
    fn extract_host_invalid_scheme() {
        assert!(extract_host("ftp://example.com").is_err());
    }
}
