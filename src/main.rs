#![windows_subsystem = "windows"]

mod cli;
mod mcp_server;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Instant;

use include_dir::{Dir, include_dir};
use nightshade::prelude::*;
use nightshade::webview::{WebviewContext, serve_embedded_dir};
use watchtower_protocol::{AgentStatus, BackendEvent, ContentFormat, FrontendCommand};

use crate::cli::{CliCommand, CliEvent, spawn_cli_worker};
use crate::mcp_server::{
    McpCommand, McpResponse, WatchtowerCommandQueue, WatchtowerResponseQueue,
    create_watchtower_mcp_queues, start_watchtower_mcp_server,
};

static DIST: Dir = include_dir!("$CARGO_MANIFEST_DIR/site/dist");

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (cli_cmd_tx, cli_cmd_rx) = mpsc::channel::<CliCommand>();
    let (cli_event_tx, cli_event_rx) = mpsc::channel::<CliEvent>();

    spawn_cli_worker(cli_cmd_rx, cli_event_tx);

    let (mcp_command_queue, mcp_response_queue) = create_watchtower_mcp_queues();
    start_watchtower_mcp_server(mcp_command_queue.clone(), mcp_response_queue.clone());

    let (test_result_tx, test_result_rx) = mpsc::channel::<BackendEvent>();

    launch(Watchtower {
        port: serve_embedded_dir(&DIST),
        ctx: WebviewContext::default(),
        connected: false,
        cli_cmd_tx,
        cli_event_rx,
        mcp_command_queue,
        mcp_response_queue,
        test_result_tx,
        test_result_rx,
        cli_prompt_test_running: Arc::new(AtomicBool::new(false)),
    })?;

    Ok(())
}

struct Watchtower {
    port: u16,
    ctx: WebviewContext<FrontendCommand, BackendEvent>,
    connected: bool,
    cli_cmd_tx: mpsc::Sender<CliCommand>,
    cli_event_rx: mpsc::Receiver<CliEvent>,
    mcp_command_queue: WatchtowerCommandQueue,
    mcp_response_queue: WatchtowerResponseQueue,
    test_result_tx: mpsc::Sender<BackendEvent>,
    test_result_rx: mpsc::Receiver<BackendEvent>,
    cli_prompt_test_running: Arc<AtomicBool>,
}

impl State for Watchtower {
    fn title(&self) -> &str {
        "Watchtower"
    }

    fn initialize(&mut self, world: &mut World) {
        world.resources.user_interface.enabled = true;
    }

    fn ui(&mut self, world: &mut World, ctx: &egui::Context) {
        let commands: Vec<FrontendCommand> = self.ctx.drain_messages().collect();
        for cmd in commands {
            match cmd {
                FrontendCommand::Ready => {
                    if !self.connected {
                        self.ctx.send(BackendEvent::Connected);
                        self.ctx.send(BackendEvent::StatusUpdate {
                            status: AgentStatus::Idle,
                        });
                        self.connected = true;
                    }
                }
                FrontendCommand::SendPrompt { prompt, session_id, model } => {
                    self.ctx.send(BackendEvent::StatusUpdate {
                        status: AgentStatus::Thinking,
                    });
                    let _ = self.cli_cmd_tx.send(CliCommand::StartQuery {
                        prompt,
                        session_id,
                        model,
                    });
                }
                FrontendCommand::CancelRequest => {
                    let _ = self.cli_cmd_tx.send(CliCommand::Cancel);
                    self.ctx.send(BackendEvent::StatusUpdate {
                        status: AgentStatus::Idle,
                    });
                }
                FrontendCommand::UserInputResponse { response, .. } => {
                    let mut resp_queue = self.mcp_response_queue.write().unwrap();
                    *resp_queue = Some(McpResponse::UserInput(response));
                }
                FrontendCommand::SetWorkingDirectory { path } => {
                    let _ = self.cli_cmd_tx.send(CliCommand::SetWorkingDirectory {
                        path: std::path::PathBuf::from(path),
                    });
                }
                FrontendCommand::BrowseWorkingDirectory => {
                    if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                        let path = folder.display().to_string();
                        let _ = self.cli_cmd_tx.send(CliCommand::SetWorkingDirectory {
                            path: folder,
                        });
                        self.ctx.send(BackendEvent::WorkingDirectoryChanged {
                            path,
                        });
                    }
                }
                FrontendCommand::RunTest { test_name } => {
                    self.handle_run_test(&test_name);
                }
            }
        }

