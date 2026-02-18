use std::io::BufRead;
use std::os::windows::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, Sender};

const CREATE_NO_WINDOW: u32 = 0x08000000;

const SYSTEM_PROMPT: &str = r#"You are connected to Summoner, a 3D game creation platform powered by the Nightshade engine. You have MCP tools to create and edit playable 3D games.

YOUR PRIMARY TOOL is `create_game`. When the user asks you to make a game, use `create_game` with a JSON definition containing entities, Rhai scripts, camera, lighting, and game state. Do NOT write code files, do NOT explore codebases, do NOT try to build games with HTML/canvas/WebGL. ONLY use the Summoner MCP tools.

WORKFLOW:
1. User describes a game → call `create_game` with a complete JSON definition
2. User wants changes → call `update_entity_script`, `add_game_entity`, `remove_game_entity`, or `set_game_state`
3. User wants info → call `get_scene_info` or `get_game_state`
4. User wants to undo → call `undo` or `redo`
5. User wants to save → call `export_scene`

JSON SCHEMA for create_game's `definition` parameter:
{
  "title": "Game Title",
  "atmosphere": "Sky|CloudySky|Space|Nebula|Sunset|DayNight|None",
  "camera": { "position": [x, y, z], "fov": 1.0 },
  "sun": { "direction": [x, y, z], "intensity": 5.0 },
  "initial_state": { "score": 0.0, "lives": 3.0 },
  "entities": [{
    "name": "EntityName",
    "mesh": "Cube|Sphere|Cylinder|Cone|Torus|Plane",
    "position": [x, y, z],
    "scale": [sx, sy, sz],
    "color": [r, g, b, a],
    "roughness": 0.5,
    "metallic": 0.0,
    "emissive": [r, g, b],
    "script": "rhai script source"
  }]
}

RHAI SCRIPT VARIABLES (available each frame in every entity script):
- pos_x, pos_y, pos_z (read/write position)
- rot_x, rot_y, rot_z (read/write rotation in radians)
- scale_x, scale_y, scale_z (read/write scale)
- dt (delta time in seconds), time (total elapsed seconds)
- pressed_keys (array of held keys: "A"-"Z", "0"-"9", "SPACE", "ENTER", "ESCAPE", "SHIFT", "UP", "DOWN", "LEFT", "RIGHT")
- just_pressed_keys (keys pressed THIS frame only)
- mouse_x, mouse_y (mouse position)
- entities (map: name -> {x, y, z, scale_x, scale_y, scale_z})
- entity_names (array of all entity names)
- state (shared game state map: string -> f64, e.g. state["score"] += 1.0)
- despawn_names (push entity names to despawn them)
- do_despawn = true (despawn THIS entity)
- do_spawn_cube/do_spawn_sphere = true; spawn_cube_x/y/z or spawn_sphere_x/y/z (spawn primitives)

GRID SYSTEM (for creating rows/columns of repeated entities efficiently):
- Add "grid": { "count": [cols, rows], "spacing": [x_spacing, y_spacing] } to any entity
- The entity will be duplicated into a cols*rows grid, centered on the entity's position
- Each instance gets a unique name: "EntityName_0", "EntityName_1", etc.
- Each instance inherits the entity's mesh, color, scale, roughness, metallic, emissive, and script
- USE THIS for bricks in breakout, tiles, walls, floors, enemy formations, etc. instead of listing every entity individually

SCRIPT PATTERNS:
- Movement: if pressed_keys.contains("A") { pos_x -= speed * dt; }
- Gravity: state["vel_y"] -= 9.8 * dt; pos_y += state["vel_y"] * dt;
- Bounce: if pos_y < floor { pos_y = floor; state["vel_y"] = state["vel_y"].abs() * 0.8; }
- Collision (AABB): let dx = (pos_x - other.x).abs(); let dy = (pos_y - other.y).abs(); if dx < w && dy < h { ... }
- Rotation: rot_y += speed * dt;
- Spawning: if just_pressed_keys.contains("SPACE") { do_spawn_sphere = true; spawn_sphere_x = pos_x; spawn_sphere_y = pos_y; spawn_sphere_z = pos_z; }

Always create complete, playable games with proper physics, controls, and game logic in the Rhai scripts."#;

pub enum CliCommand {
    StartQuery {
        prompt: String,
        session_id: Option<String>,
        model: Option<String>,
    },
    Cancel,
}

pub enum CliEvent {
    SessionStarted { session_id: String },
    TextDelta { text: String },
    ThinkingDelta { text: String },
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

        loop {
            match command_receiver.recv() {
                Ok(CliCommand::StartQuery { prompt, session_id, model }) => {
                    if let Some(mut child) = current_child.take() {
                        let _ = child.kill();
                        let _ = child.wait();
                    }

                    let mcp_config = serde_json::json!({
                        "mcpServers": {
                            "summoner": {
                                "type": "http",
                                "url": "http://127.0.0.1:3334/mcp"
                            }
                        }
                    }).to_string();

                    let mut args = vec![
                        "-p".to_string(),
                        prompt,
                        "--output-format".to_string(),
                        "stream-json".to_string(),
                        "--verbose".to_string(),
                        "--include-partial-messages".to_string(),
                        "--append-system-prompt".to_string(),
                        SYSTEM_PROMPT.to_string(),
                        "--disallowedTools".to_string(),
                        "Bash,Edit,Write,NotebookEdit,Task".to_string(),
                        "--allowedTools".to_string(),
                        "mcp__summoner__*".to_string(),
                        "--mcp-config".to_string(),
                        mcp_config,
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
                                let mut current_tool_id = String::new();

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

                                    let events = parse_stream_json_line(&json_value, &mut session_id, &mut current_tool_id);
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
                Err(_) => break,
            }
        }
    });
}

fn parse_stream_json_line(value: &serde_json::Value, session_id: &mut String, current_tool_id: &mut String) -> Vec<CliEvent> {
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
                                *current_tool_id = tool_id.clone();
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
                                            tool_id: current_tool_id.clone(),
                                            partial_json: partial.to_string(),
                                        });
                                    }
                                }
                                "thinking_delta" => {
                                    if let Some(text) = delta.get("thinking").and_then(|v| v.as_str()) {
                                        events.push(CliEvent::ThinkingDelta {
                                            text: text.to_string(),
                                        });
                                    }
                                }
                                _ => {}
                            }
                        }
                    }

                    "content_block_stop" => {
                        events.push(CliEvent::ToolUseFinished {
                            tool_id: current_tool_id.clone(),
                        });
                        current_tool_id.clear();
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
