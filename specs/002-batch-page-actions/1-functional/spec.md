# Functional Specification: batch-page-actions

**Feature**: batch-page-actions
**Feature ID**: feat-002
**Status**: Draft
**Created**: 2026-06-12
**Depends on**: feat-001 (chrome-session-navigation) — session infrastructure

---

## 1. Problem Statement

All existing MCP tools require one tool call per page interaction. Filling a form with
three fields and clicking Submit requires four separate calls: three `fill_form` calls
and one `click_button`. Each call spawns a new browser, navigates to the page, performs
one action, and discards everything. This is slow, wasteful, and error-prone — the page
state between calls is not guaranteed to be consistent.

There is also no way to perform multiple interactions at once on a single page load: all
actions are strictly serial, and there is no tool that lets callers express "do all of
these things together in one call."

Additionally, the current tools have no session awareness — filling a form that requires
authentication forces the caller to manually pass cookies on every call, with no way to
leverage the persistent session established by feat-001.

---

## 2. Objectives

1. Allow multiple page interactions (fill text inputs, fill number inputs, select
   dropdown options, click buttons, toggle checkboxes) to be dispatched concurrently in
   a single tool call, returning per-action results and the resulting page content.
2. Provide an **authenticated variant** (`act_with_session`) that uses the persistent
   Chrome session from feat-001 — so a login performed once carries into the actions.
3. Provide a **stateless variant** (`act`) for pages that do not require authentication,
   with the same concurrent execution model.

---

## 3. Success Metrics

| Metric                    | Target                                                                     |
|---------------------------|----------------------------------------------------------------------------|
| Actions per call          | Up to `max_actions` (default 20) actions dispatched in a single call       |
| Partial failure isolation | One failing action does not abort the others; per-action error returned    |
| Page text returned        | `page_text_after` is always present in the response                        |
| Authenticated actions     | `act_with_session` executes actions in the session established by feat-001 |
| No credentials passed     | User logs in via Chrome manually; no username/password ever passed to the AI agent |

---

## 4. Scope

### In Scope

- New MCP tool **`act_with_session`**: concurrent actions on an authenticated page using
  feat-001's persistent Chrome session and cookie extraction
- New MCP tool **`act`**: concurrent actions on a page without session (accepts optional
  `cookies` and `headers` parameters like existing tools)
- Action types: `Fill` (text / number / email / password / textarea), `Select`
  (dropdown), `Click` (by visible label or HTML id), `Check` (checkbox toggle)
- Per-action result: `{ action, success, error? }` returned for every action regardless
  of individual failure — one failing action does not abort the rest
- `page_text_after`: page text content returned after all actions settle
- `next_step`: optional detected form fields/buttons if a new form appeared after actions
- Configurable action limit via `max_actions` parameter (default: 20)
- Configurable per-action retry timeout via `field_timeout_ms` parameter (default: 2000ms) for fields that are initially
  disabled

### Out of Scope

- Using `act` / `act_with_session` when you only need a single action — the existing
  `fill_form` and `click_button` tools are simpler for that. `act` accepts single-action
  lists and they work, but it adds unnecessary overhead compared to the dedicated tools.
  This is a usage recommendation, not a capability restriction.
- Multi-page / wizard flows where step N depends on step N-1 navigation result
- `detect_form` with session (form discovery remains stateless)
- Writing back to the Chrome profile (feat-001 constraint)

---

## 5. User Stories

### US-1 — Execute concurrent authenticated page interactions

**As** an MCP user,
**I want to** call `act_with_session` with a URL and a list of actions,
**So that** multiple form fields, selects, and buttons are acted on simultaneously using
my logged-in Chrome session, and I get the resulting page content back in one call.

**Acceptance Criteria**:

- AC-1.1: Given an authenticated page with multiple inputs, when `act_with_session` is
  called with a list of Fill / Select / Click actions, then all actions are dispatched
  concurrently, per-action results are returned, and `page_text_after` contains the
  page content after all actions have settled.
