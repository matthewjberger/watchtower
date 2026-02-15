use rmcp::{
    ServerHandler,
    handler::server::{router::tool::ToolRouter, tool::Parameters},
    model::*,
    tool, tool_handler, tool_router,
    transport::streamable_http_server::{
        StreamableHttpService, session::local::LocalSessionManager,
    },
};
use std::sync::{Arc, RwLock};

#[derive(Clone)]
pub enum McpCommand {
    ShowNotification { title: String, body: String },
    DisplayContent { content: String, format: String },
    RequestUserInput { request_id: String, prompt: String, options: Vec<String> },
    SetStatusMessage { message: String },
}

#[derive(Clone)]
pub enum McpResponse {
    Success(String),
    UserInput(String),
}

pub type WatchtowerCommandQueue = Arc<RwLock<Vec<McpCommand>>>;
pub type WatchtowerResponseQueue = Arc<RwLock<Option<McpResponse>>>;

pub fn create_watchtower_mcp_queues() -> (WatchtowerCommandQueue, WatchtowerResponseQueue) {
    (
        Arc::new(RwLock::new(Vec::new())),
        Arc::new(RwLock::new(None)),
    )
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ShowNotificationRequest {
    #[schemars(description = "Title of the notification")]
    pub title: String,
    #[schemars(description = "Body text of the notification")]
    pub body: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct DisplayContentRequest {
    #[schemars(description = "Content to display in the UI")]
    pub content: String,
    #[schemars(description = "Format of the content: markdown, code, or text")]
    pub format: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct RequestUserInputRequest {
    #[schemars(description = "Prompt to show the user")]
    pub prompt: String,
    #[schemars(description = "Options for the user to choose from")]
    pub options: Option<Vec<String>>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SetStatusMessageRequest {
    #[schemars(description = "Status message to display in the toolbar")]
    pub message: String,
}

#[derive(Clone)]
pub struct WatchtowerMcpServer {
    tool_router: ToolRouter<Self>,
    command_queue: WatchtowerCommandQueue,
    response_queue: WatchtowerResponseQueue,
}

#[tool_router]
impl WatchtowerMcpServer {
    pub fn new(command_queue: WatchtowerCommandQueue, response_queue: WatchtowerResponseQueue) -> Self {
        Self {
            tool_router: Self::tool_router(),
            command_queue,
            response_queue,
        }
    }

    fn send_command_and_wait(&self, cmd: McpCommand) -> String {
        {
            let mut queue = self.command_queue.write().unwrap();
            queue.push(cmd);
        }

        for _ in 0..100 {
            std::thread::sleep(std::time::Duration::from_millis(50));
            let mut response = self.response_queue.write().unwrap();
            if let Some(resp) = response.take() {
                return match resp {
                    McpResponse::Success(message) => message,
                    McpResponse::UserInput(input) => input,
                };
            }
        }

        "Timeout waiting for response".to_string()
    }

    #[tool(description = "Show a notification in the Watchtower UI")]
    async fn show_notification(&self, Parameters(request): Parameters<ShowNotificationRequest>) -> String {
        self.send_command_and_wait(McpCommand::ShowNotification {
            title: request.title,
            body: request.body,
        })
    }

    #[tool(description = "Display content (markdown, code, or text) in the Watchtower UI")]
    async fn display_content(&self, Parameters(request): Parameters<DisplayContentRequest>) -> String {
        self.send_command_and_wait(McpCommand::DisplayContent {
            content: request.content,
            format: request.format,
        })
    }

    #[tool(description = "Request input from the user via the Watchtower UI. Blocks until the user responds.")]
    async fn request_user_input(&self, Parameters(request): Parameters<RequestUserInputRequest>) -> String {
        let request_id = format!("req_{}", std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis());

        self.send_command_and_wait(McpCommand::RequestUserInput {
            request_id,
            prompt: request.prompt,
            options: request.options.unwrap_or_default(),
        })
    }

    #[tool(description = "Set the status message displayed in the Watchtower toolbar")]
    async fn set_status_message(&self, Parameters(request): Parameters<SetStatusMessageRequest>) -> String {
        self.send_command_and_wait(McpCommand::SetStatusMessage {
            message: request.message,
        })
    }
}

#[tool_handler]
impl ServerHandler for WatchtowerMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "Watchtower MCP Server - Command the Watchtower desktop UI for Claude Code".into(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

pub fn start_watchtower_mcp_server(
    command_queue: WatchtowerCommandQueue,
    response_queue: WatchtowerResponseQueue,
) {
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let command_queue_clone = command_queue.clone();
            let response_queue_clone = response_queue.clone();

            let service = StreamableHttpService::new(
                move || Ok(WatchtowerMcpServer::new(command_queue_clone.clone(), response_queue_clone.clone())),
                LocalSessionManager::default().into(),
                Default::default(),
            );

            let router = axum::Router::new().nest_service("/mcp", service);
            let tcp_listener = tokio::net::TcpListener::bind("127.0.0.1:3334").await.unwrap();

            eprintln!("Watchtower MCP server listening on http://127.0.0.1:3334/mcp");
            eprintln!("Add to Claude Code: claude mcp add --transport http watchtower http://127.0.0.1:3334/mcp");

            axum::serve(tcp_listener, router)
                .with_graceful_shutdown(async {
                    tokio::signal::ctrl_c().await.ok();
                })
                .await
                .ok();
        });
    });
}
