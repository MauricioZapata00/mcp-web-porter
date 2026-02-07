# MCP Web Porter

> A high-performance Model Context Protocol (MCP) server for intelligent web automation and content extraction

[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

## What is Web Porter?

Web Porter bridges the gap between AI assistants and the modern web. It's an MCP server that gives AI the ability to:

- 📄 **Read any webpage** - Static sites, SPAs, or JavaScript-heavy applications
- 🔄 **Handle dynamic content** - Waits for JavaScript to render (React, Vue, Docsify, etc.)
- 📝 **Automate forms** - Fill and submit web forms programmatically
- 🖼️ **Extract images** - Capture images as base64 for AI analysis
- 📚 **Browse documentation** - Navigate multi-page doc sites automatically
- 🚀 **Scale effortlessly** - Handle 100+ concurrent browser sessions

Built with Rust for safety, speed, and concurrency using the Chrome DevTools Protocol.

## Why Web Porter?

Modern websites are increasingly JavaScript-dependent, making traditional scraping tools inadequate. Web Porter solves this by:

1. **Rendering First** - Lets JavaScript fully execute before extracting content
2. **Type-Safe** - Uses Rust's type system to prevent invalid states at compile time
3. **Concurrent** - Built on Tokio for efficient async operations
4. **AI-Optimized** - Returns content in formats ready for AI consumption
5. **MCP Native** - Built with the official [Model Context Protocol Rust SDK](https://github.com/modelcontextprotocol/rust-sdk) for standards-compliant tool implementation

## Quick Start

### Prerequisites

- Rust 1.75 or higher
- Chrome/Chromium installed (or access to remote Chrome endpoint)

### Installation
```bash
# Clone the repository
git clone https://github.com/yourusername/mcp-web-porter.git
cd mcp-web-porter

# Build the project
cargo build --release
```

### Configuration

Set up your environment variables:
```bash
export MCP_HOST=127.0.0.1
export MCP_PORT=3000
export MAX_SESSIONS=50
```

### Running the Server
```bash
# Development mode
cargo run

# Production mode
cargo run --release
```

### Using with MCP Clients

Web Porter works with any MCP-compatible client:

- **Claude Desktop** - Add to your MCP settings
- **Cursor** - Configure in MCP server settings
- **Windsurf** - Add as MCP provider
- **Cline** - Connect via MCP configuration
- **Other MCP clients** - Follow client-specific setup

Example MCP configuration:
```json
{
  "mcpServers": {
    "web-porter": {
      "command": "/path/to/mcp-web-porter/target/release/mcp-web-porter",
      "env": {
        "MAX_SESSIONS": "50"
      }
    }
  }
}
```

## Features

### 🌐 Universal Web Reading
- Handles static HTML and JavaScript-rendered content
- Automatic waiting for page load and JavaScript execution
- Extracts clean text content from complex layouts

### 📝 Form Automation
- Intelligent form detection and field mapping
- Supports text, email, password, select, checkbox, and textarea fields
- Validation before submission
- Optional auto-submit

### 🖼️ Image Extraction
- Extracts images with metadata (URL, alt text, dimensions)
- Converts to base64 for AI context
- Automatic optimization for token efficiency

### 🏗️ Architecture Highlights
- **Type-Driven Development** - Invalid states unrepresentable at compile time
- **Typestate Pattern** - Browser sessions enforce correct state transitions
- **Pure Business Logic** - Easy testing without mocks
- **Effect Isolation** - Clear boundaries between I/O and logic
- **MCP SDK Based** - Official Rust SDK for protocol compliance and type safety

### ⚡ Performance
- Built on `spider_chromiumoxide_cdp` for high concurrency
- Semaphore-based session limiting
- Configurable wait times and timeouts
- Scales to 100+ concurrent sessions

## Use Cases

- **Documentation Indexing** - Scrape entire doc sites (Docsify, VuePress, etc.)
- **Form Testing** - Automated form filling and validation
- **Content Monitoring** - Track changes on JavaScript-heavy sites
- **AI Research** - Provide web content to AI assistants
- **Data Collection** - Extract structured data from dynamic pages

## Architecture

Web Porter uses a hybrid module-centric architecture with the official MCP Rust SDK:
```
mcp-web-porter/
├── types/          # Domain types with compile-time guarantees
├── operations/     # Pure business logic (testable without mocks)
├── browser/        # Chrome DevTools Protocol client
├── mcp/            # MCP protocol implementation using rust-sdk
└── server/         # Application composition
```

### MCP SDK Integration

This project leverages the **[Model Context Protocol Rust SDK](https://github.com/modelcontextprotocol/rust-sdk)** for:
- Type-safe tool definitions and handlers
- Standards-compliant protocol implementation
- Built-in request/response validation
- Server lifecycle management

## Configuration

### Environment Variables

| Variable | Description | Default | Example |
|----------|-------------|---------|---------|
| `MCP_HOST` | Server bind address | `127.0.0.1` | `0.0.0.0` |
| `MCP_PORT` | Server port | `3000` | `8080` |
| `MAX_SESSIONS` | Max concurrent browser sessions | `100` | `150` |
| `RUST_LOG` | Logging level | `info` | `debug` |

### Browser Configuration

The server automatically configures Chrome/Chromium with optimal settings for web automation. Advanced users can modify browser settings in the configuration file.

## Deployment

### Docker
```bash
# Build image
docker build -t mcp-web-porter .

# Run container
docker run -p 3000:3000 -e MAX_SESSIONS=150 mcp-web-porter
```

### Docker Compose
```yaml
version: '3.8'

services:
  mcp-web-porter:
    build: .
    ports:
      - "3000:3000"
    environment:
      - MCP_HOST=0.0.0.0
      - MCP_PORT=3000
      - MAX_SESSIONS=100
```

## Development

### Running Tests
```bash
# Run all tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Run specific test
cargo test test_render_page
```

### Building for Production
```bash
# Optimized release build
cargo build --release

# The binary will be at: target/release/mcp-web-porter
```

## Troubleshooting

### Common Issues

**Chrome crashes or fails to start:**
```bash
# Ensure Chrome is installed
which google-chrome

# Or specify Chrome path
export CHROME_PATH=/usr/bin/chromium
```

**Timeout errors on heavy JavaScript sites:**
- Increase `wait_time_ms` in your requests
- Check network connectivity
- Verify the site loads correctly in a regular browser

**High memory usage:**
- Reduce `MAX_SESSIONS` value
- Monitor with `docker stats` if using Docker
- Consider using remote Chrome instance for scaling

## Contributing

Contributions are welcome! Please follow these steps:

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

Please ensure:
- All tests pass (`cargo test`)
- Code is formatted (`cargo fmt`)
- No clippy warnings (`cargo clippy`)

## Acknowledgments

- Implements the [Model Context Protocol Rust SDK](https://github.com/modelcontextprotocol/rust-sdk)
- Built with [spider_chromiumoxide_cdp](https://crates.io/crates/spider_chromiumoxide_cdp) for browser automation
- Uses [scraper](https://crates.io/crates/scraper) for HTML parsing
- Powered by [Tokio](https://tokio.rs/) async runtime
- Follows the [Model Context Protocol](https://modelcontextprotocol.io/) specification

## Links

- [Issue Tracker](https://github.com/yourusername/mcp-web-porter/issues)
- [Changelog](CHANGELOG.md)
- [Model Context Protocol](https://modelcontextprotocol.io/)