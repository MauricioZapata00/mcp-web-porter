use serde::{Deserialize, Serialize};
use schemars::JsonSchema;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FormFieldType {
    Text,
    Email,
    Password,
    Select,
    Checkbox,
    Textarea,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectOption {
    pub value: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormField {
    pub selector:    String,
    pub field_type:  FormFieldType,
    pub name:        Option<String>,
    pub id:          Option<String>,
    pub label:       Option<String>,
    pub placeholder: Option<String>,
    pub required:    bool,
    pub options:     Vec<SelectOption>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormButton {
    pub label:    String,
    pub selector: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum FieldValue {
    Text(String),
    Selected(String),
    Checked(bool),
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FieldInput {
    pub identifier: String,
    pub value:      FieldValue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormFillResult {
    pub fields_filled:   Vec<FilledField>,
    pub submitted:       bool,
    pub page_text_after: Option<String>,
    pub next_step:       Option<FormStep>,
    pub preview:         Option<FormPreview>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilledField {
    pub identifier: String,
    pub field_type: FormFieldType,
    pub value:      FieldValue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormStep {
    pub fields:  Vec<FormField>,
    pub buttons: Vec<FormButton>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormPreview {
    pub entries: Vec<FieldPreview>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldPreview {
    pub identifier:    String,
    pub field_type:    FormFieldType,
    pub current_value: Option<String>,
    pub planned_value: FieldValue,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_field_value_text_roundtrip() {
        let v = FieldValue::Text("hello".to_string());
        let json = serde_json::to_string(&v).unwrap();
        let back: FieldValue = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, FieldValue::Text(s) if s == "hello"));
    }

    #[test]
    fn test_field_value_selected_roundtrip() {
        let v = FieldValue::Selected("option1".to_string());
        let json = serde_json::to_string(&v).unwrap();
        let back: FieldValue = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, FieldValue::Selected(s) if s == "option1"));
    }

    #[test]
    fn test_field_value_checked_roundtrip() {
        let v = FieldValue::Checked(true);
        let json = serde_json::to_string(&v).unwrap();
        let back: FieldValue = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, FieldValue::Checked(true)));
    }

    #[test]
    fn test_field_input_roundtrip() {
        let input = FieldInput {
            identifier: "username".to_string(),
            value: FieldValue::Text("alice".to_string()),
        };
        let json = serde_json::to_string(&input).unwrap();
        let back: FieldInput = serde_json::from_str(&json).unwrap();
        assert_eq!(back.identifier, "username");
        assert!(matches!(back.value, FieldValue::Text(s) if s == "alice"));
    }
}
