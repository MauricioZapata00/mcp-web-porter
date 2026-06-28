use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::form::FormStep;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type")]
pub enum PageAction {
    Fill   { identifier: String, value: String },
    Select { identifier: String, value: String },
    Click  { label: Option<String>, id: Option<String> },
    Check  { identifier: String, checked: bool },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionResult {
    pub action:  PageAction,
    pub success: bool,
    pub error:   Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActResult {
    pub action_results:  Vec<ActionResult>,
    pub page_text_after: String,
    pub next_step:       Option<FormStep>,
}

/// AI-agent safety guard: caps actions per call to prevent a looping agent
/// from generating an unbounded list. Real web forms rarely exceed 15 fields.
pub const DEFAULT_MAX_ACTIONS: usize = 20;

/// Upper bound for `field_timeout_ms` validation. 30s comfortably covers the
/// slowest progressive-disclosure forms without letting a bad input hang
/// the entire call indefinitely.
pub const MAX_FIELD_TIMEOUT_MS: u64 = 30_000;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_action_roundtrip() {
        let a = PageAction::Fill {
            identifier: "email".to_string(),
            value:      "a@b.com".to_string(),
        };
        let json = serde_json::to_string(&a).unwrap();
        let back: PageAction = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, PageAction::Fill { ref identifier, .. } if identifier == "email"));
    }

    #[test]
    fn select_action_roundtrip() {
        let a = PageAction::Select {
            identifier: "country".to_string(),
            value:      "Colombia".to_string(),
        };
        let json = serde_json::to_string(&a).unwrap();
        let back: PageAction = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, PageAction::Select { ref value, .. } if value == "Colombia"));
    }

    #[test]
    fn click_action_roundtrip_with_label() {
        let a = PageAction::Click { label: Some("Save".to_string()), id: None };
        let json = serde_json::to_string(&a).unwrap();
        let back: PageAction = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, PageAction::Click { label: Some(ref l), id: None } if l == "Save"));
    }

    #[test]
    fn click_action_roundtrip_with_id() {
        let a = PageAction::Click { label: None, id: Some("submit-btn".to_string()) };
        let json = serde_json::to_string(&a).unwrap();
        let back: PageAction = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, PageAction::Click { id: Some(ref i), label: None } if i == "submit-btn"));
    }

    #[test]
    fn check_action_roundtrip() {
        let a = PageAction::Check { identifier: "tos".to_string(), checked: true };
        let json = serde_json::to_string(&a).unwrap();
        let back: PageAction = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, PageAction::Check { checked: true, ref identifier } if identifier == "tos"));
    }

    #[test]
    fn act_result_serializes() {
        let r = ActResult {
            action_results: vec![ActionResult {
                action:  PageAction::Fill { identifier: "x".to_string(), value: "y".to_string() },
                success: true,
                error:   None,
            }],
            page_text_after: "ok".to_string(),
            next_step:       None,
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("\"success\":true"));
        assert!(json.contains("\"page_text_after\":\"ok\""));
    }
}
