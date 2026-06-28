# Feature Meta: batch-page-actions

## Identity

- **feature_name**: batch-page-actions
- **feature_number**: 002
- **feature_id**: feat-002
- **created_at**: 2026-06-09

## Context

- **project_mode**: brownfield
- **execution_mode**: standard
- **project_type**: production
- **technology**: rust
- **framework**: chromiumoxide (CDP), rmcp (MCP Rust SDK)
- **git_branch**: feature/batch-page-actions (to be created)

## Description

Add a new MCP tool that accepts a list of page interaction actions (fill text field,
click button, select dropdown option) and executes them **concurrently** in a single
tool call, instead of requiring separate `fill_form` and `click_button` calls.

**Key design decisions**:

- New MCP tool: `act` (or `interact`) accepting `actions: Vec<Action>`
- Each action is typed: `Fill { identifier, value }`, `Click { label? / id? }`, `Select { identifier, value }`
- Actions run concurrently via `tokio::join!` or `futures::join_all`
- Actions that depend on DOM state (e.g., click after fill) should be sequenceable
  via an optional `after` field or sequential group
- Returns per-action results with success/error per action

**Concurrent execution model**:

- All independent actions fire in parallel (tokio tasks on the same page)
- Collect results after all complete (success or failure per action)
- Page state is read once before dispatch; actions applied concurrently via CDP

## Implementation Scope

- `src/types/form.rs` — add `Action` enum and `ActResult` types
- `src/browser/page.rs` — add `execute_actions_concurrent()` method
- `src/operations/` — add `act.rs` with batch orchestration logic
- `src/mcp/` — add `act_handler.rs`
- `src/main.rs` — expose new `act` MCP tool

## Testing Config (Production)

- **unit_coverage_target**: 80%
- **integration_tests**: required
- **test_threads**: 1 (Chrome concurrency constraint — browser-level, not action-level)
- **fury_test**: n/a (non-Fury project)

## Status

- [ ] Functional spec — Draft, ready for review
- [ ] Technical spec — Draft, ready for review
- [ ] Tasks
- [ ] Implementation
- [ ] Code review
