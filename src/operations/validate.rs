use crate::types::{FieldInput, FieldValue, FormField, FormFieldType, HtmlError};

pub fn validate_inputs(detected: &[FormField], inputs: &[FieldInput]) -> Result<(), HtmlError> {
    for input in inputs {
        let field = resolve_field(detected, &input.identifier)
            .ok_or_else(|| HtmlError::FieldNotFound(input.identifier.clone()))?;
        match (&field.field_type, &input.value) {
            (FormFieldType::Email, FieldValue::Text(v)) if !is_valid_email(v) => {
                return Err(HtmlError::InvalidFieldValue {
                    field: input.identifier.clone(),
                    reason: format!("'{}' is not a valid email address", v),
                });
            }
            (FormFieldType::Select, FieldValue::Selected(v)) => {
                let exists = field.options.iter().any(|o| {
                    o.value.eq_ignore_ascii_case(v) || o.label.eq_ignore_ascii_case(v)
                });
                if !exists {
                    return Err(HtmlError::InvalidFieldValue {
                        field: input.identifier.clone(),
                        reason: format!("option '{}' not found in select", v),
                    });
                }
            }
            _ => {}
        }
    }
    Ok(())
}

pub(crate) fn resolve_field<'a>(fields: &'a [FormField], id: &str) -> Option<&'a FormField> {
    fields.iter().find(|f| {
        f.name.as_deref().is_some_and(|n| n.eq_ignore_ascii_case(id))
            || f.id.as_deref().is_some_and(|i| i.eq_ignore_ascii_case(id))
            || f.label.as_deref().is_some_and(|l| l.eq_ignore_ascii_case(id))
    })
}

fn is_valid_email(v: &str) -> bool {
    match v.find('@') {
        Some(pos) if pos > 0 => {
            let domain = &v[pos + 1..];
            !domain.is_empty() && domain.contains('.') && !domain.ends_with('.')
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{FormFieldType, SelectOption};

    fn text_field(name: &str) -> FormField {
        FormField {
            selector: format!("[name=\"{}\"]", name),
            field_type: FormFieldType::Text,
            name: Some(name.to_string()),
            id: None,
            label: None,
            placeholder: None,
            required: false,
            options: vec![],
        }
    }

    fn email_field(name: &str) -> FormField {
        FormField {
            selector: format!("[name=\"{}\"]", name),
            field_type: FormFieldType::Email,
            name: Some(name.to_string()),
            id: None,
            label: None,
            placeholder: None,
            required: false,
            options: vec![],
        }
    }

    fn select_field_with_options(name: &str) -> FormField {
        FormField {
            selector: format!("[name=\"{}\"]", name),
            field_type: FormFieldType::Select,
            name: Some(name.to_string()),
            id: None,
            label: None,
            placeholder: None,
            required: false,
            options: vec![
                SelectOption { value: "us".to_string(), label: "United States".to_string() },
                SelectOption { value: "ca".to_string(), label: "Canada".to_string() },
            ],
        }
    }

    #[test]
    fn validate_valid_text_passes() {
        let fields = vec![text_field("username")];
        let inputs = vec![FieldInput {
            identifier: "username".to_string(),
            value: FieldValue::Text("alice".to_string()),
        }];
        assert!(validate_inputs(&fields, &inputs).is_ok());
    }

    #[test]
    fn validate_valid_email_passes() {
        let fields = vec![email_field("email")];
        let inputs = vec![FieldInput {
            identifier: "email".to_string(),
            value: FieldValue::Text("alice@example.com".to_string()),
        }];
        assert!(validate_inputs(&fields, &inputs).is_ok());
    }

    #[test]
    fn validate_invalid_email_returns_error() {
        let fields = vec![email_field("email")];
        let inputs = vec![FieldInput {
            identifier: "email".to_string(),
            value: FieldValue::Text("not-an-email".to_string()),
        }];
        let err = validate_inputs(&fields, &inputs).unwrap_err();
        assert!(matches!(err, HtmlError::InvalidFieldValue { .. }));
    }

    #[test]
    fn validate_select_known_value_passes() {
        let fields = vec![select_field_with_options("country")];
        let inputs = vec![FieldInput {
            identifier: "country".to_string(),
            value: FieldValue::Selected("us".to_string()),
        }];
        assert!(validate_inputs(&fields, &inputs).is_ok());
    }

    #[test]
    fn validate_select_known_label_case_insensitive_passes() {
        let fields = vec![select_field_with_options("country")];
        let inputs = vec![FieldInput {
            identifier: "country".to_string(),
            value: FieldValue::Selected("CANADA".to_string()),
        }];
        assert!(validate_inputs(&fields, &inputs).is_ok());
    }

    #[test]
    fn validate_select_unknown_option_returns_error() {
        let fields = vec![select_field_with_options("country")];
        let inputs = vec![FieldInput {
            identifier: "country".to_string(),
            value: FieldValue::Selected("Germany".to_string()),
        }];
        let err = validate_inputs(&fields, &inputs).unwrap_err();
        assert!(matches!(err, HtmlError::InvalidFieldValue { .. }));
    }

    #[test]
    fn validate_unknown_identifier_returns_field_not_found() {
        let fields = vec![text_field("username")];
        let inputs = vec![FieldInput {
            identifier: "nonexistent".to_string(),
            value: FieldValue::Text("value".to_string()),
        }];
        let err = validate_inputs(&fields, &inputs).unwrap_err();
        assert!(matches!(err, HtmlError::FieldNotFound(_)));
    }

    #[test]
    fn validate_empty_inputs_returns_ok() {
        let fields = vec![text_field("username")];
        assert!(validate_inputs(&fields, &[]).is_ok());
    }

    #[test]
    fn resolve_field_matches_by_name_case_insensitive() {
        let fields = vec![text_field("Username")];
        assert!(resolve_field(&fields, "username").is_some());
    }

    #[test]
    fn resolve_field_matches_by_id() {
        let mut field = text_field("u");
        field.id = Some("userId".to_string());
        let fields = vec![field];
        assert!(resolve_field(&fields, "USERID").is_some());
    }

    #[test]
    fn resolve_field_matches_by_label() {
        let mut field = text_field("u");
        field.label = Some("Email Address".to_string());
        let fields = vec![field];
        assert!(resolve_field(&fields, "email address").is_some());
    }

    #[test]
    fn resolve_field_returns_none_for_no_match() {
        let fields = vec![text_field("username")];
        assert!(resolve_field(&fields, "password").is_none());
    }
}
