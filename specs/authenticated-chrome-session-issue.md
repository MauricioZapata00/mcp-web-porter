# Improve authenticated Chrome session handling without direct cookie DB access

## Problem

`read_page_with_session` currently tries to authenticate by reading the user's Chrome cookie SQLite database directly:

```text
~/Library/Application Support/Google/Chrome/Default/Cookies
```

On macOS this is fragile because Chrome profile data is protected by privacy controls. When the MCP server process does
not have Full Disk Access, the tool fails before navigation with errors like:

```text
Could not read Chrome cookies database: unable to open database file
```

or, when checked directly:

```text
authorization denied
```

This makes authenticated/internal-page access depend on macOS Full Disk Access and Keychain/cookie decryption behavior.
Playwright-style browser automation usually avoids this by either owning its browser profile or connecting to a running
browser through DevTools instead of scraping Chrome's profile database.

Related context: https://github.com/MauricioZapata00/mcp-web-porter/issues/6 already discusses remote debugging for
authenticated internal pages. This issue focuses specifically on making `read_page_with_session` and session-based tools
avoid direct Chrome cookie DB access as the primary path.

## Observed behavior

Calling `read_page_with_session` against an authenticated page can fail before loading the URL because the MCP process
cannot open Chrome's cookie database.

Passing `debug_port` does not currently avoid this path. The parameter exists in the tool schema, but the session
implementation still attempts cookie extraction instead of connecting to the existing Chrome instance.

## Desired behavior

Session-aware tools should support authenticated browsing without requiring direct access to Chrome's protected cookie
database.

At minimum, when a debug port is provided, the tool should connect to the existing Chrome instance and use that live
browser session instead of reading/decrypting cookies manually.

## Option 1: Connect to existing Chrome via DevTools

Add real support for `debug_port` / remote debugging in session-aware tools.

User flow:

```bash
/Applications/Google\ Chrome.app/Contents/MacOS/Google\ Chrome \
  --remote-debugging-port=9222
```

Then call:

```json
{
  "url": "https://internal.example.com/page",
  "debug_port": 9222
}
```

Implementation idea:

- If `debug_port` is provided, connect to `http://127.0.0.1:{debug_port}` via CDP/chromiumoxide.
- Reuse the user's already-authenticated Chrome session.
- Do not read `Default/Cookies`.
- Do not require Full Disk Access.
- Apply this to `read_page_with_session` and `act_with_session`.

Pros:

- Closest to how Playwright avoids cookie DB access when connecting to an existing browser.
- Reuses real authenticated browser state, including SSO, local storage, session storage, and any browser-managed auth
  state.
- Avoids macOS privacy prompts for Chrome profile files.

Cons:

- Requires the user to launch Chrome with remote debugging enabled.
- Needs careful tab/page lifecycle handling so the tool does not disturb active user tabs unexpectedly.

## Option 2: Use a persistent web-porter browser profile

Instead of reading the user's real Chrome profile, launch Chromium with a stable profile owned by web-porter, for
example:

```text
~/.mcp-web-porter/chrome-profile
```

The user logs in once in that profile. Subsequent `read_page_with_session` calls reuse the same profile and preserve
cookies/session state.

Pros:

- Avoids Full Disk Access because web-porter owns the profile directory.
- Does not require launching the user's normal Chrome with remote debugging.
- Similar to Playwright's `launchPersistentContext` model.

Cons:

- It is a separate browser identity, not the user's active Chrome session.
- The user must log in once inside the web-porter profile.
- Profile lifecycle and cleanup need to be explicit.

## Option 3: Support storage-state import/export

Add support for importing a storage state file containing cookies and browser storage, similar to Playwright's
`storageState` pattern.

Example flow:

1. User authenticates through an automation/bootstrap flow.
2. Session state is exported to a JSON file.
3. web-porter imports that state and injects cookies/localStorage/sessionStorage before navigation.

Pros:

- Explicit and portable.
- Avoids scraping Chrome's protected profile DB.
- Works well for CI and repeatable test setups.

Cons:

- Requires a separate state capture step.
- Session state can expire and must be refreshed.
- Needs implementation for localStorage/sessionStorage in addition to cookies.

## Recommendation

Implement Option 1 first because the public tool schema already exposes `debug_port`. The current behavior is
surprising: users can pass `debug_port`, but session tools still attempt direct cookie extraction.

After that, Option 2 would be a good default for users who do not want to run Chrome with remote debugging. Option 3 is
useful for CI and repeatable authenticated test workflows.

## Acceptance criteria

- `read_page_with_session` uses `debug_port` when provided and does not attempt to read Chrome's cookie DB in that path.
- `act_with_session` follows the same behavior.
- If `debug_port` is omitted, the existing cookie extraction fallback can remain.
- Documentation explains the three supported authentication/session strategies and their tradeoffs.
- macOS users can access authenticated pages through the debug-port path without granting Full Disk Access.
