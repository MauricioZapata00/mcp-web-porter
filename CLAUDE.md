# MCP Web Automation Server

A high-performance Model Context Protocol (MCP) server for web content extraction, form automation, and documentation scraping. Built with Rust for speed, safety, and concurrent operation handling.

## Overview

This MCP server provides AI assistants with powerful web automation capabilities:
- 📄 Read HTML content from static and dynamic websites
- 🔄 Handle JavaScript-heavy SPAs (Single Page Applications) by letting JS render
- 📝 Fill and submit web forms programmatically
- 🖼️ Extract images from web pages (as base64 for AI context)
- 📚 Read Docsify documentation sites (JS-rendered, no plugin handling needed)
- 🚀 Handle 10-100+ concurrent browser sessions

## MCP Implementation

This project uses the official **[Model Context Protocol Rust SDK](https://github.com/modelcontextprotocol/rust-sdk)** to implement MCP tools and resources. The SDK provides:
- Type-safe protocol implementation
- Built-in request/response handling
- Tool and resource registration
- Server lifecycle management

## Architecture

**Type:** Hybrid Module-Centric with Type-Driven Development
```
mcp-web-porter/
├── types/          # Core domain types with compile-time guarantees (type-driven!)
├── operations/     # Pure business logic (testable without mocks)
├── browser/        # Chrome DevTools Protocol client
├── mcp/            # MCP protocol implementation using rust-sdk
└── server/         # Application composition
```

### Key Design Principles

1. **Type-Driven Development**: Use Rust's type system to make invalid states unrepresentable
2. **Pure Core**: Business logic is pure functions (easy testing, no mocks)
3. **Clear Effect Boundaries**: I/O operations isolated in specific modules
4. **Typestate Pattern**: Browser sessions use compile-time state machines
5. **Concurrent by Design**: Built for 100+ parallel browser sessions
6. **JS-First**: Always let JavaScript render, scrape the final result
7. **MCP SDK Integration**: Use official rust-sdk for protocol compliance

## Technology Stack

### Core Dependencies
- **MCP SDK**: Model Context Protocol Rust SDK for protocol implementation
- **Async Runtime**: tokio for async operations
- **Browser Automation**: spider_chromiumoxide_cdp for high-concurrency browser control
- **HTML Parsing**: scraper, html5ever for parsing rendered content
- **HTTP Client**: reqwest for image downloads and network operations
- **Image Processing**: image crate for manipulation, base64 for encoding
- **Serialization**: serde for data handling
- **Error Handling**: thiserror and anyhow for robust error management
- **Logging**: tracing for structured logging

## Project Structure

### 1. Types Crate (`crates/types`)

**Purpose:** Core domain types with compile-time guarantees

Key types include:
- **BrowserSession**: Typestate pattern for browser sessions (Disconnected → Connected → PageLoaded)
- **RenderedPage**: Represents a fully-rendered web page with HTML, text content, and metadata
- **Form**: Form structure with fields (Text, Email, Password, Checkbox, Radio, Select, TextArea)
- **FilledForm**: Validated form ready for submission
- **ExtractedImage**: Image data with URL, alt text, and base64 encoding for AI context
- **ImageData**: Enum for URL or Base64 image representations

### 2. Operations Crate (`crates/operations`)

**Purpose:** Pure business logic (testable without mocks)

Key modules:
- **HTML Parsing**: Extract forms, images, text content, and main content from rendered HTML
- **Form Automation**: Create form fill strategies with validation (handles Text, Select, Checkbox, TextArea fields)
- **Image Processing**: Convert images to base64, optimize for AI context (resize to 2048x2048, convert to JPEG)

All functions are pure (no I/O), making them easily testable without mocks.

### 3. Browser Crate (`crates/browser`)

**Purpose:** Chrome DevTools Protocol client implementation

Key functionality:
- **ChromeClient**: Manages browser instances and page creation
- **Page Rendering**: Navigate to URL, wait for network idle, wait additional time for JS frameworks
- **Form Filling**: Execute fill strategies with delays between actions for reliability
- **Image Fetching**: Handle data URLs and HTTP fetches with optimization
- **JavaScript Execution**: Support for custom JS evaluation

The client automatically waits for JavaScript to render (default 1000ms, configurable).

### 4. MCP Crate (`crates/mcp`)

**Purpose:** MCP protocol implementation using the official Rust SDK

**Resources Provided:**
1. **html**: Read HTML content from any webpage
   - URI pattern: Direct URL (e.g., `https://example.com`)
   - Returns: Raw HTML content from the specified URL
   - Use case: Deliver webpage content as data for AI consumption

**Tools Provided:**
1. **fill_form**: Fill and optionally submit web forms
   - Parameters: url, form_selector (optional), data, submit (optional)

2. **extract_images**: Extract images as base64 for AI analysis
   - Parameters: url, max_images (optional)

**Resource and Tool Handlers:**
- Use semaphore for concurrency control (respects MAX_SESSIONS)
- Coordinate between ChromeClient and operations modules
- Handle errors gracefully with proper error types
- Resources deliver data, tools perform actions

### 5. Server Crate (`crates/server`)

**Purpose:** Application composition and MCP server initialization using the Rust SDK

**Server Initialization:**
1. Initialize tracing for structured logging
2. Load configuration from environment variables
3. Initialize ChromeClient with browser configuration
4. Create ToolHandlers with concurrency limits
5. Register tools with MCP server using rust-sdk
6. Start server and handle requests

**Configuration:**
- Loaded from environment variables (MCP_HOST, MCP_PORT, MAX_SESSIONS)
- Browser configured for headless operation with optimal viewport size
- Concurrency controlled via semaphore

## Usage Examples

### Reading a Docsify Site
Use the `read_page` tool to read JavaScript-rendered documentation sites. Specify the URL and optionally request image extraction with configurable wait time for slow-loading content.

### Reading an SPA
The `read_page` tool automatically handles Single Page Applications by waiting for JavaScript to render before extracting content.

### Filling a Form
Use the `fill_form` tool to programmatically fill form fields. Provide the URL, form data as key-value pairs, and optionally submit the form after filling.

### Extracting Images
The `extract_images` tool extracts images from a webpage and converts them to base64 format for AI analysis. You can limit the number of images extracted.

## Running the Server

### Local Development (10-20 sessions)
```bash
# Set environment variables
export MCP_HOST=127.0.0.1
export MCP_PORT=3000
export MAX_SESSIONS=20
export RUST_LOG=info

# Run in development mode
cargo run --release
```

### Production (100+ sessions)

For 100+ concurrent sessions, deploy to a cloud instance with:
- **4+ CPU cores** (8+ recommended)
- **8GB+ RAM** (16GB+ recommended)
- **SSD storage**
```bash
# Production configuration
export MCP_HOST=0.0.0.0
export MCP_PORT=3000
export MAX_SESSIONS=150
export RUST_LOG=info

# Build optimized binary
cargo build --release

# Run
./target/release/mcp-server
```

### Docker Deployment
Multi-stage Docker build with Chrome installation. The runtime image includes Chrome stable for browser automation. Configure via environment variables for host, port, and max sessions.

### Docker Compose (with separate Chrome)

For extreme scaling, run Chrome separately using browserless/chrome image. The MCP server connects to the Chrome instance via WebSocket. This setup allows for better resource isolation and horizontal scaling.

## Testing

### Unit Tests (Pure Functions)
Test pure business logic functions without mocks:
- HTML parsing (forms, images, text extraction)
- Form fill strategy creation and validation
- Image optimization and base64 encoding

### Integration Tests (With Browser)
Test browser automation with real Chrome instances:
- Rendering static pages
- Rendering JavaScript-heavy pages (Docsify, SPAs)
- Form filling and submission
- Image extraction

## Performance Optimization

### Concurrency Control
The server uses a semaphore to limit concurrent browser sessions. Each handler acquires a permit before processing, ensuring MAX_SESSIONS is respected.

### Resource Management
Browser pool for efficient reuse of browser instances with round-robin selection.

### Memory Optimization
Automatic page cleanup using Drop trait to close pages after use.

## Deployment Strategies

### Single Instance (10-50 sessions)
**Setup:**
- Local Chrome instance
- Direct CDP connection
- Simple configuration

**Use case:** Development, small teams

### Remote Chrome (50-150 sessions)
**Setup:**
- Separate Chrome instance on dedicated server
- Connect via WebSocket
- Better resource isolation

**Use case:** Production, medium scale

**Configuration:**
Run Chrome server on dedicated instance and configure MCP server to connect via WebSocket endpoint.

### Chrome Grid (150+ sessions)
**Setup:**
- Multiple Chrome instances behind load balancer
- Session routing
- Horizontal scaling

**Use case:** High-volume production

## Troubleshooting

### Common Issues

**1. Chrome Crashes**
- Solution: Increase memory allocation or reduce MAX_SESSIONS
- For Docker: increase memory limit

**2. Timeout Errors**
- Solution: Increase wait_time_ms parameter for heavy JavaScript sites
- Check network connectivity

**3. Form Filling Fails**
- Verify CSS selectors are correct
- Add delays between actions
- Check if page uses shadow DOM

**4. Image Extraction Slow**
- Limit concurrent image fetches
- Optimize image sizes
- Use connection pooling

**5. Memory Leaks**
- Monitor memory usage with system tools
- Implement automatic restart strategy for long-running servers

### Debug Mode
- Enable verbose logging: set RUST_LOG=debug
- Run with visible Chrome (not headless) for debugging

### Testing Against Real Sites
Test with common frameworks (React, Vue, Docsify) to ensure proper JavaScript rendering.

## API Examples

The server exposes MCP tools that can be called through any MCP-compatible client. Tools include read_page, fill_form, and extract_images.

## Configuration Reference

### Environment Variables

| Variable | Description | Default | Example |
|----------|-------------|---------|---------|
| `MCP_HOST` | Server bind address | `127.0.0.1` | `0.0.0.0` |
| `MCP_PORT` | Server port | `3000` | `8080` |
| `MAX_SESSIONS` | Max concurrent browser sessions | `100` | `150` |
| `CHROME_WS_ENDPOINT` | Remote Chrome WebSocket URL | None | `ws://chrome:9222` |
| `RUST_LOG` | Logging level | `info` | `debug` |

### Browser Configuration
Browser configured with viewport size 1920x1080, headless mode, and Chrome args optimized for Docker (--no-sandbox, --disable-dev-shm-usage, --disable-gpu).

## Best Practices

### 1. Always Wait for JavaScript
The library automatically waits 1 second after navigation. For heavier sites, increase wait_time_ms parameter.

### 2. Handle Timeouts Gracefully
Implement timeout handling with configurable duration for page rendering operations.

### 3. Limit Image Sizes
Always optimize images for AI context - automatically resized to 2048x2048 and converted to JPEG.

### 4. Use Semaphore for Concurrency
Already implemented - respects MAX_SESSIONS automatically.

### 5. Monitor Resource Usage
Monitor memory and CPU usage with system tools to ensure optimal performance.

## Security Considerations

### 1. URL Validation
Validate URLs before processing and block local/private IPs to prevent SSRF attacks.

### 2. Rate Limiting
Implement rate limiting to prevent abuse (e.g., using governor crate).

### 3. Sanitize Form Data
Remove scripts and validate inputs before processing form data.

## Future Enhancements

- [ ] Screenshot capture tool
- [ ] PDF generation from pages
- [ ] Cookie/session management
- [ ] Proxy rotation support
- [ ] WebSocket streaming for real-time updates
- [ ] Caching layer for frequently accessed pages
- [ ] Batch processing endpoints
- [ ] Custom JavaScript injection

## Performance Benchmarks

Expected performance on 4-core, 8GB RAM:

| Concurrent Sessions | Pages/Second | Avg Response Time |
|---------------------|--------------|-------------------|
| 10 | ~8 | 1.2s |
| 50 | ~35 | 1.5s |
| 100 | ~60 | 2.0s |
| 150 | ~75 | 2.5s |

*Benchmarks for typical web pages with moderate JavaScript*

## Contributing

1. Fork the repository
2. Create feature branch
3. Write tests
4. Submit PR

## License

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

---

## Quick Start
1. Clone and build the project with `cargo build --release`
2. Configure environment variables (MAX_SESSIONS, MCP_PORT)
3. Run with `cargo run --release`

## Support

- GitHub Issues: [Report bugs](https://github.com/your-org/mcp-web-automation/issues)
- Documentation: [Full docs](https://docs.example.com)
- Discord: [Community chat](https://discord.gg/example)