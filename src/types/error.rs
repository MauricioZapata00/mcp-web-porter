use std::fmt;

#[derive(Debug)]
pub enum HtmlError {
    FetchError(String),
    InvalidUrl(String),
    HttpError(u16),
    BrowserError(String),
    FieldNotFound(String),
    InvalidFieldValue { field: String, reason: String },
    SubmitFailed(String),
    ButtonNotFound(String),
    CookieExtractionError(String),
    SessionUnavailable(String),
    NoActions,
    TooManyActions { count: usize, max: usize },
}

impl fmt::Display for HtmlError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            HtmlError::FetchError(msg) => write!(f, "Failed to fetch URL: {}", msg),
            HtmlError::InvalidUrl(msg) => write!(f, "Invalid URL: {}", msg),
            HtmlError::HttpError(status) => write!(f, "HTTP error: {}", status),
            HtmlError::BrowserError(msg) => write!(f, "Browser error: {}", msg),
            HtmlError::FieldNotFound(n) => write!(f, "Field not found: {}", n),
            HtmlError::InvalidFieldValue { field, reason } => {
                write!(f, "Invalid value for field '{}': {}", field, reason)
            }
            HtmlError::SubmitFailed(m) => write!(f, "Form submission failed: {}", m),
            HtmlError::ButtonNotFound(l) => write!(f, "Button not found: '{}'", l),
            HtmlError::CookieExtractionError(msg) => write!(f, "{}", msg),
            HtmlError::SessionUnavailable(msg) => write!(f, "{}", msg),
            HtmlError::NoActions => write!(f, "actions list must not be empty"),
            HtmlError::TooManyActions { count, max } => {
                write!(f, "actions list has {} items, exceeds max of {}", count, max)
            }
        }
    }
}

impl std::error::Error for HtmlError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_field_not_found_display() {
        let e = HtmlError::FieldNotFound("email".to_string());
        assert!(e.to_string().contains("email"));
    }

    #[test]
    fn test_invalid_field_value_display() {
        let e = HtmlError::InvalidFieldValue {
            field: "email".to_string(),
            reason: "not a valid email".to_string(),
        };
        let s = e.to_string();
        assert!(s.contains("email"));
        assert!(s.contains("not a valid email"));
    }

    #[test]
    fn test_submit_failed_display() {
        let e = HtmlError::SubmitFailed("no submit button".to_string());
        assert!(e.to_string().contains("no submit button"));
    }

    #[test]
    fn test_button_not_found_display() {
        let e = HtmlError::ButtonNotFound("Continue".to_string());
        assert!(e.to_string().contains("Continue"));
    }

    #[test]
    fn test_no_actions_display() {
        let e = HtmlError::NoActions;
        assert!(e.to_string().contains("empty"));
    }

    #[test]
    fn test_too_many_actions_display() {
        let e = HtmlError::TooManyActions { count: 42, max: 20 };
        let s = e.to_string();
        assert!(s.contains("42"));
        assert!(s.contains("20"));
    }
}
