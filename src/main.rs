use mcp_web_porter::HtmlResourceHandler as HtmlFetcher;
use rmcp::{
    model::*,
    service::RequestContext,
    transport,
    ErrorData as McpError,
    RoleServer,
    ServerHandler, ServiceExt,
};
use std::borrow::Cow;

#[derive(Clone)]
pub struct HtmlResourceServer {
    fetcher: HtmlFetcher,
}

impl HtmlResourceServer {
    fn new() -> Self {
        Self {
            fetcher: HtmlFetcher::new(),
        }
    }
}

impl ServerHandler for HtmlResourceServer {
    async fn list_resource_templates(
        &self,
        _params: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, McpError> {
        let template = RawResourceTemplate {
            uri_template: "{url}".to_string(),
            name: "HTML Page Content".to_string(),
            title: None,
            description: Some(
                "Fetch HTML content from any URL. Use https://example.com".to_string(),
            ),
            mime_type: Some("text/html".to_string()),
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
                    mime_type: Some("text/html".to_string()),
                    text: content.html().to_string(),
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
                "HTML Resource Server - Provides HTML content from web pages via direct URLs"
                    .into(),
            ),
            capabilities: ServerCapabilities::builder()
                .enable_resources()
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
