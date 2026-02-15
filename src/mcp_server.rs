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
    Open3dWindow { width: u32, height: u32 },
    Close3dWindow,
    SpawnEntity { name: String, shape: String, position: [f32; 3], scale: [f32; 3] },
    RemoveEntity { name: String },
    MoveEntity { name: String, position: [f32; 3] },
    RotateEntity { name: String, rotation: [f32; 3] },
    ScaleEntity { name: String, scale: [f32; 3] },
    SetCamera { focus: [f32; 3], radius: f32, yaw: f32, pitch: f32 },
    ListEntities,
    ClearScene,
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

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct Open3dWindowRequest {
    #[schemars(description = "Width of the 3D window in pixels (default: 800)")]
    pub width: Option<u32>,
    #[schemars(description = "Height of the 3D window in pixels (default: 600)")]
    pub height: Option<u32>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SpawnEntityRequest {
    #[schemars(description = "Unique name for the entity")]
    pub name: String,
    #[schemars(description = "Shape primitive: cube, sphere, cylinder, cone, torus, or plane")]
    pub shape: String,
    #[schemars(description = "Position as [x, y, z]")]
    pub position: [f32; 3],
    #[schemars(description = "Scale as [x, y, z] (default: [1, 1, 1])")]
    pub scale: Option<[f32; 3]>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct RemoveEntityRequest {
    #[schemars(description = "Name of the entity to remove")]
    pub name: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct MoveEntityRequest {
    #[schemars(description = "Name of the entity to move")]
    pub name: String,
    #[schemars(description = "New position as [x, y, z]")]
    pub position: [f32; 3],
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct RotateEntityRequest {
    #[schemars(description = "Name of the entity to rotate")]
    pub name: String,
    #[schemars(description = "Rotation in degrees as [x, y, z] euler angles")]
    pub rotation: [f32; 3],
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ScaleEntityRequest {
    #[schemars(description = "Name of the entity to scale")]
    pub name: String,
    #[schemars(description = "New scale as [x, y, z]")]
    pub scale: [f32; 3],
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SetCameraRequest {
    #[schemars(description = "Focus point as [x, y, z]")]
    pub focus: [f32; 3],
    #[schemars(description = "Distance from focus point")]
    pub radius: f32,
    #[schemars(description = "Yaw angle in degrees")]
    pub yaw: f32,
    #[schemars(description = "Pitch angle in degrees")]
    pub pitch: f32,
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

    #[tool(description = "Open a secondary 3D window with a camera and sun light. Use spawn_entity to add objects.")]
    async fn open_3d_window(&self, Parameters(request): Parameters<Open3dWindowRequest>) -> String {
        self.send_command_and_wait(McpCommand::Open3dWindow {
            width: request.width.unwrap_or(800),
            height: request.height.unwrap_or(600),
        })
    }

    #[tool(description = "Close the 3D window and clear all entities from the scene")]
    async fn close_3d_window(&self) -> String {
        self.send_command_and_wait(McpCommand::Close3dWindow)
    }

    #[tool(description = "Spawn a 3D primitive entity in the scene. Shapes: cube, sphere, cylinder, cone, torus, plane")]
    async fn spawn_entity(&self, Parameters(request): Parameters<SpawnEntityRequest>) -> String {
        self.send_command_and_wait(McpCommand::SpawnEntity {
            name: request.name,
            shape: request.shape,
            position: request.position,
            scale: request.scale.unwrap_or([1.0, 1.0, 1.0]),
        })
    }

    #[tool(description = "Remove a named entity from the 3D scene")]
    async fn remove_entity(&self, Parameters(request): Parameters<RemoveEntityRequest>) -> String {
        self.send_command_and_wait(McpCommand::RemoveEntity {
            name: request.name,
        })
    }

    #[tool(description = "Move a named entity to a new position")]
    async fn move_entity(&self, Parameters(request): Parameters<MoveEntityRequest>) -> String {
        self.send_command_and_wait(McpCommand::MoveEntity {
            name: request.name,
            position: request.position,
        })
    }

    #[tool(description = "Set the rotation of a named entity using euler angles in degrees")]
    async fn rotate_entity(&self, Parameters(request): Parameters<RotateEntityRequest>) -> String {
        self.send_command_and_wait(McpCommand::RotateEntity {
            name: request.name,
            rotation: request.rotation,
        })
    }

    #[tool(description = "Set the scale of a named entity")]
    async fn scale_entity(&self, Parameters(request): Parameters<ScaleEntityRequest>) -> String {
        self.send_command_and_wait(McpCommand::ScaleEntity {
            name: request.name,
            scale: request.scale,
        })
    }

    #[tool(description = "Set the camera position by specifying focus point, distance (radius), yaw and pitch in degrees")]
    async fn set_camera(&self, Parameters(request): Parameters<SetCameraRequest>) -> String {
        self.send_command_and_wait(McpCommand::SetCamera {
            focus: request.focus,
            radius: request.radius,
            yaw: request.yaw,
            pitch: request.pitch,
        })
    }

    #[tool(description = "List all named entities in the 3D scene with their positions")]
    async fn list_entities(&self) -> String {
        self.send_command_and_wait(McpCommand::ListEntities)
    }

    #[tool(description = "Remove all spawned entities from the scene (keeps camera and sun)")]
    async fn clear_scene(&self) -> String {
        self.send_command_and_wait(McpCommand::ClearScene)
    }
}

#[tool_handler]
impl ServerHandler for WatchtowerMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "Watchtower MCP Server - Command the Watchtower desktop UI and 3D scene for Claude Code".into(),
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
