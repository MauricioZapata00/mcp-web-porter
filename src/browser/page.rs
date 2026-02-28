use chromiumoxide::{Browser, BrowserConfig};
use futures::StreamExt;
use crate::types::HtmlError;

/// Abstracts the three browser operations that can independently fail,
/// enabling mock injection to cover each error path in tests.
#[cfg_attr(test, mockall::automock)]
pub(crate) trait BrowserOps {
    /// Build config and launch browser. Covers the error path at L13 (config
    /// build failure) and the equivalent launch failure.
    async fn start(&mut self) -> Result<(), HtmlError>;

    /// Navigate the launched browser to the given URL. Covers the error path
    /// at L27 (new_page failure).
    async fn open(&mut self, url: &str) -> Result<(), HtmlError>;

    /// Wait `wait_ms` ms, then retrieve the rendered page HTML. Covers the
    /// error path at L34 (page.content() failure).
    async fn read(&mut self, wait_ms: u64) -> Result<String, HtmlError>;
}

// ─── Real implementation ──────────────────────────────────────────────────────

struct ChromiumBrowser {
    stealth: bool,
    browser: Option<Browser>,
    handler_task: Option<tokio::task::JoinHandle<()>>,
    page: Option<chromiumoxide::Page>,
}

impl ChromiumBrowser {
    fn new(stealth: bool) -> Self {
        Self { stealth, browser: None, handler_task: None, page: None }
    }

    async fn cleanup(&mut self) {
        drop(self.page.take());
        if let Some(mut browser) = self.browser.take() {
            browser.close().await.ok();
        }
        if let Some(task) = self.handler_task.take() {
            task.abort();
        }
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

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Covers the error branch that wraps BrowserConfig::build() failure (L13).
    #[tokio::test]
    async fn test_start_error_propagates() {
        let mut mock = MockBrowserOps::new();
        mock.expect_start()
            .returning(|| Err(HtmlError::BrowserError("config build failed".to_string())));

        let result = fetch_with_ops("https://example.com", 0, &mut mock).await;

        assert!(matches!(result, Err(HtmlError::BrowserError(_))));
    }

    /// Covers the error branch that wraps browser.new_page() failure (L27).
    #[tokio::test]
    async fn test_open_error_propagates() {
        let mut mock = MockBrowserOps::new();
        mock.expect_start().returning(|| Ok(()));
        mock.expect_open()
            .returning(|_| Err(HtmlError::BrowserError("new_page failed".to_string())));

        let result = fetch_with_ops("https://example.com", 0, &mut mock).await;

        assert!(matches!(result, Err(HtmlError::BrowserError(_))));
    }

    /// Covers the error branch that wraps page.content() failure (L34).
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
}