- AC-1.2: Given one action fails (e.g., field not found by name/id/label), then the
  remaining actions still complete and the failed action's result contains `error` — the
  entire call is NOT aborted.
- AC-1.3: Given `actions` exceeds `max_actions`, then the tool returns a validation
  error before touching the page.
- AC-1.4: Given Chrome has saved cookies for the target domain, the actions execute in
  the authenticated page state (not the login or redirect page).
- AC-1.5: Given a new form appears on the page after actions settle, then `next_step`
  in the response describes that form's fields and buttons.
- AC-1.6: Given `debug_port` is provided, `act_with_session` connects to that Chrome
  instance and skips cookie extraction — same fallback path as `read_page_with_session`.

---

### US-2 — Execute concurrent stateless page interactions

**As** an MCP user,
**I want to** call `act` with a URL and a list of actions,
**So that** I hand all required form information to the tool in a single call and it
fills the entire form at once — no orchestrating N separate tool calls, no N Chrome
launches, no N round-trips through the MCP protocol.

**Acceptance Criteria**:

- AC-2.1: Given a public (unauthenticated) page, when `act` is called with a list of
  Fill / Select / Click / Check actions, then all actions are dispatched concurrently
  and per-action results are returned.
- AC-2.2: `act` accepts optional `cookies` and `headers` parameters, consistent with
  existing tools (`fill_form`, `click_button`).
- AC-2.3: `act` spawns a fresh browser per call (no persistent session), consistent
  with existing tools.
- AC-2.4: Partial failure behavior, `page_text_after`, and `next_step` behave identically
  to `act_with_session`.

---

### US-3 — Interact with authenticated pages without passing credentials

**As** an MCP user who is already logged into a website in my Chrome browser,
**I want to** call `act_with_session` to fill forms and click buttons on that
authenticated page,
**So that** I never pass my username, password, or any credentials to the AI agent —
authentication is handled entirely by my existing Chrome session.

**Acceptance Criteria**:

- AC-3.1: Given the user is logged into a site in their Chrome browser, when
  `act_with_session` is called with that site's URL and a list of non-credential
  actions, then the page loads in authenticated state and all actions execute without
  the user providing any credentials.
- AC-3.2: Given the user has NOT logged into a site in Chrome, when `act_with_session`
  navigates to a page that redirects to a login form, the tool returns the login page
  content in `page_text_after` and does NOT prompt for credentials — the user must
  log in via Chrome first.
- AC-3.3: Credentials (username, password, tokens, API keys) must never appear as
  `value` in any `Fill` action. This is a usage constraint enforced by documentation,
  not by the tool itself.

---

## 6. User Experience

### Interacting with an already-authenticated page (correct flow)

The user is already logged into `app.example.com` in their Chrome browser.
No credentials are involved — feat-001's cookie extraction handles authentication.

```
Prerequisites (done by the user in Chrome, not the AI agent):
  User opens Chrome → navigates to app.example.com → logs in manually

Call 1 — act_with_session (fill and submit a settings form)
Input: {
  "url": "https://app.example.com/settings",
  "actions": [
    { "type": "Fill",   "identifier": "First name", "value": "Alice" },
    { "type": "Fill",   "identifier": "Last name",  "value": "Smith" },
    { "type": "Select", "identifier": "Country",    "value": "Colombia" },
    { "type": "Select", "identifier": "Language",   "value": "English" },
    { "type": "Click",  "label": "Save changes" }
  ]
}
→ feat-001 extracts session cookies from ~/Library/.../Chrome/Default/Cookies
→ Browser navigates to /settings — page loads authenticated (no login redirect)
→ Fills + Selects dispatched concurrently; Click fires after all settle
→ Returns: { action_results: [...], page_text_after: "Settings saved", next_step: null }

Call 2 — act_with_session (another form on the same site, same session)
Input: { "url": "https://app.example.com/profile", "actions": [...] }
→ Same browser session reused — cookies already present, no re-extraction needed
```

### What happens if the user is NOT logged in

