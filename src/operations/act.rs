use std::time::{Duration, Instant};

use crate::browser::{
    click_button_by_id_or_label_on_page, detect_form_on_page, field_ready_on_page,
    fill_field_on_page,
};
use crate::operations::resolve_field;
use crate::types::{
    ActionResult, FieldValue, FormField, FormFieldType, HtmlError, MAX_FIELD_TIMEOUT_MS,
    PageAction,
};

const FORBIDDEN_IDENTIFIER_CHARS: &[char] = &['>', '~', '+', ':'];

pub(crate) fn validate_actions(
    actions:          &[PageAction],
    max_actions:      usize,
    field_timeout_ms: u64,
) -> Result<(), HtmlError> {
    if field_timeout_ms > MAX_FIELD_TIMEOUT_MS {
        return Err(HtmlError::InvalidFieldValue {
            field:  "field_timeout_ms".into(),
            reason: format!("must be between 0 and {}", MAX_FIELD_TIMEOUT_MS),
        });
    }
    if actions.is_empty() {
        return Err(HtmlError::NoActions);
    }
    if actions.len() > max_actions {
        return Err(HtmlError::TooManyActions {
            count: actions.len(),
            max:   max_actions,
        });
    }
    for action in actions {
        match action {
            PageAction::Click { label: None, id: None } => {
                return Err(HtmlError::ButtonNotFound(
                    "Click action requires at least one of: label, id".into(),
                ));
            }
            PageAction::Fill { identifier, .. }
            | PageAction::Check { identifier, .. } => {
                validate_identifier(identifier)?;
            }
            PageAction::Select { identifier, value } => {
                validate_identifier(identifier)?;
                if value.is_empty() {
                    return Err(HtmlError::InvalidFieldValue {
                        field:  identifier.clone(),
                        reason: "Select value must be a non-empty string".into(),
                    });
                }
            }
            PageAction::Click { .. } => {}
        }
    }
    Ok(())
}

fn validate_identifier(id: &str) -> Result<(), HtmlError> {
    if id.is_empty() {
        return Err(HtmlError::FieldNotFound(
            "identifier must not be empty".into(),
        ));
    }
    if let Some(bad) = id.chars().find(|c| FORBIDDEN_IDENTIFIER_CHARS.contains(c)) {
        return Err(HtmlError::InvalidFieldValue {
            field:  id.to_string(),
            reason: format!("Invalid identifier: contains CSS selector character '{}'", bad),
        });
    }
    Ok(())
}

pub(crate) async fn execute_actions_concurrent<'a>(
    page:             &'a chromiumoxide::Page,
    actions:          &'a [PageAction],
    detected:         &'a [FormField],
    field_timeout_ms: u64,
) -> Vec<ActionResult> {
    let mut results: Vec<Option<ActionResult>> = (0..actions.len()).map(|_| None).collect();

    // Phase 1: Fill / Select / Check — concurrent (page is Arc-backed, CDP multiplexes).
    let fill_futures = actions
        .iter()
        .enumerate()
        .filter(|(_, a)| !matches!(a, PageAction::Click { .. }))
        .map(|(i, action)| async move {
            (i, execute_single_action(page, action, detected, field_timeout_ms).await)
        });
    for (i, r) in futures::future::join_all(fill_futures).await {
        results[i] = Some(r);
    }

    // Phase 2: Click actions — sequential after fills settle to avoid mid-fill nav.
    for (i, action) in actions.iter().enumerate() {
        if matches!(action, PageAction::Click { .. }) {
            results[i] =
                Some(execute_single_action(page, action, detected, field_timeout_ms).await);
        }
    }

    results.into_iter().map(|r| r.expect("every slot filled")).collect()
}

async fn execute_single_action(
    page:             &chromiumoxide::Page,
    action:           &PageAction,
    detected:         &[FormField],
    field_timeout_ms: u64,
) -> ActionResult {
    match action {
        PageAction::Fill { identifier, value } => {
            match resolve_with_retry(page, identifier, detected, field_timeout_ms).await {
                Ok(field) => {
                    let fv = FieldValue::Text(value.clone());
                    let outcome = fill_field_on_page(page, &field.selector, &fv).await;
                    finish(redact_if_password(action.clone(), &field.field_type), outcome)
                }
                Err(e) => fail(action.clone(), e),
            }
        }
        PageAction::Select { identifier, value } => {
            match resolve_with_retry(page, identifier, detected, field_timeout_ms).await {
                Ok(field) => {
                    let fv = FieldValue::Selected(value.clone());
                    let outcome = fill_field_on_page(page, &field.selector, &fv).await;
                    finish(action.clone(), outcome)
                }
                Err(e) => fail(action.clone(), e),
            }
        }
        PageAction::Check { identifier, checked } => {
            match resolve_with_retry(page, identifier, detected, field_timeout_ms).await {
                Ok(field) => {
                    let fv = FieldValue::Checked(*checked);
                    let outcome = fill_field_on_page(page, &field.selector, &fv).await;
                    finish(action.clone(), outcome)
                }
                Err(e) => fail(action.clone(), e),
            }
        }
        PageAction::Click { label, id } => {
            let outcome =
                click_button_by_id_or_label_on_page(page, id.as_deref(), label.as_deref())
                    .await
                    .map(|_| ());
            finish(action.clone(), outcome)
        }
    }
}

fn finish(action: PageAction, outcome: Result<(), HtmlError>) -> ActionResult {
    match outcome {
        Ok(()) => ActionResult { action, success: true, error: None },
        Err(e) => ActionResult { action, success: false, error: Some(e.to_string()) },
    }
}

