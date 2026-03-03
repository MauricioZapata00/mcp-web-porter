# MCP Web Porter

## MCP Implementation

This project uses the official **[Model Context Protocol Rust SDK](https://github.com/modelcontextprotocol/rust-sdk)** to implement MCP tools and resources.

## Architecture

```
mcp-web-porter/
├── types/          # Core domain types with compile-time guarantees (type-driven!)
├── operations/     # Pure business logic (testable without mocks)
├── browser/        # Chrome DevTools Protocol client
├── mcp/            # MCP protocol implementation using rust-sdk
└── server/         # Application composition
```

## Unit Testing

- Tests live in `#[cfg(test)] mod tests { use super::*; ... }` at the bottom of the same file as the code under test — never in separate test files
- Cover all lines and edge cases (happy path, error variants, boundary values)
- **Pure functions** (`types/`, `operations/parse.rs`): use plain `#[test]` — no async, no mocks needed
- **Browser/HTTP integration** (`operations/fetch.rs`, `mcp/`): use `#[tokio::test(flavor = "multi_thread")]` + `#[serial_test::serial]`
- **Error path coverage for browser ops** (`browser/page.rs`): extract I/O behind a trait, annotate it with `#[cfg_attr(test, mockall::automock)]`, then inject a `MockBrowserOps` to force each error variant independently
- Run browser integration tests with `-- --test-threads=1` to avoid Chrome concurrency failures

## Code Style

- Do NOT add comments to code unless the logic is truly non-obvious
- When the user requests a commit message, use imperative mood (e.g. "Add stealth mode" not "Added stealth mode")