```
Call — act_with_session on a page that requires login
Input: { "url": "https://app.example.com/dashboard", "actions": [...] }

→ feat-001 finds no valid session cookies for this domain in Chrome
→ Browser navigates to /dashboard → server redirects to /login
→ Actions target fields on /dashboard but the page is now /login
→ Most actions fail (fields not found) and return per-action errors
→ page_text_after contains the login page content

Correct resolution: user logs in via Chrome first, then retries the call.
The AI agent does NOT ask for credentials — it returns the login page text
so the caller can detect the redirect and instruct the user to log in.
```

### Stateless concurrent actions

```
Tool: act
Input: {
  "url": "https://httpbin.org/forms/post",
  "actions": [
    { "type": "Fill", "identifier": "custname", "value": "Alice" },
    { "type": "Fill", "identifier": "custtel",  "value": "555-1234" },
    { "type": "Fill", "identifier": "custemail","value": "alice@x.com" }
  ],
  "auto_submit": true
}
→ Fresh browser spawned
→ Three Fill actions dispatched concurrently
→ Form submitted
→ Returns: { action_results: [...], page_text_after: "...", next_step: null }
```

---

## 7. Business Rules

| ID   | Rule                                                                                                                                                                                                                                                                                                                                                                                                                              |
|------|-----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| BR-1 | Actions are dispatched concurrently. One action failing does not cancel the others; all run to completion or individual timeout.                                                                                                                                                                                                                                                                                                  |
| BR-2 | Action `identifier` fields are resolved by name → id → label (same order as `fill_form`). Arbitrary CSS selectors are not accepted.                                                                                                                                                                                                                                                                                               |
| BR-3 | Cookie injection for `act_with_session` is scoped to the target URL's domain — cookies from other domains are not injected.                                                                                                                                                                                                                                                                                                       |
| BR-4 | Fill / Select / Check actions are dispatched concurrently. Click actions are dispatched sequentially after all non-Click actions have settled, to prevent triggering navigation mid-fill.                                                                                                                                                                                                                                         |
| BR-5 | The `max_actions` limit (default 20) is an AI-agent safety guard, not a Chrome technical constraint. It prevents runaway agents from generating unbounded action lists.                                                                                                                                                                                                                                                           |
| BR-6 | `act_with_session` uses feat-001's singleton browser session. `act` spawns a fresh browser per call.                                                                                                                                                                                                                                                                                                                              |
| BR-7 | If a field is disabled or not yet present in the DOM when its action is dispatched, the action retries with a short wait (up to a configurable `field_timeout_ms`, default 2000ms) before failing. This handles progressive-disclosure forms where filling field A triggers JS that enables field B. The retry is per-action and transparent to the caller — the final result reflects success or failure after the full timeout. |

---

## 8. Validations

| Input                             | Validation                                                | Error                                              |
|-----------------------------------|-----------------------------------------------------------|----------------------------------------------------|
| `url`                             | Must be a valid `http://` or `https://` URL               | "Invalid URL: must start with http:// or https://" |
| `actions`                         | Must not be empty                                         | `NoActions` error                                  |
| `actions` length                  | Must not exceed `max_actions` (default 20)                | `TooManyActions { count, max }` error              |
| `field_timeout_ms`                | Must be 0–30000ms if provided                             | "field_timeout_ms must be between 0 and 30000"     |
| `action.type`                     | Must be one of: Fill, Select, Click, Check                | "Unknown action type: {type}"                      |
| `action.identifier`               | Non-empty; no CSS special characters (`>`, `~`, `+`, `:`) | "Invalid identifier: {reason}"                     |
| `action.value` (Fill)             | Must be a string (empty string allowed)                   | "Fill value must be a string"                      |
| `action.value` (Select)           | Must be a non-empty string                                | "Select value must be a non-empty string"          |
| `action.label` (Click)            | At least one of `label` or `id` must be provided          | "Click action requires at least one of: label, id" |
| `debug_port` (`act_with_session`) | Valid port (1–65535)                                      | "Invalid debug_port: must be between 1 and 65535"  |

