use leptos::prelude::*;
use watchtower_protocol::AgentStatus;

#[derive(Clone, Copy, PartialEq)]
pub enum ActiveTab {
    Chat,
    Test,
}

#[derive(Clone, PartialEq)]
pub enum TestStatus {
    Pending,
    Running,
    Passed,
    Failed,
}

#[derive(Clone)]
pub struct TestEntry {
    pub test_name: String,
    pub status: TestStatus,
    pub message: String,
    pub duration_ms: u64,
}

#[derive(Clone)]
pub struct ToolUseBlock {
    pub tool_name: String,
    pub tool_id: String,
    pub input_json: String,
    pub finished: bool,
}

#[derive(Clone)]
pub enum MessageRole {
    User,
    Assistant,
}

#[derive(Clone)]
pub struct ChatMessage {
    pub role: MessageRole,
    pub content: String,
    pub tool_uses: Vec<ToolUseBlock>,
}

#[derive(Clone)]
pub enum StatusDisplay {
    Disconnected,
    Idle,
    Thinking,
    Streaming,
    UsingTool { tool_name: String },
}

impl StatusDisplay {
    pub fn from_agent_status(status: &AgentStatus) -> Self {
        match status {
            AgentStatus::Idle => StatusDisplay::Idle,
            AgentStatus::Thinking => StatusDisplay::Thinking,
            AgentStatus::Streaming => StatusDisplay::Streaming,
            AgentStatus::UsingTool { tool_name } => StatusDisplay::UsingTool {
                tool_name: tool_name.clone(),
            },
        }
    }

    pub fn label(&self) -> &str {
        match self {
            StatusDisplay::Disconnected => "Disconnected",
            StatusDisplay::Idle => "Ready",
            StatusDisplay::Thinking => "Thinking...",
            StatusDisplay::Streaming => "Streaming...",
            StatusDisplay::UsingTool { .. } => "Using tool...",
        }
    }

    pub fn dot_color_class(&self) -> &str {
        match self {
            StatusDisplay::Disconnected => "bg-red-500",
            StatusDisplay::Idle => "bg-green-500",
            StatusDisplay::Thinking => "bg-yellow-500",
            StatusDisplay::Streaming => "bg-blue-500",
            StatusDisplay::UsingTool { .. } => "bg-purple-500",
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    pub connected: RwSignal<bool>,
    pub status: RwSignal<StatusDisplay>,
    pub messages: RwSignal<Vec<ChatMessage>>,
    pub streaming_text: RwSignal<String>,
    pub current_session_id: RwSignal<Option<String>>,
    pub active_tools: RwSignal<Vec<ToolUseBlock>>,
    pub notifications: RwSignal<Vec<(String, String)>>,
    pub pending_input_request: RwSignal<Option<InputRequest>>,
    pub working_directory: RwSignal<String>,
    pub active_tab: RwSignal<ActiveTab>,
    pub test_results: RwSignal<Vec<TestEntry>>,
}

#[derive(Clone)]
pub struct InputRequest {
    pub request_id: String,
    pub prompt: String,
    pub options: Vec<String>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            connected: RwSignal::new(false),
            status: RwSignal::new(StatusDisplay::Disconnected),
            messages: RwSignal::new(Vec::new()),
            streaming_text: RwSignal::new(String::new()),
            current_session_id: RwSignal::new(None),
            active_tools: RwSignal::new(Vec::new()),
            notifications: RwSignal::new(Vec::new()),
            pending_input_request: RwSignal::new(None),
            working_directory: RwSignal::new(String::new()),
            active_tab: RwSignal::new(ActiveTab::Chat),
            test_results: RwSignal::new(Vec::new()),
        }
    }

    pub fn finalize_streaming_message(&self) {
        let text = self.streaming_text.get_untracked();
        let tools = self.active_tools.get_untracked();

        if !text.is_empty() || !tools.is_empty() {
            self.messages.update(|messages| {
                messages.push(ChatMessage {
                    role: MessageRole::Assistant,
                    content: text,
                    tool_uses: tools,
                });
            });
        }

        self.streaming_text.set(String::new());
        self.active_tools.set(Vec::new());
    }
}
