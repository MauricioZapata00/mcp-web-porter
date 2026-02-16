use super::error::HtmlError;

#[derive(Debug, Clone)]
pub struct ResourceUri {
    url: String,
}

impl ResourceUri {
    pub fn parse(uri: &str) -> Result<Self, HtmlError> {
        if uri.is_empty() {
            return Err(HtmlError::InvalidUrl("URL cannot be empty".to_string()));
        }

        if !uri.starts_with("http://") && !uri.starts_with("https://") {
            return Err(HtmlError::InvalidUrl(
                "URL must start with http:// or https://".to_string(),
            ));
        }

        Ok(Self {
            url: uri.to_string(),
        })
    }

    pub fn url(&self) -> &str {
        &self.url
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_uri_parse_valid_https() {
        let result = ResourceUri::parse("https://example.com");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().url(), "https://example.com");
    }

    #[test]
    fn test_resource_uri_parse_valid_http() {
        let result = ResourceUri::parse("http://example.com");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().url(), "http://example.com");
    }

    #[test]
    fn test_resource_uri_parse_with_path() {
        let result = ResourceUri::parse("https://example.com/path/to/page");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().url(), "https://example.com/path/to/page");
    }

    #[test]
    fn test_resource_uri_parse_with_query_params() {
        let result = ResourceUri::parse("https://example.com/page?param=value&other=123");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().url(), "https://example.com/page?param=value&other=123");
    }

    #[test]
    fn test_resource_uri_parse_invalid_scheme() {
        let result = ResourceUri::parse("ftp://example.com");
        assert!(result.is_err());
        match result {
            Err(HtmlError::InvalidUrl(msg)) => {
                assert!(msg.contains("must start with http://"));
            }
            _ => panic!("Expected InvalidUrl error"),
        }
    }

    #[test]
    fn test_resource_uri_parse_empty_url() {
        let result = ResourceUri::parse("");
        assert!(result.is_err());
        match result {
            Err(HtmlError::InvalidUrl(msg)) => {
                assert!(msg.contains("cannot be empty"));
            }
            _ => panic!("Expected InvalidUrl error for empty URL"),
        }
    }

    #[test]
    fn test_resource_uri_parse_malformed_url() {
        let result = ResourceUri::parse("not-a-valid-url");
        assert!(result.is_err());
        match result {
            Err(HtmlError::InvalidUrl(msg)) => {
                assert!(msg.contains("must start with http://"));
            }
            _ => panic!("Expected InvalidUrl error"),
        }
    }
}
