use chromiumoxide::Browser;
use crate::types::HtmlError;
use std::sync::OnceLock;
use futures::StreamExt;

pub(crate) struct SessionState {
    pub browser: Browser,
    pub handler: tokio::task::JoinHandle<()>,
}

static SESSION: OnceLock<tokio::sync::Mutex<Option<SessionState>>> = OnceLock::new();

pub(crate) fn session_lock() -> &'static tokio::sync::Mutex<Option<SessionState>> {
    SESSION.get_or_init(|| tokio::sync::Mutex::new(None))
}

pub(crate) async fn get_or_init_session() -> Result<tokio::sync::MutexGuard<'static, Option<SessionState>>, HtmlError> {
    let guard = session_lock().lock().await;

    if guard.is_none() {
        drop(guard);

        let unique_id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);

        // Create a unique temp directory for this Chrome instance to avoid lock conflicts
        let temp_dir = format!("/tmp/mcp-chrome-session-{}", unique_id);
        let _ = std::fs::create_dir_all(&temp_dir);

        let config = chromiumoxide::BrowserConfig::builder()
            .no_sandbox()
            .arg("--disable-dev-shm-usage")
            .arg("--disable-gpu")
            .arg(format!("--user-data-dir={}", temp_dir))
            .build()
            .map_err(|e| HtmlError::SessionUnavailable(
                format!("Failed to configure browser: {}", e)
            ))?;

        let (browser, mut handler) = Browser::launch(config)
            .await
            .map_err(|e| HtmlError::SessionUnavailable(
                format!("Failed to launch browser session: {}", e)
            ))?;

        let handler_task = tokio::spawn(async move {
            while handler.next().await.is_some() {}
        });

        let mut guard = session_lock().lock().await;
        *guard = Some(SessionState {
            browser,
            handler: handler_task
        });
        Ok(guard)
    } else {
        Ok(guard)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_lock_initializes() {
        let lock = session_lock();
        assert!(lock.blocking_lock().is_none(), "Session should start as None");
    }
}
