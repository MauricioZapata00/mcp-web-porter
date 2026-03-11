use mcp_web_porter::mcp::FormHandler;
use mcp_web_porter::HtmlResourceHandler as HtmlFetcher;
use mcp_web_porter::operations::fetch_html_with_options;
use mcp_web_porter::types::{FieldInput, ResourceUri};
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
struct ReadPageParams {
    #[schemars(description = "The URL to fetch (http:// or https://)")]
    url: String,
    #[schemars(description = "Enable stealth mode to bypass bot detection (e.g. news sites)")]
    stealth: Option<bool>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct DetectFormParams {
    #[schemars(description = "The URL to navigate to (http:// or https://)")]
    url: String,
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
}

#[derive(Clone)]
pub struct HtmlResourceServer {
    fetcher:      HtmlFetcher,
    form_handler: FormHandler,
    tool_router:  ToolRouter<Self>,
}

#[tool_router]
impl HtmlResourceServer {
    fn new() -> Self {
        Self {
            fetcher:      HtmlFetcher::new(),
            form_handler: FormHandler::default(),
            tool_router:  Self::tool_router(),
        }
    }

    #[tool(description = "Fetch a webpage and return its readable text content with JavaScript \
                          fully rendered. Set stealth=true to bypass bot detection on heavy \
                          commercial sites (news portals, paywalls).")]
    async fn read_page(
        &self,
        Parameters(ReadPageParams { url, stealth }): Parameters<ReadPageParams>,
    ) -> Result<CallToolResult, McpError> {
        let stealth = stealth.unwrap_or(false);
        let resource_uri = match ResourceUri::parse(&url) {
            Ok(u) => u,
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        };
        match fetch_html_with_options(resource_uri.url(), stealth).await {
            Ok(content) => Ok(CallToolResult::success(vec![Content::text(content.text())])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    #[tool(description = "Navigate to a URL and detect all form fields and buttons present after \
                          JavaScript renders.")]
    async fn detect_form(
        &self,
        Parameters(DetectFormParams { url }): Parameters<DetectFormParams>,
    ) -> Result<CallToolResult, McpError> {
        match self.form_handler.handle_detect(&url).await {
            Ok(step) => {
                let text = serde_json::to_string_pretty(&step)
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
        }): Parameters<FillFormParams>,
    ) -> Result<CallToolResult, McpError> {
        match self
            .form_handler
            .handle_fill(
                &url,
                fields,
                auto_submit.unwrap_or(false),
                submit_button_label,
                dry_run.unwrap_or(false),
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
