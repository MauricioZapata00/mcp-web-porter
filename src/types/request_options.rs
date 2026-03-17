use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Cookie {
    pub name:   String,
    pub value:  String,
    pub domain: Option<String>,
    pub path:   Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct RequestOptions {
    pub cookies: Option<Vec<Cookie>>,
    pub headers: Option<HashMap<String, String>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cookie_with_no_optional_fields_constructs() {
        let c = Cookie {
            name:   "session".to_string(),
            value:  "abc".to_string(),
            domain: None,
            path:   None,
        };
        assert_eq!(c.name, "session");
        assert_eq!(c.value, "abc");
        assert!(c.domain.is_none());
        assert!(c.path.is_none());
    }

    #[test]
    fn cookie_with_all_fields_constructs() {
        let c = Cookie {
            name:   "auth".to_string(),
            value:  "token123".to_string(),
            domain: Some("example.com".to_string()),
            path:   Some("/api".to_string()),
        };
        assert_eq!(c.domain.as_deref(), Some("example.com"));
        assert_eq!(c.path.as_deref(), Some("/api"));
    }

    #[test]
    fn request_options_default_has_none_fields() {
        let opts = RequestOptions::default();
        assert!(opts.cookies.is_none());
        assert!(opts.headers.is_none());
    }
}
