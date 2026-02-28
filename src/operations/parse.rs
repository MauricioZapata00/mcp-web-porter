use scraper::{ElementRef, Html, Selector};

pub fn extract_text(html: &str) -> String {
    let document = Html::parse_document(html);
    let content_selector = Selector::parse(
        "title, h1, h2, h3, h4, h5, h6, p, li, blockquote, pre, td, th"
    ).unwrap();

    let mut lines: Vec<String> = Vec::new();

    for element in document.select(&content_selector) {
        if is_inside_noise(&element) {
            continue;
        }
        let raw: String = element.text().collect::<Vec<_>>().join(" ");
        let trimmed = raw.split_whitespace().collect::<Vec<_>>().join(" ");
        if trimmed.is_empty() {
            continue;
        }
        let line = match element.value().name() {
            "title" | "h1"      => format!("# {}", trimmed),
            "h2"                => format!("## {}", trimmed),
            "h3"                => format!("### {}", trimmed),
            "h4" | "h5" | "h6" => format!("#### {}", trimmed),
            "li"                => format!("- {}", trimmed),
            _                   => trimmed,
        };
        lines.push(line);
    }

    lines.join("\n")
}

fn is_inside_noise(element: &ElementRef<'_>) -> bool {
    const NOISE: &[&str] = &[
        "script", "style", "noscript", "nav",
        "footer", "header", "aside", "iframe",
    ];
    let mut cursor = element.parent();
    while let Some(node) = cursor {
        if let Some(el) = ElementRef::wrap(node) {
            if NOISE.contains(&el.value().name()) {
                return true;
            }
        }
        cursor = node.parent();
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_title_extracted() {
        let html = "<html><head><title>My Page</title></head><body></body></html>";
        let result = extract_text(html);
        assert!(result.contains("# My Page"), "got: {}", result);
    }

    #[test]
    fn test_h1_prefix() {
        let html = "<html><body><h1>Main Heading</h1></body></html>";
        let result = extract_text(html);
        assert!(result.contains("# Main Heading"), "got: {}", result);
    }

    #[test]
    fn test_h2_prefix() {
        let html = "<html><body><h2>Sub Heading</h2></body></html>";
        let result = extract_text(html);
        assert!(result.contains("## Sub Heading"), "got: {}", result);
    }

    #[test]
    fn test_h3_prefix() {
        let html = "<html><body><h3>Minor Heading</h3></body></html>";
        let result = extract_text(html);
        assert!(result.contains("### Minor Heading"), "got: {}", result);
    }

    #[test]
    fn test_paragraph_no_prefix() {
        let html = "<html><body><p>Some paragraph text</p></body></html>";
        let result = extract_text(html);
        assert!(result.contains("Some paragraph text"), "got: {}", result);
        assert!(!result.contains("# Some"), "got: {}", result);
    }

    #[test]
    fn test_li_prefix() {
        let html = "<html><body><ul><li>Item</li></ul></body></html>";
        let result = extract_text(html);
        assert!(result.contains("- Item"), "got: {}", result);
    }

    #[test]
    fn test_script_excluded() {
        let html = "<html><body><script><p>Should not appear</p></script><p>Real</p></body></html>";
        let result = extract_text(html);
        assert!(!result.contains("Should not appear"), "got: {}", result);
        assert!(result.contains("Real"), "got: {}", result);
    }

    #[test]
    fn test_style_excluded() {
        let html = "<html><head><style><p>CSS noise</p></style></head><body><p>Visible</p></body></html>";
        let result = extract_text(html);
        assert!(!result.contains("CSS noise"), "got: {}", result);
        assert!(result.contains("Visible"), "got: {}", result);
    }

    #[test]
    fn test_nav_excluded() {
        let html = "<html><body><nav><li>Nav link</li></nav><p>Content</p></body></html>";
        let result = extract_text(html);
        assert!(!result.contains("Nav link"), "got: {}", result);
        assert!(result.contains("Content"), "got: {}", result);
    }

    #[test]
    fn test_footer_excluded() {
        let html = "<html><body><footer><p>Footer text</p></footer><p>Main</p></body></html>";
        let result = extract_text(html);
        assert!(!result.contains("Footer text"), "got: {}", result);
        assert!(result.contains("Main"), "got: {}", result);
    }

    #[test]
    fn test_header_excluded() {
        let html = "<html><body><header><h1>Site Title</h1></header><h1>Article</h1></body></html>";
        let result = extract_text(html);
        assert!(!result.contains("# Site Title"), "got: {}", result);
        assert!(result.contains("# Article"), "got: {}", result);
    }

    #[test]
    fn test_aside_excluded() {
        let html = "<html><body><aside><p>Sidebar</p></aside><p>Body</p></body></html>";
        let result = extract_text(html);
        assert!(!result.contains("Sidebar"), "got: {}", result);
        assert!(result.contains("Body"), "got: {}", result);
    }

    #[test]
    fn test_empty_elements_skipped() {
        let html = "<html><body><p></p><p>  </p><p>Real</p></body></html>";
        let result = extract_text(html);
        let lines: Vec<&str> = result.lines().collect();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0], "Real");
    }

    #[test]
    fn test_whitespace_normalized() {
        let html = "<html><body><p>  Hello   World  </p></body></html>";
        let result = extract_text(html);
        assert!(result.contains("Hello World"), "got: {}", result);
        assert!(!result.contains("  Hello"), "got: {}", result);
    }

    #[test]
    fn test_empty_document() {
        let result = extract_text("");
        assert!(result.is_empty());
    }

    #[test]
    fn test_multiple_headings_order_preserved() {
        let html = "<html><body><h1>First</h1><h2>Second</h2><h3>Third</h3></body></html>";
        let result = extract_text(html);
        let first_pos = result.find("# First").unwrap();
        let second_pos = result.find("## Second").unwrap();
        let third_pos = result.find("### Third").unwrap();
        assert!(first_pos < second_pos);
        assert!(second_pos < third_pos);
    }
}