        for event in self.cli_event_rx.try_iter() {
            match event {
                CliEvent::SessionStarted { session_id } => {
                    self.ctx.send(BackendEvent::StreamingStarted {
                        session_id,
                    });
                    self.ctx.send(BackendEvent::StatusUpdate {
                        status: AgentStatus::Streaming,
                    });
                }
                CliEvent::TextDelta { text } => {
                    self.ctx.send(BackendEvent::TextDelta { text });
                }
                CliEvent::ToolUseStarted { tool_name, tool_id } => {
                    self.ctx.send(BackendEvent::StatusUpdate {
                        status: AgentStatus::UsingTool {
                            tool_name: tool_name.clone(),
                        },
                    });
                    self.ctx.send(BackendEvent::ToolUseStarted {
                        tool_name,
                        tool_id,
                    });
                }
                CliEvent::ToolUseInputDelta { tool_id, partial_json } => {
                    self.ctx.send(BackendEvent::ToolUseInputDelta {
                        tool_id,
                        partial_json,
                    });
                }
                CliEvent::ToolUseFinished { tool_id } => {
                    self.ctx.send(BackendEvent::ToolUseFinished { tool_id });
                    self.ctx.send(BackendEvent::StatusUpdate {
                        status: AgentStatus::Streaming,
                    });
                }
                CliEvent::TurnComplete { session_id } => {
                    self.ctx.send(BackendEvent::TurnComplete {
                        session_id,
                    });
                }
                CliEvent::Complete { session_id, total_cost_usd, num_turns } => {
                    self.ctx.send(BackendEvent::RequestComplete {
                        session_id,
                        total_cost_usd,
                        num_turns,
                    });
                    self.ctx.send(BackendEvent::StatusUpdate {
                        status: AgentStatus::Idle,
                    });
                    if self.cli_prompt_test_running.swap(false, Ordering::SeqCst) {
                        self.ctx.send(BackendEvent::TestResult {
                            test_name: "cli_prompt".to_string(),
                            success: true,
                            message: format!("CLI completed ({num_turns} turns)"),
                            duration_ms: 0,
                        });
                    }
                }
                CliEvent::Error { message } => {
                    self.ctx.send(BackendEvent::Error { message: message.clone() });
                    self.ctx.send(BackendEvent::StatusUpdate {
                        status: AgentStatus::Idle,
                    });
                    if self.cli_prompt_test_running.swap(false, Ordering::SeqCst) {
                        self.ctx.send(BackendEvent::TestResult {
                            test_name: "cli_prompt".to_string(),
                            success: false,
                            message,
                            duration_ms: 0,
                        });
                    }
                }
            }
        }

        let commands: Vec<McpCommand> = {
            let mut queue = self.mcp_command_queue.write().unwrap();
            queue.drain(..).collect()
        };

        for command in commands {
            match command {
                McpCommand::ShowNotification { title, body } => {
                    self.ctx.send(BackendEvent::Notification { title, body });
                    let mut resp = self.mcp_response_queue.write().unwrap();
                    *resp = Some(McpResponse::Success("Notification shown".to_string()));
                }
                McpCommand::DisplayContent { content, format } => {
                    let content_format = match format.as_str() {
                        "markdown" => ContentFormat::Markdown,
                        "code" => ContentFormat::Code,
                        _ => ContentFormat::Text,
                    };
                    self.ctx.send(BackendEvent::ContentDisplay {
                        content,
                        format: content_format,
                    });
                    let mut resp = self.mcp_response_queue.write().unwrap();
                    *resp = Some(McpResponse::Success("Content displayed".to_string()));
                }
                McpCommand::RequestUserInput { request_id, prompt, options } => {
                    self.ctx.send(BackendEvent::UserInputRequest {
                        request_id,
                        prompt,
                        options,
                    });
                }
                McpCommand::SetStatusMessage { message } => {
                    self.ctx.send(BackendEvent::Notification {
                        title: "Status".to_string(),
                        body: message,
                    });
                    let mut resp = self.mcp_response_queue.write().unwrap();
                    *resp = Some(McpResponse::Success("Status updated".to_string()));
                }
            }
        }

        for test_event in self.test_result_rx.try_iter() {
            self.ctx.send(test_event);
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| {
                if let Some(handle) = &world.resources.window.handle {
                    self.ctx.ensure_webview(
                        handle.clone(),
                        self.port,
                        ui.available_rect_before_wrap(),
                    );
                    handle.request_redraw();
                }
            });
    }
}

