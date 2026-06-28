use mcp_web_porter::mcp::{FormHandler, SessionHandler};
use mcp_web_porter::HtmlResourceHandler as HtmlFetcher;
use mcp_web_porter::operations::{fetch_html_with_options, fetch_html_with_images_options};
use mcp_web_porter::types::{Cookie, FieldInput, RequestOptions, ResourceUri};
use std::collections::HashMap;
use base64::Engine;
use rmcp::{
    ErrorData as McpError,
    RoleServer,
    ServerHandler,
    ServiceExt,
    handler::server::router::tool::ToolRouter,
    handler::server::wrapper::Parameters,
    model::*,
    service::RequestContext,
    tool, tool_handler, tool_router,
    transport,
};
use std::borrow::Cow;

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct CookieParam {
    #[schemars(description = "Cookie name")]
    name:   String,
    #[schemars(description = "Cookie value")]
    value:  String,
    #[schemars(description = "Domain the cookie is scoped to (defaults to target URL host)")]
    domain: Option<String>,
    #[schemars(description = "Path the cookie is scoped to (defaults to /)")]
    path:   Option<String>,
}

fn into_request_options(
    cookies: Option<Vec<CookieParam>>,
    headers: Option<HashMap<String, String>>,
) -> Option<RequestOptions> {
    match (cookies, headers) {
        (None, None) => None,
        (cookies, headers) => Some(RequestOptions {
            cookies: cookies.map(|cs| {
                cs.into_iter()
                    .map(|c| Cookie { name: c.name, value: c.value, domain: c.domain, path: c.path })
                    .collect()
            }),
            headers,
        }),
    }
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct ReadPageParams {
    #[schemars(description = "The URL to fetch (http:// or https://)")]
    url: String,
    #[schemars(description = "Enable stealth mode to bypass bot detection (e.g. news sites)")]
    stealth: Option<bool>,
    #[schemars(description = "Include images from the page as image content items alongside the text (default: true)")]
    include_images: Option<bool>,
    #[schemars(description = "Cookies to set before navigation")]
    cookies: Option<Vec<CookieParam>>,
    #[schemars(description = "Extra HTTP headers to send with every request")]
    headers: Option<HashMap<String, String>>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct DetectFormParams {
    #[schemars(description = "The URL to navigate to (http:// or https://)")]
    url: String,
    #[schemars(description = "Cookies to set before navigation")]
    cookies: Option<Vec<CookieParam>>,
    #[schemars(description = "Extra HTTP headers to send with every request")]
    headers: Option<HashMap<String, String>>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct FillFormParams {
    #[schemars(description = "The URL of the page with the form (http:// or https://)")]
    url: String,
    #[schemars(description = "Fields to fill, each with identifier and value")]
    fields: Vec<FieldInput>,
    #[schemars(description = "Submit the form automatically after filling")]
    auto_submit: Option<bool>,
    #[schemars(description = "Click a continuation button by its text label instead of submitting")]
    submit_button_label: Option<String>,
    #[schemars(description = "Preview planned changes without touching the DOM")]
    dry_run: Option<bool>,
    #[schemars(description = "Cookies to set before navigation")]
    cookies: Option<Vec<CookieParam>>,
    #[schemars(description = "Extra HTTP headers to send with every request")]
    headers: Option<HashMap<String, String>>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct ClickButtonParams {
    #[schemars(description = "The URL of the page containing the button (http:// or https://)")]
    url: String,
    #[schemars(description = "The HTML id attribute of the button to click")]
    button_id: Option<String>,
    #[schemars(description = "Visible text label of the button (case-insensitive partial match)")]
    label: Option<String>,
    #[schemars(description = "Cookies to set before navigation")]
    cookies: Option<Vec<CookieParam>>,
    #[schemars(description = "Extra HTTP headers to send with every request")]
    headers: Option<HashMap<String, String>>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct ReadPageWithSessionParams {
    #[schemars(description = "The URL to fetch (http:// or https://)")]
    url: String,
    #[schemars(description = "Enable stealth mode to bypass bot detection")]
    stealth: Option<bool>,
    #[schemars(description = "Include images from the page as image content items (default: true)")]
    include_images: Option<bool>,
    #[schemars(description = "Connect to existing Chrome at this debug port instead of extracting cookies (opt-in)")]
    debug_port: Option<u16>,
    #[schemars(description = "Extra HTTP headers to send with every request")]
    headers: Option<HashMap<String, String>>,
}

#[derive(Clone)]
pub struct HtmlResourceServer {
    fetcher:         HtmlFetcher,
    form_handler:    FormHandler,
    session_handler: SessionHandler,
    tool_router:     ToolRouter<Self>,
}

#[tool_router]
impl HtmlResourceServer {
    fn new() -> Self {
        Self {
            fetcher:         HtmlFetcher::new(),
            form_handler:    FormHandler::default(),
            session_handler: SessionHandler::default(),
            tool_router:     Self::tool_router(),
        }
    }

    #[tool(description = "Fetch a webpage and return its readable text content with JavaScript \
                          fully rendered. Images embedded in the page are returned as image \
                          content items alongside the text (set include_images=false to skip). \
                          Set stealth=true to bypass bot detection on heavy commercial sites.")]
    async fn read_page(
        &self,
        Parameters(ReadPageParams { url, stealth, include_images, cookies, headers }): Parameters<ReadPageParams>,
    ) -> Result<CallToolResult, McpError> {
        let stealth = stealth.unwrap_or(false);
        let include_images = include_images.unwrap_or(true);
        let resource_uri = match ResourceUri::parse(&url) {
            Ok(u) => u,
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        };
        let options = into_request_options(cookies, headers);
        if include_images {
            match fetch_html_with_images_options(resource_uri.url(), stealth, options.as_ref()).await {
                Ok((content, images)) => {
                    let mut items = vec![Content::text(content.text())];
                    for img in images {
                        items.push(Content::image(img.data().to_string(), img.mime_type().to_string()));
                    }
                    Ok(CallToolResult::success(items))
                }
                Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
            }
        } else {
            match fetch_html_with_options(resource_uri.url(), stealth, options.as_ref()).await {
                Ok(content) => Ok(CallToolResult::success(vec![Content::text(content.text())])),
                Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
            }
        }
    }

    #[tool(description = "Navigate to a URL and detect all form fields and buttons present after \
                          JavaScript renders.")]
    async fn detect_form(
        &self,
        Parameters(DetectFormParams { url, cookies, headers }): Parameters<DetectFormParams>,
    ) -> Result<CallToolResult, McpError> {
        let options = into_request_options(cookies, headers);
        match self.form_handler.handle_detect(&url, options).await {
            Ok(step) => {
                let text = serde_json::to_string_pretty(&step)
                    .unwrap_or_else(|_| "{}".to_string());
                Ok(CallToolResult::success(vec![Content::text(text)]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    #[tool(description = "Find and click a button on a web page by its HTML id or visible text \
                          label. Returns the resolved button, how it was found, the page text \
                          after clicking, and any new form fields that appear.")]
    async fn click_button(
        &self,
        Parameters(ClickButtonParams { url, button_id, label, cookies, headers }): Parameters<ClickButtonParams>,
    ) -> Result<CallToolResult, McpError> {
        let options = into_request_options(cookies, headers);
        match self.form_handler.handle_click_button(&url, button_id, label, options).await {
            Ok(result) => {
                let text = serde_json::to_string_pretty(&result)
                    .unwrap_or_else(|_| "{}".to_string());
                Ok(CallToolResult::success(vec![Content::text(text)]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    #[tool(description = "Fill form fields on a web page and optionally submit. Supports \
                          dry_run preview and continuation buttons.")]
    async fn fill_form(
        &self,
        Parameters(FillFormParams {
            url,
            fields,
            auto_submit,
            submit_button_label,
            dry_run,
            cookies,
            headers,
        }): Parameters<FillFormParams>,
    ) -> Result<CallToolResult, McpError> {
        let options = into_request_options(cookies, headers);
        match self
            .form_handler
            .handle_fill(
                &url,
                fields,
                auto_submit.unwrap_or(false),
                submit_button_label,
                dry_run.unwrap_or(false),
                options,
            )
            .await
        {
            Ok(result) => {
                let text = serde_json::to_string_pretty(&result)
                    .unwrap_or_else(|_| "{}".to_string());
                Ok(CallToolResult::success(vec![Content::text(text)]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    #[tool(description = "Fetch a webpage using the user's Chrome session cookies. Access pages that require authentication without manual credential entry. Automatically extracts cookies from Google Chrome's profile and injects them into a persistent browser session that lasts the entire Claude Code session. Set stealth=true to bypass bot detection. Images are returned as image content items (set include_images=false to skip).")]
    async fn read_page_with_session(
        &self,
        Parameters(ReadPageWithSessionParams { url, stealth: _, include_images, debug_port: _, headers }): Parameters<ReadPageWithSessionParams>,
    ) -> Result<CallToolResult, McpError> {
        let include_images = include_images.unwrap_or(true);

        match self.session_handler.handle_read_with_session(&url, false, include_images, None, headers).await {
            Ok((content, image_urls)) => {
                let mut items = vec![Content::text(content)];
                let client = reqwest::Client::new();
                for img_url in image_urls {
                    if let Ok(response) = client.get(&img_url).send().await {
                        if let Ok(bytes) = response.bytes().await {
                            let base64_data = base64::engine::general_purpose::STANDARD.encode(&bytes);
                            let mime_type = "image/png".to_string();
                            items.push(Content::image(base64_data, mime_type));
                        }
                    }
                }
                Ok(CallToolResult::success(items))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }
}

#[tool_handler]
impl ServerHandler for HtmlResourceServer {
    async fn list_resource_templates(
        &self,
        _params: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, McpError> {
        let template = RawResourceTemplate {
            uri_template: "{url}".to_string(),
            name: "Web Page Text Content".to_string(),
            title: None,
            description: Some(
                "Fetch and extract readable text from any URL. JavaScript is fully rendered before extraction. Use https://example.com".to_string(),
            ),
            mime_type: Some("text/plain".to_string()),
            icons: None,
        };

        Ok(ListResourceTemplatesResult {
            resource_templates: vec![Annotated::new(template, None)],
            next_cursor: None,
            meta: None,
        })
    }

    async fn read_resource(
        &self,
        params: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        match self.fetcher.handle_read(&params.uri).await {
            Ok(content) => Ok(ReadResourceResult {
                contents: vec![ResourceContents::TextResourceContents {
                    uri: params.uri.clone(),
                    mime_type: Some("text/plain".to_string()),
                    text: content.text().to_string(),
                    meta: None,
                }],
            }),
            Err(e) => Err(McpError {
                code: ErrorCode(-32603),
                message: Cow::Owned(e.to_string()),
                data: None,
            }),
        }
    }

    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "Web Porter - Provides readable text extracted from web pages. JavaScript-rendered content (SPAs, Docsify, React, Vue) is fully supported."
                    .into(),
            ),
            capabilities: ServerCapabilities::builder()
                .enable_resources()
                .enable_tools()
                .build(),
            ..Default::default()
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    eprintln!("MCP Web Porter - HTML Resource Server");
    eprintln!("Ready to serve HTML resources via direct URLs");

    let service = HtmlResourceServer::new()
        .serve(transport::io::stdio())
        .await
        .inspect_err(|e| {
            eprintln!("Error starting server: {}", e);
        })?;

    service.waiting().await?;

    Ok(())
}
