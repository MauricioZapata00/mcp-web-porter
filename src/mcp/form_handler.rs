use crate::browser::{BrowserOps, ChromiumBrowser};
use crate::operations::{extract_text, resolve_field, validate_inputs};
use crate::types::{
    FieldInput, FieldPreview, FilledField, FormFillResult, FormPreview, FormStep, HtmlError,
    ResourceUri,
};

#[derive(Clone, Default)]
pub struct FormHandler;

impl FormHandler {
    pub async fn handle_detect(&self, url: &str) -> Result<FormStep, HtmlError> {
        ResourceUri::parse(url)?;
        let mut driver = ChromiumBrowser::new(false);
        let result = detect_with_ops(url, &mut driver).await;
        driver.cleanup().await;
        result
    }

    pub async fn handle_fill(
        &self,
        url: &str,
        inputs: Vec<FieldInput>,
        auto_submit: bool,
        submit_button_label: Option<String>,
        dry_run: bool,
    ) -> Result<FormFillResult, HtmlError> {
        ResourceUri::parse(url)?;
        let mut driver = ChromiumBrowser::new(false);
        let result = fill_with_ops(
            url,
            inputs,
            auto_submit,
            submit_button_label.as_deref(),
            dry_run,
            &mut driver,
        )
        .await;
        driver.cleanup().await;
        result
    }
}

pub(crate) async fn detect_with_ops(
    url: &str,
    ops: &mut impl BrowserOps,
) -> Result<FormStep, HtmlError> {
    ops.start().await?;
    ops.open(url).await?;
    ops.read(1500).await?;
    let (fields, buttons) = ops.detect_fields().await?;
    Ok(FormStep { fields, buttons })
}