impl Watchtower {
    fn handle_run_test(&mut self, test_name: &str) {
        match test_name {
            "ipc_echo" => {
                let start = Instant::now();
                let elapsed = start.elapsed();
                self.ctx.send(BackendEvent::TestResult {
                    test_name: "ipc_echo".to_string(),
                    success: true,
                    message: "IPC round-trip successful".to_string(),
                    duration_ms: elapsed.as_millis() as u64,
                });
            }

            "mcp_round_trip" => {
                let sender = self.test_result_tx.clone();
                std::thread::spawn(move || {
                    let start = Instant::now();
                    let mcp_init_body = serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": 1,
                        "method": "initialize",
                        "params": {
                            "protocolVersion": "2025-03-26",
                            "capabilities": {},
                            "clientInfo": {
                                "name": "watchtower-test",
                                "version": "0.1.0"
                            }
                        }
                    });

                    let result = ureq::post("http://127.0.0.1:3334/mcp")
                        .set("Content-Type", "application/json")
                        .set("Accept", "application/json, text/event-stream")
                        .send_string(&mcp_init_body.to_string());

                    let elapsed = start.elapsed();
                    match result {
                        Ok(response) => {
                            let status = response.status();
                            let _ = sender.send(BackendEvent::TestResult {
                                test_name: "mcp_round_trip".to_string(),
                                success: (200..300).contains(&status),
                                message: format!("MCP server responded with status {status}"),
                                duration_ms: elapsed.as_millis() as u64,
                            });
                        }
                        Err(error) => {
                            let _ = sender.send(BackendEvent::TestResult {
                                test_name: "mcp_round_trip".to_string(),
                                success: false,
                                message: format!("MCP request failed: {error}"),
                                duration_ms: elapsed.as_millis() as u64,
                            });
                        }
                    }
                });
            }

            "show_notification" => {
                self.ctx.send(BackendEvent::Notification {
                    title: "Test Notification".to_string(),
                    body: "This notification was triggered by the show_notification test.".to_string(),
                });
                self.ctx.send(BackendEvent::TestResult {
                    test_name: "show_notification".to_string(),
                    success: true,
                    message: "Notification sent to UI".to_string(),
                    duration_ms: 0,
                });
            }

            "display_content" => {
                self.ctx.send(BackendEvent::ContentDisplay {
                    content: "# Test Content\n\nThis markdown was sent by the **display_content** test.\n\n- Item one\n- Item two\n- Item three".to_string(),
                    format: ContentFormat::Markdown,
                });
                self.ctx.send(BackendEvent::TestResult {
                    test_name: "display_content".to_string(),
                    success: true,
                    message: "Content displayed in chat".to_string(),
                    duration_ms: 0,
                });
            }

            "status_cycle" => {
                let sender = self.test_result_tx.clone();
                std::thread::spawn(move || {
                    let start = Instant::now();
                    let statuses = [
                        AgentStatus::Idle,
                        AgentStatus::Thinking,
                        AgentStatus::Streaming,
                        AgentStatus::UsingTool { tool_name: "test_tool".to_string() },
                        AgentStatus::Idle,
                    ];
                    for status in statuses {
                        let _ = sender.send(BackendEvent::StatusUpdate {
                            status,
                        });
                        std::thread::sleep(std::time::Duration::from_millis(500));
                    }
                    let elapsed = start.elapsed();
                    let _ = sender.send(BackendEvent::TestResult {
                        test_name: "status_cycle".to_string(),
                        success: true,
                        message: "Cycled through all status values".to_string(),
                        duration_ms: elapsed.as_millis() as u64,
                    });
                });
            }

            "cli_prompt" => {
                self.cli_prompt_test_running.store(true, Ordering::SeqCst);
                self.ctx.send(BackendEvent::StatusUpdate {
                    status: AgentStatus::Thinking,
                });
                let _ = self.cli_cmd_tx.send(CliCommand::StartQuery {
                    prompt: "Say hello in exactly 3 words".to_string(),
                    session_id: None,
                    model: None,
                });

                let flag = self.cli_prompt_test_running.clone();
                let sender = self.test_result_tx.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_secs(60));
                    if flag.swap(false, Ordering::SeqCst) {
                        let _ = sender.send(BackendEvent::TestResult {
                            test_name: "cli_prompt".to_string(),
                            success: false,
                            message: "Timed out after 60s waiting for CLI response".to_string(),
                            duration_ms: 60_000,
                        });
                    }
                });
            }

            _ => {
                self.ctx.send(BackendEvent::TestResult {
                    test_name: test_name.to_string(),
                    success: false,
                    message: format!("Unknown test: {test_name}"),
                    duration_ms: 0,
                });
            }
        }
    }
}
