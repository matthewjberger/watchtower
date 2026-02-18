#![no_std]
extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum PlayState {
    Stopped,
    Playing,
    Paused,
}

#[derive(Clone, Serialize, Deserialize)]
pub enum FrontendCommand {
    Ready,
    SendPrompt {
        prompt: String,
        session_id: Option<String>,
        model: Option<String>,
    },
    CancelRequest,
    UserInputResponse {
        request_id: String,
        response: String,
    },
    RunTest {
        test_name: String,
    },
    Assemble,
    PlayGame,
    PauseGame,
    StopGame,
    OpenEditorWindow,
}

#[derive(Clone, Serialize, Deserialize)]
pub enum BackendEvent {
    Connected,
    StreamingStarted {
        session_id: String,
    },
    TextDelta {
        text: String,
    },
    ThinkingDelta {
        text: String,
    },
    ToolUseStarted {
        tool_name: String,
        tool_id: String,
    },
    ToolUseInputDelta {
        tool_id: String,
        partial_json: String,
    },
    ToolUseFinished {
        tool_id: String,
    },
    TurnComplete {
        session_id: String,
    },
    RequestComplete {
        session_id: String,
        total_cost_usd: Option<f64>,
        num_turns: u32,
    },
    Error {
        message: String,
    },
    StatusUpdate {
        status: AgentStatus,
    },
    Notification {
        title: String,
        body: String,
    },
    ContentDisplay {
        content: String,
        format: ContentFormat,
    },
    UserInputRequest {
        request_id: String,
        prompt: String,
        options: Vec<String>,
    },
    TestResult {
        test_name: String,
        success: bool,
        message: String,
        duration_ms: u64,
    },
    GameStateChanged {
        has_game: bool,
        play_state: PlayState,
        editor_window_open: bool,
    },
}

#[derive(Clone, Serialize, Deserialize)]
pub enum AgentStatus {
    Idle,
    Thinking,
    Streaming,
    UsingTool { tool_name: String },
}

#[derive(Clone, Serialize, Deserialize)]
pub enum ContentFormat {
    Markdown,
    Code,
    Text,
}