---

## 9. Dependencies

| Dependency                      | Type               | Notes                                                                                                  |
|---------------------------------|--------------------|--------------------------------------------------------------------------------------------------------|
| feat-001 session infrastructure | Feature dependency | `act_with_session` uses `get_or_init_session()`, `session_lock()`, and `CookieExtractor` from feat-001 |
| `chromiumoxide::Page`           | Runtime            | `Page::clone()` (Arc-backed) enables concurrent CDP calls                                              |
| `futures` crate                 | Build              | Already present; used for `join_all` concurrent dispatch                                               |

---

## 10. Risks

| Risk                                                                    | Likelihood | Impact | Mitigation                                                                                     |
|-------------------------------------------------------------------------|------------|--------|------------------------------------------------------------------------------------------------|
| Concurrent Click actions trigger duplicate form submits                 | Medium     | Medium | Document: caller is responsible for not including multiple submit-targeting Clicks in one call |
| Page navigates away mid-fill (redirect triggered by a concurrent Click) | Low        | Medium | Each action returns its own error; remaining CDP calls report `BrowserError`                   |
| AI agent generates action list exceeding `max_actions`                  | Medium     | Low    | `TooManyActions` error returned before any page contact                                        |

---

## 11. Edge Cases

| Scenario                                                                               | Expected Behavior                                                                                                                                                                                                            |
|----------------------------------------------------------------------------------------|------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| One action's field not found                                                           | `{ success: false, error: "Field not found: {identifier}" }`; others proceed                                                                                                                                                 |
| Multiple Click actions on the same submit button                                       | All fire — caller responsible for no duplicate submits                                                                                                                                                                       |
| Page navigates away mid-action (redirect on first Click)                               | Remaining CDP operations may fail; each returns its own error                                                                                                                                                                |
| `next_step` detected after actions settle                                              | Response includes `next_step` with new form's fields and buttons                                                                                                                                                             |
| `act` called with `cookies` that cover the domain                                      | Cookies injected like existing tools; no session used                                                                                                                                                                        |
| `actions` is empty                                                                     | `NoActions` error returned before any browser interaction                                                                                                                                                                    |
| Form with many mixed field types (e.g. 5 text + 3 number + 2 select + 1 date + submit) | All supported in one call. Fill handles text/number/email/password/textarea/date; Select handles dropdowns; Click (submit) fires in Phase 2 after all fills and selects have settled.                                        |
| Calendar input is a native `<input type="date">`                                       | Use `Fill` with an ISO date string (e.g., `"2026-06-12"`). CDP sets the value directly on the input element.                                                                                                                 |
| Calendar input is a custom JS date picker widget (modal/grid)                          | Not supported in a single action. These require multiple sequential steps (open picker → navigate month → click day). Use separate tool calls per step, or model it as a future multi-step flow.                             |
| State/Province field appears only after Country is selected                            | Country `Select` fires concurrently with other actions. State `Fill` retries (BR-7) until Country's JS side-effect enables the State field, then fills it. Both complete within the same `act` call — no second call needed. |
| Field remains disabled past `field_timeout_ms`                                         | Action returns `{ success: false, error: "Field not found or still disabled after {timeout}ms: {identifier}" }`. Other actions are unaffected.                                                                               |

---

## 12. Non-Goals (Explicit)

- `act` / `act_with_session` do NOT accept arbitrary CSS selectors as identifiers — only
  name, id, and label resolution is supported (prevents CSS selector injection).
- `act_with_session` does NOT deduplicate concurrent submit-button clicks — this is
  intentionally left to the caller to avoid silent behavior changes.
- These tools do NOT manage multi-page flows; each call is scoped to a single page load.
- **Credentials (usernames, passwords, tokens, API keys) must NOT be passed as `Fill`
  action values.** Authentication is handled by feat-001's Chrome cookie extraction —
  the user logs in via Chrome, and the tool reuses that session transparently. Passing
  credentials to the agent is a security anti-pattern this feature is explicitly designed
  to avoid.
