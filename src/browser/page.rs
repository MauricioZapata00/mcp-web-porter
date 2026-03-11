use chromiumoxide::{Browser, BrowserConfig};
use futures::StreamExt;
use crate::types::{FieldValue, FormButton, FormField, FormFieldType, HtmlError, SelectOption};

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
  const buttons = Array.from(
    document.querySelectorAll('button,input[type="submit"],input[type="button"],a[href]')
  ).filter(el => el.textContent.trim()).map((el, i) => ({
    label: el.textContent.trim().replace(/\s+/g,' '),
    selector: el.id ? '#' + CSS.escape(el.id) : el.tagName.toLowerCase() + ':nth-of-type(' + (i+1) + ')',
  }));
  return JSON.stringify({fields, buttons});
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
}

// ─── Public API ───────────────────────────────────────────────────────────────

pub async fn fetch_rendered_html(url: &str, wait_ms: u64, stealth: bool) -> Result<String, HtmlError> {
    let mut driver = ChromiumBrowser::new(stealth);
    let result = fetch_with_ops(url, wait_ms, &mut driver).await;
    driver.cleanup().await;
    result
}

/// Testable core: drives `ops` through start → open → read.
pub(crate) async fn fetch_with_ops(
    url: &str,
    wait_ms: u64,
    ops: &mut impl BrowserOps,
) -> Result<String, HtmlError> {
    ops.start().await?;
    ops.open(url).await?;
    ops.read(wait_ms).await
}

/// Testable core: start → open → read → detect_fields.
#[cfg(test)]
pub(crate) async fn form_with_ops(
    url: &str,
    wait_ms: u64,
    ops: &mut impl BrowserOps,
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

        let result = fetch_with_ops("https://example.com", 0, &mut mock).await;

        assert!(matches!(result, Err(HtmlError::BrowserError(_))));
    }

    #[tokio::test]
    async fn test_open_error_propagates() {
        let mut mock = MockBrowserOps::new();
        mock.expect_start().returning(|| Ok(()));
        mock.expect_open()
            .returning(|_| Err(HtmlError::BrowserError("new_page failed".to_string())));

        let result = fetch_with_ops("https://example.com", 0, &mut mock).await;

        assert!(matches!(result, Err(HtmlError::BrowserError(_))));
    }

    #[tokio::test]
    async fn test_read_error_propagates() {
        let mut mock = MockBrowserOps::new();
        mock.expect_start().returning(|| Ok(()));
        mock.expect_open().returning(|_| Ok(()));
        mock.expect_read()
            .returning(|_| Err(HtmlError::BrowserError("page content failed".to_string())));

        let result = fetch_with_ops("https://example.com", 0, &mut mock).await;

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
}
