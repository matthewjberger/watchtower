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
    CreateGame { definition: String },
    UpdateEntityScript { entity_name: String, script: String },
    AddGameEntity { entity_json: String },
    RemoveGameEntity { name: String },
    SetGameState { key: String, value: f64 },
    GetGameState,
    GetSceneInfo,
    ResetGame,
    Undo,
    Redo,
    GetHistory,
    ExportScene { path: String },
}

#[derive(Clone)]
pub enum McpResponse {
    Success(String),
    UserInput(String),
}

pub type SummonerCommandQueue = Arc<RwLock<Vec<McpCommand>>>;
pub type SummonerResponseQueue = Arc<RwLock<Option<McpResponse>>>;

pub fn create_summoner_mcp_queues() -> (SummonerCommandQueue, SummonerResponseQueue) {
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

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct CreateGameRequest {
    #[schemars(description = "Complete JSON game definition. Schema:
{
  \"title\": \"Game Title\",
  \"atmosphere\": \"Nebula\",
  \"camera\": { \"position\": [x,y,z], \"fov\": 1.2 },
  \"sun\": { \"direction\": [x,y,z], \"intensity\": 3.0 },
  \"initial_state\": { \"score\": 0.0, \"lives\": 3.0 },
  \"entities\": [
    {
      \"name\": \"EntityName\",
      \"mesh\": \"Cube|Sphere|Cylinder|Cone|Torus|Plane\",
      \"position\": [x,y,z],
      \"scale\": [x,y,z],
      \"color\": [r,g,b,a],
      \"roughness\": 0.3,
      \"metallic\": 0.0,
      \"emissive\": [r,g,b],
      \"script\": \"rhai_script_source\",
      \"grid\": { \"count\": [cols, rows], \"spacing\": [x_spacing, y_spacing] }
    }
  ]
}

GRID SYSTEM: Add \"grid\" to any entity to create a cols*rows grid of duplicates centered on position. Each gets a unique name (EntityName_0, EntityName_1, ...) and inherits all properties. USE THIS for bricks, tiles, walls, enemy formations instead of listing each entity.

RHAI SCRIPTING REFERENCE - Variables available in every script each frame:

Transform (read/write):
  pos_x, pos_y, pos_z - entity position
  rot_x, rot_y, rot_z - euler rotation in radians
  scale_x, scale_y, scale_z - entity scale

Timing:
  dt / delta_time - seconds since last frame
  time - total accumulated seconds

Input:
  pressed_keys - array of currently held key names (\"A\"-\"Z\", \"0\"-\"9\", \"SPACE\", \"ENTER\", \"ESCAPE\", \"SHIFT\", \"CTRL\", \"ALT\", \"TAB\", \"BACKSPACE\", \"UP\", \"DOWN\", \"LEFT\", \"RIGHT\")
  just_pressed_keys - keys pressed THIS frame only (not held)
  mouse_x, mouse_y - mouse position in screen coords

Entity data:
  entities - map of name -> {x, y, z, scale_x, scale_y, scale_z} for ALL entities
  entity_names - array of all entity names

Game state (shared across ALL scripts):
  state - map of string->f64 (e.g. state[\"score\"] += 10.0)

Spawning:
  do_spawn_cube = true; spawn_cube_x = 0.0; spawn_cube_y = 5.0; spawn_cube_z = 0.0;
  do_spawn_sphere = true; spawn_sphere_x = 0.0; spawn_sphere_y = 5.0; spawn_sphere_z = 0.0;

Despawning:
  do_despawn = true; (despawn THIS entity)
  despawn_names.push(\"EntityName\"); (despawn another entity by name)

Atmosphere options: None, Sky, CloudySky, Space, Nebula, Sunset, DayNight

PATTERNS:
- Movement: if pressed_keys.contains(\"A\") { pos_x -= speed * dt; }
- Bounce: if pos_y < -5.0 { state[\"vel_y\"] = state[\"vel_y\"].abs(); }
- Collision: for name in entity_names { let other = entities[name]; let dx = pos_x - other.x; ... }
- Spawn: if just_pressed_keys.contains(\"SPACE\") { do_spawn_sphere = true; spawn_sphere_x = pos_x; ... }
- Score: state[\"score\"] += 10.0;
")]
    pub definition: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct UpdateEntityScriptRequest {
    #[schemars(description = "Name of the entity whose script to update")]
    pub entity_name: String,
    #[schemars(description = "New Rhai script source code. See create_game for the full scripting API reference.")]
    pub script: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct AddGameEntityRequest {
    #[schemars(description = "JSON entity definition: { \"name\": \"Name\", \"mesh\": \"Cube\", \"position\": [x,y,z], \"scale\": [x,y,z], \"color\": [r,g,b,a], \"roughness\": 0.3, \"script\": \"...\" }")]
    pub entity_json: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct RemoveGameEntityRequest {
    #[schemars(description = "Name of the game entity to remove")]
    pub name: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SetGameStateRequest {
    #[schemars(description = "State variable key")]
    pub key: String,
    #[schemars(description = "State variable value (f64)")]
    pub value: f64,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ExportSceneRequest {
    #[schemars(description = "File path to export the scene JSON to (e.g. 'my_game.scene.json'). The exported file can be opened in the Nightshade editor.")]
    pub path: String,
}

#[derive(Clone)]
pub struct SummonerMcpServer {
    tool_router: ToolRouter<Self>,
    command_queue: SummonerCommandQueue,
    response_queue: SummonerResponseQueue,
}

#[tool_router]
impl SummonerMcpServer {
    pub fn new(command_queue: SummonerCommandQueue, response_queue: SummonerResponseQueue) -> Self {
        Self {
            tool_router: Self::tool_router(),
            command_queue,
            response_queue,
        }
    }

    async fn send_command_and_wait(&self, cmd: McpCommand) -> String {
        {
            let mut queue = self.command_queue.write().unwrap();
            queue.push(cmd);
        }

        for _ in 0..200 {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
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

    #[tool(description = "Show a notification in the Summoner UI")]
    async fn show_notification(&self, Parameters(request): Parameters<ShowNotificationRequest>) -> String {
        self.send_command_and_wait(McpCommand::ShowNotification {
            title: request.title,
            body: request.body,
        }).await
    }

    #[tool(description = "Display content (markdown, code, or text) in the Summoner UI")]
    async fn display_content(&self, Parameters(request): Parameters<DisplayContentRequest>) -> String {
        self.send_command_and_wait(McpCommand::DisplayContent {
            content: request.content,
            format: request.format,
        }).await
    }

    #[tool(description = "Request input from the user via the Summoner UI. Blocks until the user responds.")]
    async fn request_user_input(&self, Parameters(request): Parameters<RequestUserInputRequest>) -> String {
        let request_id = format!("req_{}", std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis());

        self.send_command_and_wait(McpCommand::RequestUserInput {
            request_id,
            prompt: request.prompt,
            options: request.options.unwrap_or_default(),
        }).await
    }

    #[tool(description = "Set the status message displayed in the Summoner toolbar")]
    async fn set_status_message(&self, Parameters(request): Parameters<SetStatusMessageRequest>) -> String {
        self.send_command_and_wait(McpCommand::SetStatusMessage {
            message: request.message,
        }).await
    }

    #[tool(description = "Open a secondary 3D window with a camera and sun light. Use spawn_entity to add objects.")]
    async fn open_3d_window(&self, Parameters(request): Parameters<Open3dWindowRequest>) -> String {
        self.send_command_and_wait(McpCommand::Open3dWindow {
            width: request.width.unwrap_or(800),
            height: request.height.unwrap_or(600),
        }).await
    }

    #[tool(description = "Close the 3D window and clear all entities from the scene")]
    async fn close_3d_window(&self) -> String {
        self.send_command_and_wait(McpCommand::Close3dWindow).await
    }

    #[tool(description = "Spawn a 3D primitive entity in the scene. Shapes: cube, sphere, cylinder, cone, torus, plane")]
    async fn spawn_entity(&self, Parameters(request): Parameters<SpawnEntityRequest>) -> String {
        self.send_command_and_wait(McpCommand::SpawnEntity {
            name: request.name,
            shape: request.shape,
            position: request.position,
            scale: request.scale.unwrap_or([1.0, 1.0, 1.0]),
        }).await
    }

    #[tool(description = "Remove a named entity from the 3D scene")]
    async fn remove_entity(&self, Parameters(request): Parameters<RemoveEntityRequest>) -> String {
        self.send_command_and_wait(McpCommand::RemoveEntity {
            name: request.name,
        }).await
    }

    #[tool(description = "Move a named entity to a new position")]
    async fn move_entity(&self, Parameters(request): Parameters<MoveEntityRequest>) -> String {
        self.send_command_and_wait(McpCommand::MoveEntity {
            name: request.name,
            position: request.position,
        }).await
    }

    #[tool(description = "Set the rotation of a named entity using euler angles in degrees")]
    async fn rotate_entity(&self, Parameters(request): Parameters<RotateEntityRequest>) -> String {
        self.send_command_and_wait(McpCommand::RotateEntity {
            name: request.name,
            rotation: request.rotation,
        }).await
    }

    #[tool(description = "Set the scale of a named entity")]
    async fn scale_entity(&self, Parameters(request): Parameters<ScaleEntityRequest>) -> String {
        self.send_command_and_wait(McpCommand::ScaleEntity {
            name: request.name,
            scale: request.scale,
        }).await
    }

    #[tool(description = "Set the camera position by specifying focus point, distance (radius), yaw and pitch in degrees")]
    async fn set_camera(&self, Parameters(request): Parameters<SetCameraRequest>) -> String {
        self.send_command_and_wait(McpCommand::SetCamera {
            focus: request.focus,
            radius: request.radius,
            yaw: request.yaw,
            pitch: request.pitch,
        }).await
    }

    #[tool(description = "List all named entities in the 3D scene with their positions")]
    async fn list_entities(&self) -> String {
        self.send_command_and_wait(McpCommand::ListEntities).await
    }

    #[tool(description = "Remove all spawned entities from the scene (keeps camera and sun)")]
    async fn clear_scene(&self) -> String {
        self.send_command_and_wait(McpCommand::ClearScene).await
    }

    #[tool(description = "Create a complete playable game from a JSON definition. Opens a 3D window and spawns all entities with scripts. See the 'definition' parameter for the full JSON schema and Rhai scripting API.")]
    async fn create_game(&self, Parameters(request): Parameters<CreateGameRequest>) -> String {
        self.send_command_and_wait(McpCommand::CreateGame {
            definition: request.definition,
        }).await
    }

    #[tool(description = "Update the Rhai script on a named game entity. The entity keeps its mesh, position, and material - only the script changes. See create_game for the Rhai API reference.")]
    async fn update_entity_script(&self, Parameters(request): Parameters<UpdateEntityScriptRequest>) -> String {
        self.send_command_and_wait(McpCommand::UpdateEntityScript {
            entity_name: request.entity_name,
            script: request.script,
        }).await
    }

    #[tool(description = "Add a new entity to the running game. Uses the same entity JSON format as create_game's entities array.")]
    async fn add_game_entity(&self, Parameters(request): Parameters<AddGameEntityRequest>) -> String {
        self.send_command_and_wait(McpCommand::AddGameEntity {
            entity_json: request.entity_json,
        }).await
    }

    #[tool(description = "Remove a named entity from the running game")]
    async fn remove_game_entity(&self, Parameters(request): Parameters<RemoveGameEntityRequest>) -> String {
        self.send_command_and_wait(McpCommand::RemoveGameEntity {
            name: request.name,
        }).await
    }

    #[tool(description = "Set a game state variable (shared across all entity scripts via state[\"key\"])")]
    async fn set_game_state(&self, Parameters(request): Parameters<SetGameStateRequest>) -> String {
        self.send_command_and_wait(McpCommand::SetGameState {
            key: request.key,
            value: request.value,
        }).await
    }

    #[tool(description = "Get all game state variables as JSON")]
    async fn get_game_state(&self) -> String {
        self.send_command_and_wait(McpCommand::GetGameState).await
    }

    #[tool(description = "Get full scene info: all entity names, positions, scales, scripts, and game state")]
    async fn get_scene_info(&self) -> String {
        self.send_command_and_wait(McpCommand::GetSceneInfo).await
    }

    #[tool(description = "Reset the game to its original state (re-spawn from the stored definition)")]
    async fn reset_game(&self) -> String {
        self.send_command_and_wait(McpCommand::ResetGame).await
    }

    #[tool(description = "Undo the last game operation (entity add/remove, script update, state change). Returns what was undone.")]
    async fn undo(&self) -> String {
        self.send_command_and_wait(McpCommand::Undo).await
    }

    #[tool(description = "Redo the last undone operation. Returns what was redone.")]
    async fn redo(&self) -> String {
        self.send_command_and_wait(McpCommand::Redo).await
    }

    #[tool(description = "Get the operation history tree showing all operations with their IDs, timestamps, and descriptions")]
    async fn get_history(&self) -> String {
        self.send_command_and_wait(McpCommand::GetHistory).await
    }

    #[tool(description = "Export the current game scene as a Nightshade .scene.json file that can be opened in the Nightshade editor")]
    async fn export_scene(&self, Parameters(request): Parameters<ExportSceneRequest>) -> String {
        self.send_command_and_wait(McpCommand::ExportScene {
            path: request.path,
        }).await
    }
}

#[tool_handler]
impl ServerHandler for SummonerMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "Summoner MCP Server - AI game creation platform. Create playable games from descriptions, edit them live, and export to Nightshade engine format.".into(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

pub fn start_summoner_mcp_server(
    command_queue: SummonerCommandQueue,
    response_queue: SummonerResponseQueue,
) {
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let command_queue_clone = command_queue.clone();
            let response_queue_clone = response_queue.clone();

            let service = StreamableHttpService::new(
                move || Ok(SummonerMcpServer::new(command_queue_clone.clone(), response_queue_clone.clone())),
                LocalSessionManager::default().into(),
                Default::default(),
            );

            let router = axum::Router::new().nest_service("/mcp", service);
            let tcp_listener = tokio::net::TcpListener::bind("127.0.0.1:3334").await.unwrap();

            eprintln!("Summoner MCP server listening on http://127.0.0.1:3334/mcp");
            eprintln!("Add to Claude Code: claude mcp add --transport http summoner http://127.0.0.1:3334/mcp");

            axum::serve(tcp_listener, router)
                .with_graceful_shutdown(async {
                    tokio::signal::ctrl_c().await.ok();
                })
                .await
                .ok();
        });
    });
}