fn fail(action: PageAction, err: HtmlError) -> ActionResult {
    ActionResult { action, success: false, error: Some(err.to_string()) }
}

fn redact_if_password(action: PageAction, ft: &FormFieldType) -> PageAction {
    if let (FormFieldType::Password, PageAction::Fill { identifier, .. }) = (ft, &action) {
        return PageAction::Fill {
            identifier: identifier.clone(),
            value:      "[REDACTED]".to_string(),
        };
    }
    action
}

async fn resolve_with_retry(
    page:             &chromiumoxide::Page,
    identifier:       &str,
    detected:         &[FormField],
    field_timeout_ms: u64,
) -> Result<FormField, HtmlError> {
    if let Some(f) = resolve_field(detected, identifier) {
        if field_ready_on_page(page, &f.selector).await {
            return Ok(f.clone());
        }
    }
    if field_timeout_ms == 0 {
        return Err(HtmlError::FieldNotFound(format!(
            "Field not found or still disabled: {}",
            identifier
        )));
    }
    let start = Instant::now();
    let mut delay_ms = 100u64;
    loop {
        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
        if start.elapsed().as_millis() as u64 >= field_timeout_ms {
            return Err(HtmlError::FieldNotFound(format!(
                "Field not found or still disabled after {}ms: {}",
                field_timeout_ms, identifier
            )));
        }
        let (new_detected, _) = detect_form_on_page(page).await?;
        if let Some(f) = resolve_field(&new_detected, identifier) {
            if field_ready_on_page(page, &f.selector).await {
                return Ok(f.clone());
            }
        }
        delay_ms = (delay_ms * 2).min(500);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fill(id: &str, v: &str) -> PageAction {
        PageAction::Fill { identifier: id.into(), value: v.into() }
    }

    #[test]
    fn rejects_empty_actions() {
        assert!(matches!(
            validate_actions(&[], 20, 2000),
            Err(HtmlError::NoActions)
        ));
    }

    #[test]
    fn rejects_too_many_actions() {
        let actions = vec![fill("a", "x"); 21];
        let err = validate_actions(&actions, 20, 2000).unwrap_err();
        assert!(matches!(err, HtmlError::TooManyActions { count: 21, max: 20 }));
    }

    #[test]
    fn rejects_oversize_field_timeout() {
        let err = validate_actions(&[fill("a", "x")], 20, 30_001).unwrap_err();
        assert!(matches!(err, HtmlError::InvalidFieldValue { .. }));
    }

    #[test]
    fn accepts_field_timeout_at_max() {
        assert!(validate_actions(&[fill("a", "x")], 20, MAX_FIELD_TIMEOUT_MS).is_ok());
    }

    #[test]
    fn rejects_click_with_both_none() {
        let actions = vec![PageAction::Click { label: None, id: None }];
        let err = validate_actions(&actions, 20, 2000).unwrap_err();
        assert!(matches!(err, HtmlError::ButtonNotFound(_)));
    }

    #[test]
    fn rejects_empty_identifier() {
        let err = validate_actions(&[fill("", "x")], 20, 2000).unwrap_err();
        assert!(matches!(err, HtmlError::FieldNotFound(_)));
    }

    #[test]
    fn rejects_css_special_chars_in_identifier() {
        for bad in ['>', '~', '+', ':'] {
            let id = format!("name{}x", bad);
            let err = validate_actions(&[fill(&id, "x")], 20, 2000).unwrap_err();
            assert!(matches!(err, HtmlError::InvalidFieldValue { .. }),
                "expected reject for char '{}'", bad);
        }
    }

    #[test]
    fn rejects_empty_select_value() {
        let actions = vec![PageAction::Select {
            identifier: "country".into(),
            value:      "".into(),
        }];
        let err = validate_actions(&actions, 20, 2000).unwrap_err();
        assert!(matches!(err, HtmlError::InvalidFieldValue { .. }));
    }

    #[test]
    fn accepts_valid_mixed_actions() {
        let actions = vec![
            fill("name", "Alice"),
            PageAction::Select { identifier: "country".into(), value: "US".into() },
            PageAction::Check { identifier: "tos".into(), checked: true },
            PageAction::Click { label: Some("Submit".into()), id: None },
            PageAction::Click { label: None, id: Some("btn".into()) },
        ];
        assert!(validate_actions(&actions, 20, 2000).is_ok());
    }

    #[test]
    fn redacts_password_fill_value() {
        let action = PageAction::Fill { identifier: "pw".into(), value: "secret".into() };
        let redacted = redact_if_password(action, &FormFieldType::Password);
        match redacted {
            PageAction::Fill { value, .. } => assert_eq!(value, "[REDACTED]"),
            _ => panic!("expected Fill"),
        }
    }

    #[test]
    fn does_not_redact_non_password_fill() {
        let action = PageAction::Fill { identifier: "name".into(), value: "Alice".into() };
        let kept = redact_if_password(action, &FormFieldType::Text);
        match kept {
            PageAction::Fill { value, .. } => assert_eq!(value, "Alice"),
            _ => panic!("expected Fill"),
        }
    }

    #[test]
    fn does_not_redact_non_fill_action_on_password_type() {
        let action = PageAction::Click { label: Some("Login".into()), id: None };
        let kept = redact_if_password(action, &FormFieldType::Password);
        assert!(matches!(kept, PageAction::Click { .. }));
    }
}
