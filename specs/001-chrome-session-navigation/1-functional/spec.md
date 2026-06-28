# Functional Specification: chrome-session-navigation

**Feature**: chrome-session-navigation
**Feature ID**: feat-001
**Status**: Draft (reopened — act_with_session moved to feat-002)
**Created**: 2026-06-09

---

## 1. Problem Statement

All existing MCP tools in this project (`read_page`, `detect_form`, `fill_form`,
`click_button`) spawn a fresh, isolated Chrome instance with a blank profile on every
call. This makes it **impossible to access any page that requires authentication** —
login-gated dashboards, account pages, or any content that only renders after a
successful session. The tools are entirely blind to the user's logged-in state in
their real Google Chrome browser.

There is currently no path to reach authenticated content, regardless of how the tools
are invoked. This blocks a whole category of real-world use cases.

Beyond that, because every tool call spawns a fresh browser with no session state,
users who need to log in before accessing content are forced to repeat the login flow
on every single call. There is no mechanism to log in once and reuse that authenticated
state across subsequent calls within the same working session.

---

## 2. Objectives

1. Allow `read_page_with_session` to retrieve content from pages the user is already
   logged into in Google Chrome — without requiring any Chrome configuration change.
2. Reuse the same browser instance across multiple calls within a Claude Code session
   to eliminate per-call startup latency.
3. Ensure the browser session is automatically cleaned up when the Claude Code session
   ends — no orphaned Chrome processes.

---

## 3. Success Metrics

| Metric                                | Target                                                                                   |
|---------------------------------------|------------------------------------------------------------------------------------------|
| Authenticated pages successfully read | A URL that requires login returns authenticated content (user dashboard, not login page) |
| Zero configuration for most users     | 90%+ of macOS/Linux users can use the tool without any setup step                        |
| Session reuse                         | Second+ calls within the same Claude Code session do not spawn a new Chrome process      |
| No orphaned processes                 | Zero Chrome processes remain after Claude Code is closed                                 |

---

## 4. Scope

### In Scope

- New MCP tool `read_page_with_session` that reads a page using the user's Chrome cookies
- Automatic extraction of cookies from Google Chrome's on-disk profile (macOS and Linux)
- Windows cookie extraction support (DPAPI-based decryption)
- Persistent browser session lasting the lifetime of the MCP server process
- Optional `debug_port` parameter to connect to a Chrome instance already running with
  remote debugging enabled (power-user fallback)
- Same return format as `read_page` (text content + optional images)
- Graceful degradation when cookies are missing or expired (returns unauthenticated page,
  not an error)
- Automatic session recovery if the browser instance crashes mid-session

### Out of Scope

- Firefox, Safari, or Edge cookie extraction (Google Chrome only)
- Chrome profiles other than the `Default` profile
- Modifying or writing back to the Chrome profile in any way
- `detect_form` with session (form discovery remains stateless)
- Concurrent batch page interactions (`act_with_session` and stateless `act`) —
  implemented in **feat-002 (batch-page-actions)**

---

## 5. User Stories

### US-1 — Read an authenticated page without manual cookie setup

**As** an MCP user,
**I want to** call `read_page_with_session` with a URL I am logged into in Chrome,
**So that** I get the authenticated page content without extracting or pasting cookies
manually.

**Acceptance Criteria**:

- AC-1.1: Given Chrome has saved cookies for a site, when `read_page_with_session` is
  called with that site's URL, then the returned content reflects the authenticated
  state (e.g., account dashboard, not login page).
- AC-1.2: Given no Chrome installation is found at the expected OS path, when the tool
  is called, then it returns an error with the expected paths and instructions to verify
  the Chrome installation.
- AC-1.3: Given cookies exist for the domain but are expired, when the tool is called,
  then it returns the page content as-is (the unauthenticated or re-login page) — not
  an application error.
- AC-1.4: Given the target URL has no saved cookies in Chrome, when the tool is called,
  then it returns the unauthenticated page content (graceful degradation).

---

### US-2 — Reuse the browser session across multiple calls

**As** an MCP user,
**I want** successive calls to `read_page_with_session` to reuse the same browser,
**So that** I avoid the cold-start overhead of spawning Chrome on every call.

**Acceptance Criteria**:

- AC-2.1: Given a browser session is already initialized, when a second call to
  `read_page_with_session` is made, then no new Chrome process is spawned.