pub(crate) async fn fill_with_ops(
    url: &str,
    inputs: Vec<FieldInput>,
    auto_submit: bool,
    submit_button_label: Option<&str>,
    dry_run: bool,
    ops: &mut impl BrowserOps,
) -> Result<FormFillResult, HtmlError> {
    ops.start().await?;
    ops.open(url).await?;
    ops.read(1500).await?;
    let (detected, _) = ops.detect_fields().await?;

    validate_inputs(&detected, &inputs)?;

    if dry_run {
        let mut entries = Vec::new();
        for input in &inputs {
            let field = resolve_field(&detected, &input.identifier).unwrap();
            let current_value = ops.read_field_value(&field.selector).await?;
            entries.push(FieldPreview {
                identifier:    input.identifier.clone(),
                field_type:    field.field_type.clone(),
                current_value,
                planned_value: input.value.clone(),
            });
        }
        return Ok(FormFillResult {
            fields_filled:   vec![],
            submitted:       false,
            page_text_after: None,
            next_step:       None,
            preview:         Some(FormPreview { entries }),
        });
    }

    let mut fields_filled = Vec::new();
    for input in &inputs {
        let field = resolve_field(&detected, &input.identifier).unwrap();
        ops.fill_field(&field.selector, &input.value).await?;
        fields_filled.push(FilledField {
            identifier: input.identifier.clone(),
            field_type: field.field_type.clone(),
            value:      input.value.clone(),
        });
    }

    if let Some(label) = submit_button_label {
        ops.click_button(label).await?;
        ops.read(1000).await?;
        let (next_fields, next_buttons) = ops.detect_fields().await?;
        return Ok(FormFillResult {
            fields_filled,
            submitted:       false,
            page_text_after: None,
            next_step:       Some(FormStep { fields: next_fields, buttons: next_buttons }),
            preview:         None,
        });
    }

    if auto_submit {
        ops.submit_form().await?;
        let html = ops.read(1000).await?;
        let text = extract_text(&html);
        return Ok(FormFillResult {
            fields_filled,
            submitted:       true,
            page_text_after: Some(text),
            next_step:       None,
            preview:         None,
        });
    }

    Ok(FormFillResult {
        fields_filled,
        submitted:       false,
        page_text_after: None,
        next_step:       None,
        preview:         None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browser::page::MockBrowserOps;
    use crate::types::{
        FieldValue, FormButton, FormField, FormFieldType,
    };

    fn make_text_field(name: &str) -> FormField {
        FormField {
            selector:    format!("[name=\"{}\"]", name),
            field_type:  FormFieldType::Text,
            name:        Some(name.to_string()),
            id:          None,
            label:       None,
            placeholder: None,
            required:    false,
            options:     vec![],
        }
    }

    fn make_email_field(name: &str) -> FormField {
        FormField {
            selector:    format!("[name=\"{}\"]", name),
            field_type:  FormFieldType::Email,
            name:        Some(name.to_string()),
            id:          None,
            label:       None,
            placeholder: None,
            required:    false,
            options:     vec![],
        }
    }

    fn detected_fields() -> Vec<FormField> {
        vec![
            make_text_field("custname"),
            make_text_field("custtel"),
            make_email_field("custemail"),
        ]
    }

    fn no_buttons() -> Vec<FormButton> {
        vec![]
    }

    // ── Mock-based tests ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_detect_error_in_fill_propagates() {
        let mut mock = MockBrowserOps::new();
        mock.expect_start().returning(|| Ok(()));
        mock.expect_open().returning(|_| Ok(()));
        mock.expect_read().returning(|_| Ok("<html/>".to_string()));
        mock.expect_detect_fields()
            .returning(|| Err(HtmlError::BrowserError("detect failed".to_string())));

        let result = fill_with_ops("https://example.com", vec![], false, None, false, &mut mock).await;
        assert!(matches!(result, Err(HtmlError::BrowserError(_))));
    }

    #[tokio::test]
    async fn test_fill_field_error_propagates() {
        let mut mock = MockBrowserOps::new();
        mock.expect_start().returning(|| Ok(()));
        mock.expect_open().returning(|_| Ok(()));
        mock.expect_read().returning(|_| Ok("<html/>".to_string()));
        mock.expect_detect_fields()
            .returning(|| Ok((detected_fields(), no_buttons())));
        mock.expect_fill_field()
            .returning(|_, _| Err(HtmlError::BrowserError("fill failed".to_string())));

        let inputs = vec![FieldInput {
            identifier: "custname".to_string(),
            value: FieldValue::Text("Alice".to_string()),
        }];
        let result = fill_with_ops("https://example.com", inputs, false, None, false, &mut mock).await;
        assert!(matches!(result, Err(HtmlError::BrowserError(_))));
    }

    #[tokio::test]
    async fn test_submit_form_error_propagates() {
        let mut mock = MockBrowserOps::new();
        mock.expect_start().returning(|| Ok(()));
        mock.expect_open().returning(|_| Ok(()));
        mock.expect_read().returning(|_| Ok("<html/>".to_string()));
        mock.expect_detect_fields()
            .returning(|| Ok((detected_fields(), no_buttons())));
        mock.expect_fill_field().returning(|_, _| Ok(()));
        mock.expect_submit_form()
            .returning(|| Err(HtmlError::SubmitFailed("no form".to_string())));

        let inputs = vec![FieldInput {
            identifier: "custname".to_string(),
            value: FieldValue::Text("Alice".to_string()),
        }];
        let result = fill_with_ops("https://example.com", inputs, true, None, false, &mut mock).await;
        assert!(matches!(result, Err(HtmlError::SubmitFailed(_))));
    }

    #[tokio::test]
    async fn test_click_button_not_found_propagates() {
        let mut mock = MockBrowserOps::new();
        mock.expect_start().returning(|| Ok(()));
        mock.expect_open().returning(|_| Ok(()));
        mock.expect_read().returning(|_| Ok("<html/>".to_string()));
        mock.expect_detect_fields()
            .returning(|| Ok((detected_fields(), no_buttons())));
        mock.expect_fill_field().returning(|_, _| Ok(()));
        mock.expect_click_button()
            .returning(|_| Err(HtmlError::ButtonNotFound("Continue".to_string())));

        let inputs = vec![FieldInput {
            identifier: "custname".to_string(),
            value: FieldValue::Text("Alice".to_string()),
        }];
        let result = fill_with_ops(
            "https://example.com",
            inputs,
            false,
            Some("Continue"),
            false,
            &mut mock,
        )
        .await;
        assert!(matches!(result, Err(HtmlError::ButtonNotFound(_))));
    }

    #[tokio::test]
    async fn test_dry_run_read_field_error_propagates() {
        let mut mock = MockBrowserOps::new();
        mock.expect_start().returning(|| Ok(()));
        mock.expect_open().returning(|_| Ok(()));
        mock.expect_read().returning(|_| Ok("<html/>".to_string()));
        mock.expect_detect_fields()
            .returning(|| Ok((detected_fields(), no_buttons())));
        mock.expect_read_field_value()
            .returning(|_| Err(HtmlError::BrowserError("read failed".to_string())));

        let inputs = vec![FieldInput {
            identifier: "custname".to_string(),
            value: FieldValue::Text("Alice".to_string()),
        }];
        let result = fill_with_ops("https://example.com", inputs, false, None, true, &mut mock).await;
        assert!(matches!(result, Err(HtmlError::BrowserError(_))));
    }

    // ── Integration tests (real Chrome) ───────────────────────────────────────

    #[serial_test::serial]
    #[tokio::test(flavor = "multi_thread")]
    async fn test_handle_detect_returns_fields() {
        let handler = FormHandler::default();
        let result = handler.handle_detect("https://httpbin.org/forms/post").await;
        assert!(result.is_ok(), "detect failed: {:?}", result);
        let step = result.unwrap();
        assert!(!step.fields.is_empty(), "expected at least one field");
    }

    #[serial_test::serial]
    #[tokio::test(flavor = "multi_thread")]
    async fn test_handle_fill_no_submit() {
        let handler = FormHandler::default();
        let inputs = vec![FieldInput {
            identifier: "custname".to_string(),
            value: FieldValue::Text("Alice".to_string()),
        }];
        let result = handler
            .handle_fill("https://httpbin.org/forms/post", inputs, false, None, false)
            .await;
        assert!(result.is_ok(), "fill failed: {:?}", result);
        let r = result.unwrap();
        assert!(!r.fields_filled.is_empty());
        assert!(!r.submitted);
    }

    #[serial_test::serial]
    #[tokio::test(flavor = "multi_thread")]
    async fn test_handle_fill_auto_submit() {
        let handler = FormHandler::default();
        let inputs = vec![FieldInput {
            identifier: "custname".to_string(),
            value: FieldValue::Text("Alice".to_string()),
        }];
        let result = handler
            .handle_fill("https://httpbin.org/forms/post", inputs, true, None, false)
            .await;
        assert!(result.is_ok(), "fill+submit failed: {:?}", result);
        let r = result.unwrap();
        assert!(r.submitted);
        assert!(r.page_text_after.is_some());
    }

    #[serial_test::serial]
    #[tokio::test(flavor = "multi_thread")]
    async fn test_handle_fill_unknown_identifier() {
        let handler = FormHandler::default();
        let inputs = vec![FieldInput {
            identifier: "nonexistent_field_xyz".to_string(),
            value: FieldValue::Text("x".to_string()),
        }];
        let result = handler
            .handle_fill("https://httpbin.org/forms/post", inputs, false, None, false)
            .await;
        assert!(matches!(result, Err(HtmlError::FieldNotFound(_))));
    }

    #[serial_test::serial]
    #[tokio::test(flavor = "multi_thread")]
    async fn test_handle_fill_invalid_email() {
        let handler = FormHandler::default();
        let inputs = vec![FieldInput {
            identifier: "custemail".to_string(),
            value: FieldValue::Text("not-an-email".to_string()),
        }];
        let result = handler
            .handle_fill("https://httpbin.org/forms/post", inputs, false, None, false)
            .await;
        assert!(matches!(result, Err(HtmlError::InvalidFieldValue { .. })));
    }

    #[serial_test::serial]
    #[tokio::test(flavor = "multi_thread")]
    async fn test_handle_fill_dry_run() {
        let handler = FormHandler::default();
        let inputs = vec![FieldInput {
            identifier: "custname".to_string(),
            value: FieldValue::Text("Alice".to_string()),
        }];
        let result = handler
            .handle_fill("https://httpbin.org/forms/post", inputs, false, None, true)
            .await;
        assert!(result.is_ok(), "dry_run failed: {:?}", result);
        let r = result.unwrap();
        assert!(r.fields_filled.is_empty());
        assert!(r.preview.is_some());
    }
}
