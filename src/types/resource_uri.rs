use super::error::HtmlError;

#[derive(Debug, Clone)]
pub struct ResourceUri {
    url: String,
}

impl ResourceUri {
    pub fn parse(uri: &str) -> Result<Self, HtmlError> {
        if !uri.starts_with("html:///") {
            return Err(HtmlError::InvalidUrl(
                "URI must start with html:///".to_string(),
            ));
        }

        let url = uri.strip_prefix("html:///").unwrap();
        if url.is_empty() {
            return Err(HtmlError::InvalidUrl("URL cannot be empty".to_string()));
        }

        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Err(HtmlError::InvalidUrl(
                "URL must start with http:// or https://".to_string(),
            ));
        }

        Ok(Self {
            url: url.to_string(),
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
    fn test_resource_uri_parse_valid() {
        let result = ResourceUri::parse("html:///https://example.com");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().url(), "https://example.com");
    }

    #[test]
    fn test_resource_uri_parse_with_path() {
        let result = ResourceUri::parse("html:///https://example.com/path/to/page");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().url(), "https://example.com/path/to/page");
    }

    #[test]
    fn test_resource_uri_parse_invalid_scheme() {
        let result = ResourceUri::parse("http:///https://example.com");
        assert!(result.is_err());
    }

    #[test]
    fn test_resource_uri_parse_empty_url() {
        let result = ResourceUri::parse("html:///");
        assert!(result.is_err());
    }

    #[test]
    fn test_resource_uri_parse_malformed_url() {
        let result = ResourceUri::parse("html:///not-a-valid-url");
        assert!(result.is_err());
    }
}