- AC-2.2: Given the browser session crashes or becomes unresponsive, when the next call
  is made, then a new session is transparently initialized without requiring any user
  action.
- AC-2.3: The session is shared across all tools that use the session (feat-001 and
  feat-002) within the same MCP server process lifetime.

---

### US-3 — Connect to a running Chrome via remote debugging port (opt-in)

**As** an MCP power user who has Chrome running with `--remote-debugging-port`,
**I want to** pass a `debug_port` parameter to `read_page_with_session`,
**So that** the tool connects to my live browser and has access to my full session state
(including sessionStorage and in-memory state that is not persisted to disk).

**Acceptance Criteria**:

- AC-3.1: Given Chrome is running with `--remote-debugging-port=9222`, when
  `read_page_with_session` is called with `debug_port: 9222`, then the tool connects
  to that instance and skips cookie extraction.
- AC-3.2: Given `debug_port` is provided but nothing is listening on that port, when
  the tool is called, then it returns a clear error: "No Chrome instance found on port
  {port}. Make sure Chrome is running with --remote-debugging-port={port}."
- AC-3.3: When `debug_port` is provided, no filesystem access to the Chrome profile
  occurs.

---

### US-4 — Automatic session cleanup on Claude Code exit

**As** an MCP user,
**I want** Chrome to be closed automatically when I close Claude Code,
**So that** I do not accumulate orphaned Chrome processes over time.

**Acceptance Criteria**:

- AC-4.1: Given Claude Code is closed (graceful or SIGTERM), then no Chrome subprocess
  spawned by the session tools remains running.
- AC-4.2: Given an unrecoverable error occurs in the session, the Chrome subprocess is
  terminated before the error is returned to the caller.

---

## 6. User Experience

### Login once, reuse session across calls

The session browser's in-memory cookie store persists across all calls for the lifetime
of the Claude Code session. A login performed via feat-002's `act_with_session` sets
server-issued cookies (`Set-Cookie`) in the browser's in-memory jar; subsequent
`read_page_with_session` calls to the same domain find those cookies already present
and return authenticated content without re-login.

```
Call 1 — act_with_session (feat-002) — fills login form + clicks Sign in
→ Server responds with Set-Cookie: session_token=abc123
→ Browser stores session_token in its in-memory cookie jar

Call 2 — read_page_with_session (feat-001)
Input: { "url": "https://app.example.com/account/settings" }
→ Browser reused (same process, same in-memory cookie jar)
→ session_token=abc123 is already present — no login prompt
→ Returns authenticated page content directly

Call N — any subsequent read_page_with_session to the same domain
→ Session remains valid for the lifetime of the Claude Code session
→ User never re-enters credentials
```

### Tool Call — Read authenticated page (zero-config)

```
Tool: read_page_with_session
Input: { "url": "https://github.com/notifications" }

→ Detects Chrome profile at ~/Library/Application Support/Google/Chrome/Default/Cookies
→ Extracts and decrypts cookies for github.com
→ Spawns browser (first call only) / reuses existing session
→ Navigates to URL with cookies injected
→ Returns page text content
```

### Tool Call — Debug Port Path (power users)

```
Tool: read_page_with_session
Input: { "url": "https://myapp.internal/dashboard", "debug_port": 9222 }

→ Connects to chrome://localhost:9222 via CDP
→ Navigates to URL in existing Chrome session
→ Returns page text content
```

### Error Experience — Chrome Not Found

```
Error: "Chrome profile not found. Expected at:
  macOS: ~/Library/Application Support/Google/Chrome/Default/Cookies
  Linux: ~/.config/google-chrome/Default/Cookies

  Verify Google Chrome is installed, or use 'debug_port' to connect to a
  running Chrome instance."
```

---

## 7. Business Rules

| ID   | Rule                                                                                                                                                                                                                                                                                       |
|------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| BR-1 | Cookies are re-extracted from the Chrome profile on every `read_page_with_session` call so they are always fresh.                                                                                                                                                                          |
| BR-2 | The Chrome Cookies SQLite DB is opened in read-only mode. The tool must never write to or lock the user's Chrome profile.                                                                                                                                                                  |
| BR-3 | The browser session (Chrome subprocess) is a singleton shared across all session-aware tools within the same MCP server process.                                                                                                                                                           |
| BR-4 | If the session browser crashes, it is transparently re-initialized on the next call. The caller receives no indication that a restart occurred.                                                                                                                                            |
| BR-5 | When `debug_port` is provided, cookie extraction is skipped entirely. The two paths are mutually exclusive.                                                                                                                                                                                |
| BR-6 | The tool targets the Chrome `Default` profile only. No profile selection is supported in this version.                                                                                                                                                                                     |
| BR-7 | Cookies set by websites during navigation (e.g., `Set-Cookie` response headers from a login endpoint) are stored in the browser's in-memory cookie store and persist across all subsequent calls within the same Claude Code session. The user does not need to log in again on each call. |

