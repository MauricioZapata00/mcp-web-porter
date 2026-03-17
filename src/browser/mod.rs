pub(crate) mod page;

pub use page::fetch_rendered_html;
pub(crate) use page::{apply_request_options, BrowserOps, ChromiumBrowser};
