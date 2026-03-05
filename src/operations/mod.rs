mod fetch;
mod parse;
pub mod validate;

pub use fetch::fetch_html;
pub use fetch::fetch_html_with_options;
pub use parse::extract_text;
pub use validate::validate_inputs;
pub(crate) use validate::resolve_field;