---

## 8. Validations

| Input               | Validation                                  | Error                                              |
|---------------------|---------------------------------------------|----------------------------------------------------|
| `url`               | Must be a valid `http://` or `https://` URL | "Invalid URL: must start with http:// or https://" |
| `debug_port`        | Must be a valid port number (1–65535)       | "Invalid debug_port: must be between 1 and 65535"  |
| Chrome profile path | Must be accessible (file exists, readable)  | "Chrome profile not found at {path}"               |
| Cookies DB          | Must be a valid SQLite file                 | "Could not read Chrome cookies: {reason}"          |

---

## 9. Dependencies

| Dependency                                             | Type    | Notes                                                                         |
|--------------------------------------------------------|---------|-------------------------------------------------------------------------------|
| Google Chrome                                          | Runtime | Must be installed; version-independent for cookie format                      |
| OS Keychain (macOS)                                    | Runtime | Required for AES key derivation; triggers user permission dialog on first use |
| `rusqlite` crate                                       | Build   | SQLite read access for Cookies DB                                             |
| Crypto crates (`aes`, `cbc`, `pbkdf2`, `sha1`, `hmac`) | Build   | Cookie decryption                                                             |
| `security-framework` crate (macOS)                     | Build   | Keychain access                                                               |

---

## 10. Risks

| Risk                                              | Likelihood | Impact | Mitigation                                                                       |
|---------------------------------------------------|------------|--------|----------------------------------------------------------------------------------|
| Chrome changes cookie encryption scheme           | Low        | High   | Isolate decryption in `browser/cookies.rs`; version detection can be added later |
| Cookies DB locked by Chrome on Windows            | Medium     | Medium | Use WAL-mode read; retry once; document known limitation                         |
| macOS Keychain dialog surprises user on first use | Medium     | Low    | Document in README; expected OS behavior                                         |
| Session browser PID leaks on panic                | Low        | Medium | Register `Drop` on `BrowserSession`; document OS-level cleanup via SIGKILL       |

---

## 11. Edge Cases

| Scenario                                                                                 | Expected Behavior                                                                                                                          |
|------------------------------------------------------------------------------------------|--------------------------------------------------------------------------------------------------------------------------------------------|
| Chrome is not installed                                                                  | Error with expected profile paths                                                                                                          |
| Chrome profile is encrypted (enterprise policy)                                          | Error: "Cannot decrypt Chrome cookies: {reason}. Try using debug_port instead."                                                            |
| Target URL has no cookies in Chrome                                                      | Unauthenticated page returned (not an error)                                                                                               |
| Cookies are expired                                                                      | Unauthenticated or redirect-to-login page returned (not an error)                                                                          |
| `debug_port` provided, Chrome not running                                                | Error: "No Chrome instance found on port {port}"                                                                                           |
| `debug_port` provided, Chrome running without debug flag                                 | Same error as above                                                                                                                        |
| Session browser crashes between calls                                                    | Transparent re-initialization on next call                                                                                                 |
| Multiple concurrent calls on first initialization                                        | One initialization wins; others wait and use the same session                                                                              |
| User logs in via feat-002 `act_with_session`; next `read_page_with_session` to same site | Returns authenticated content — no re-login required (BR-7)                                                                                |
| Claude Code session ends and restarts                                                    | In-browser session cookies are gone; Chrome profile cookies are re-extracted on next call, but website session cookies require a new login |

---

## 12. Non-Goals (Explicit)

- This feature does NOT interact with any active Chrome tab or window. It operates in
  a separate headless Chrome process.
- This feature does NOT store, cache, or transmit any extracted cookies beyond the
  in-memory CDP injection.
- This feature does NOT require or modify `~/.mcp-web-porter` or any other config file.
- Concurrent batch page interactions are NOT part of this feature — see feat-002.
