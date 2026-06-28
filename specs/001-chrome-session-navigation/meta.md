# Feature Meta: chrome-session-navigation

## Identity

- **feature_name**: chrome-session-navigation
- **feature_number**: 001
- **feature_id**: feat-001
- **created_at**: 2026-06-09

## Context

- **project_mode**: brownfield
- **execution_mode**: standard
- **project_type**: production
- **technology**: rust
- **framework**: chromiumoxide (CDP), rmcp (MCP Rust SDK)
- **git_branch**: feature/allow-same-session-in-browser

## Description

Add a new MCP tool `read_page_with_session` that navigates pages using the cookies
from the user's actual Google Chrome installation — giving access to logged-in accounts
without requiring any Chrome reconfiguration. The browser instance is kept alive for the
entire Claude Code session and reused across calls; it is only torn down when the MCP
server process exits (i.e., when Claude Code is closed).

## Key Design Decisions

### 1. Cookie Extraction (zero-config, default path)

Extract cookies directly from Chrome's on-disk SQLite database while Chrome is running.
Decrypt them using the OS keychain and inject into the tool's browser via CDP before
navigation. No Chrome restart or debug-port flag needed.

**Auto-detected cookie DB paths per OS:**

- macOS: `~/Library/Application Support/Google/Chrome/Default/Cookies`
- Linux: `~/.config/google-chrome/Default/Cookies`
- Windows: `%LOCALAPPDATA%\Google\Chrome\User Data\Default\Network\Cookies`

**Decryption:**

- macOS: derive key from Keychain item `Chrome Safe Storage` via Security framework
  (use `security` CLI or `keychain` crate); AES-128-CBC with PBKDF2 from that key
- Linux: same PBKDF2 derivation but with hardcoded password `"peanuts"` + salt `"saltysalt"`
- Windows: DPAPI (`CryptUnprotectData`)

Cookies are re-extracted on each new tool call so they stay fresh.

### 2. Persistent Browser Session (process-lifetime)

The browser instance spawned by this tool is held in a process-level shared state
(`OnceLock<Arc<Mutex<BrowserSession>>>`). It is initialized lazily on the first
`read_page_with_session` call and reused for all subsequent calls within the same
Claude Code session. It is NOT explicitly closed between calls — it is dropped naturally
when the MCP server process exits (Claude Code session closed).

**Session lifecycle:**

```
First call  → spawn Chrome + inject cookies → hold in OnceLock
Nth call    → reuse existing browser → open new tab → navigate → return content → close tab
Claude exit → process drop → Chrome subprocess terminated automatically
```

**Browser options for this session:**

- Launches a fresh Chrome (no user-data-dir conflict with running Chrome)
- Cookies injected via CDP `Network.setCookies` before each navigation
- Same stealth mode, headers, and wait options as existing tools

### 3. Fallback: Remote Debugging Port (opt-in)

For users who want full live session access (localStorage, sessionStorage, active tabs),
an optional `debug_port: u16` parameter connects via `Browser::connect()` to a
Chrome instance already running with `--remote-debugging-port=<debug_port>`. When
`debug_port` is provided, cookie extraction is skipped entirely.

## New MCP Tool: `read_page_with_session`

**Parameters:**

- `url` (string, required)
- `stealth` (bool, optional): bypass bot detection
- `include_images` (bool, optional, default=true)
- `debug_port` (u16, optional): connect to existing Chrome at this port instead of
  extracting cookies (opt-in fallback)
- `headers` (object, optional): extra HTTP headers

**Returns:** same as `read_page` (text content + optional images)

## Implementation Scope

- `src/browser/session.rs` — NEW: `BrowserSession` struct with `OnceLock` global,
  `get_or_init()`, `open_tab()`, `close_tab()` methods
- `src/browser/cookies.rs` — NEW: `extract_chrome_cookies(profile: Option<&Path>)`
  with per-OS paths and decryption
- `src/browser/page.rs` — add `open_tab_in_session()` alongside existing `open()`
- `src/mcp/session_handler.rs` — NEW: `read_page_with_session` handler
- `src/main.rs` — expose `read_page_with_session` tool

## Dependencies to Add

- `rusqlite` — read Chrome's Cookies SQLite DB
- `aes` + `cbc` + `pbkdf2` + `hmac` + `sha1` — cookie decryption
- macOS only: `security-framework` crate (or shell out to `security` CLI for keychain)

## Testing Config (Production)

- **unit_coverage_target**: 80%
- **integration_tests**: required (cookie extraction mocked behind trait)
- **test_threads**: 1 (Chrome concurrency constraint)
- **fury_test**: n/a (non-Fury project)

## Open Questions

- Should `read_page_with_session` also support `detect_form` / `fill_form` / `click_button`
  variants that reuse the persistent session? (out of scope for now — start with read-only)
- Profile path override: expose `chrome_profile` parameter to support non-Default profiles
  (e.g., Profile 1, Profile 2)?

## Status

- [x] Functional spec — reopened (act_with_session moved to feat-002), needs re-approval
- [x] Technical spec — reopened (act_with_session moved to feat-002), needs re-approval
- [ ] Tasks
- [ ] Implementation
- [ ] Code review
