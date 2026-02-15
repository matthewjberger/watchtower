use std::io::BufRead;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, Sender};

const CREATE_NO_WINDOW: u32 = 0x08000000;

pub enum CliCommand {
    StartQuery {
        prompt: String,
        session_id: Option<String>,
        model: Option<String>,
    },
    Cancel,
    SetWorkingDirectory {
        path: PathBuf,
    },
}

pub enum CliEvent {
    SessionStarted { session_id: String },
    TextDelta { text: String },
    ToolUseStarted { tool_name: String, tool_id: String },
    ToolUseInputDelta { tool_id: String, partial_json: String },
    ToolUseFinished { tool_id: String },
    TurnComplete { session_id: String },
    Complete { session_id: String, total_cost_usd: Option<f64>, num_turns: u32 },
    Error { message: String },
}

pub fn spawn_cli_worker(
    command_receiver: Receiver<CliCommand>,
    event_sender: Sender<CliEvent>,
) {
    std::thread::spawn(move || {
        let mut current_child: Option<Child> = None;
        let mut current_session_id = String::new();
        let mut working_directory: Option<PathBuf> = None;

        loop {
            match command_receiver.recv() {
                Ok(CliCommand::StartQuery { prompt, session_id, model }) => {
                    if let Some(mut child) = current_child.take() {
                        let _ = child.kill();
                        let _ = child.wait();
                    }

                    let mut args = vec![
                        "-p".to_string(),
                        prompt,
                        "--output-format".to_string(),
                        "stream-json".to_string(),
                        "--verbose".to_string(),
                        "--include-partial-messages".to_string(),
                    ];

                    if let Some(sid) = session_id {
                        args.push("--resume".to_string());
                        args.push(sid);
                    }

                    if let Some(model_name) = model {
                        args.push("--model".to_string());
                        args.push(model_name);
                    }

                    let mut cmd = Command::new("claude");
                    cmd.args(&args)
                        .stdout(Stdio::piped())
                        .stderr(Stdio::piped())
                        .creation_flags(CREATE_NO_WINDOW)
                        .env_remove("CLAUDECODE");

                    if let Some(cwd) = &working_directory {
                        cmd.current_dir(cwd);
                    }

                    match cmd.spawn() {
                        Ok(mut child) => {
                            let stdout = child.stdout.take().expect("stdout was piped");
                            let stderr = child.stderr.take().expect("stderr was piped");
                            current_child = Some(child);

                            std::thread::spawn(move || {
                                let reader = std::io::BufReader::new(stderr);
                                for _ in reader.lines() {}
                            });

                            let event_sender_clone = event_sender.clone();

                            std::thread::spawn(move || {
                                let reader = std::io::BufReader::new(stdout);
                                let mut session_id = String::new();

                                for line_result in reader.lines() {
                                    let line = match line_result {
                                        Ok(line) => line,
                                        Err(_) => break,
                                    };

                                    if line.trim().is_empty() {
                                        continue;
                                    }

                                    let json_value: serde_json::Value = match serde_json::from_str(&line) {
                                        Ok(value) => value,
                                        Err(_) => continue,
                                    };

                                    let events = parse_stream_json_line(&json_value, &mut session_id);
                                    for event in events {
                                        if event_sender_clone.send(event).is_err() {
                                            return;
                                        }
                                    }
                                }
                            });

                            current_session_id = String::new();
                        }
                        Err(error) => {
                            let _ = event_sender.send(CliEvent::Error {
                                message: format!("Failed to spawn claude CLI: {error}"),
                            });
                        }
                    }
                }
                Ok(CliCommand::Cancel) => {
                    if let Some(mut child) = current_child.take() {
                        let _ = child.kill();
                        let _ = child.wait();
                    }
                    let _ = event_sender.send(CliEvent::TurnComplete {
                        session_id: current_session_id.clone(),
                    });
                }
                Ok(CliCommand::SetWorkingDirectory { path }) => {
                    working_directory = Some(path);
                }
                Err(_) => break,
            }
        }
    });
}

fn parse_stream_json_line(value: &serde_json::Value, session_id: &mut String) -> Vec<CliEvent> {
    let mut events = Vec::new();

    let message_type = value.get("type").and_then(|v| v.as_str()).unwrap_or("");

    match message_type {
        "system" => {
            if let Some(sid) = value.get("session_id").and_then(|v| v.as_str()) {
                *session_id = sid.to_string();
                events.push(CliEvent::SessionStarted {
                    session_id: sid.to_string(),
                });
            }
        }

        "stream_event" => {
            if let Some(event) = value.get("event") {
                let event_type = event.get("type").and_then(|v| v.as_str()).unwrap_or("");

                match event_type {
                    "content_block_start" => {
                        if let Some(content_block) = event.get("content_block") {
                            let block_type = content_block.get("type").and_then(|v| v.as_str()).unwrap_or("");
                            if block_type == "tool_use" {
                                let tool_name = content_block.get("name")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("unknown")
                                    .to_string();
                                let tool_id = content_block.get("id")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string();
                                events.push(CliEvent::ToolUseStarted { tool_name, tool_id });
                            }
                        }
                    }

                    "content_block_delta" => {
                        if let Some(delta) = event.get("delta") {
                            let delta_type = delta.get("type").and_then(|v| v.as_str()).unwrap_or("");

                            match delta_type {
                                "text_delta" => {
                                    if let Some(text) = delta.get("text").and_then(|v| v.as_str()) {
                                        events.push(CliEvent::TextDelta {
                                            text: text.to_string(),
                                        });
                                    }
                                }
                                "input_json_delta" => {
                                    if let Some(partial) = delta.get("partial_json").and_then(|v| v.as_str()) {
                                        events.push(CliEvent::ToolUseInputDelta {
                                            tool_id: String::new(),
                                            partial_json: partial.to_string(),
                                        });
                                    }
                                }
                                _ => {}
                            }
                        }
                    }

                    "content_block_stop" => {
                        events.push(CliEvent::ToolUseFinished {
                            tool_id: String::new(),
                        });
                    }

                    "message_stop" => {
                        events.push(CliEvent::TurnComplete {
                            session_id: session_id.clone(),
                        });
                    }

                    _ => {}
                }
            }
        }

        "result" => {
            let total_cost = value.get("total_cost_usd").and_then(|v| v.as_f64());
            let num_turns = value.get("num_turns").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            events.push(CliEvent::Complete {
                session_id: session_id.clone(),
                total_cost_usd: total_cost,
                num_turns,
            });
        }

        _ => {}
    }

    events
}
