#![windows_subsystem = "windows"]

mod cli;
mod mcp_server;
mod scene;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Instant;

use include_dir::{Dir, include_dir};
use nightshade::ecs::camera::spawn_pan_orbit_camera;
use nightshade::prelude::*;
use nightshade::webview::{WebviewContext, serve_embedded_dir};
use watchtower_protocol::{AgentStatus, BackendEvent, ContentFormat, FrontendCommand};

use crate::cli::{CliCommand, CliEvent, spawn_cli_worker};
use crate::mcp_server::{
    McpCommand, McpResponse, WatchtowerCommandQueue, WatchtowerResponseQueue,
    create_watchtower_mcp_queues, start_watchtower_mcp_server,
};
use crate::scene::SceneState;

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
        scene: SceneState::default(),
        assemble_counter: 0,
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
    scene: SceneState,
    assemble_counter: u32,
}

impl State for Watchtower {
    fn title(&self) -> &str {
        "Watchtower"
    }

    fn initialize(&mut self, world: &mut World) {
        world.resources.user_interface.enabled = true;
    }

    fn pre_render(&mut self, renderer: &mut dyn nightshade::ecs::world::Render, world: &mut World) {
        let window_indices: Vec<usize> = world.resources.secondary_windows.states
            .iter()
            .map(|state| state.index)
            .collect();
        for index in window_indices {
            let _ = renderer.render_world_to_secondary_surface(index, world);
        }
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
                FrontendCommand::RunTest { test_name } => {
                    self.handle_run_test(&test_name);
                }
                FrontendCommand::Assemble => {
                    self.handle_assemble(world);
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
                CliEvent::ThinkingDelta { text } => {
                    self.ctx.send(BackendEvent::ThinkingDelta { text });
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

        let mcp_commands: Vec<McpCommand> = {
            let mut queue = self.mcp_command_queue.write().unwrap();
            queue.drain(..).collect()
        };

        for command in mcp_commands {
            self.handle_mcp_command(command, world);
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
    fn respond_success(&self, message: &str) {
        let mut resp = self.mcp_response_queue.write().unwrap();
        *resp = Some(McpResponse::Success(message.to_string()));
    }

    fn setup_scene(&mut self, world: &mut World, window_count: u32) {
        let camera = spawn_pan_orbit_camera(
            world,
            nalgebra_glm::Vec3::new(0.0, 2.0, 0.0),
            15.0,
            0.3,
            0.5,
            "Scene Camera".to_string(),
        );
        world.resources.active_camera = Some(camera);

        let sun = spawn_sun(world);

        self.scene.camera_entity = Some(camera);
        self.scene.sun_entity = Some(sun);
        self.scene.window_count = window_count;

        for window_index in 0..window_count {
            world.resources.secondary_windows.pending_spawns.push(WindowSpawnRequest {
                title: format!("Watchtower 3D #{}", window_index + 1),
                width: 800,
                height: 600,
                egui_enabled: false,
            });
        }
    }

    fn spawn_named(&mut self, world: &mut World, name: &str, shape: &str, position: [f32; 3], scale: [f32; 3]) {
        let pos = nalgebra_glm::Vec3::new(position[0], position[1], position[2]);
        let entity = match shape {
            "cube" => spawn_cube_at(world, pos),
            "sphere" => spawn_sphere_at(world, pos),
            "cylinder" => spawn_cylinder_at(world, pos),
            "cone" => spawn_cone_at(world, pos),
            "torus" => spawn_torus_at(world, pos),
            "plane" => spawn_plane_at(world, pos),
            _ => return,
        };

        if scale != [1.0, 1.0, 1.0] {
            if let Some(transform) = world.get_local_transform_mut(entity) {
                transform.scale = nalgebra_glm::Vec3::new(scale[0], scale[1], scale[2]);
            }
            world.set_local_transform_dirty(entity, LocalTransformDirty);
        }

        self.scene.entities.insert(name.to_string(), entity);
    }

    fn handle_assemble(&mut self, world: &mut World) {
        if self.scene.is_open() {
            self.scene.teardown(world);
        }

        let config = self.assemble_counter % 4;
        self.assemble_counter += 1;

        match config {
            0 => self.assemble_cityscape(world),
            1 => self.assemble_solar_system(world),
            2 => self.assemble_garden(world),
            _ => self.assemble_abstract(world),
        }
    }

    fn assemble_cityscape(&mut self, world: &mut World) {
        self.setup_scene(world, 2);

        self.spawn_named(world, "ground", "plane", [0.0, 0.0, 0.0], [20.0, 1.0, 20.0]);

        self.spawn_named(world, "tower_1", "cube", [-4.0, 3.0, -2.0], [2.0, 6.0, 2.0]);
        self.spawn_named(world, "tower_2", "cube", [0.0, 2.0, -3.0], [1.5, 4.0, 1.5]);
        self.spawn_named(world, "tower_3", "cube", [3.0, 4.0, -1.0], [1.8, 8.0, 1.8]);
        self.spawn_named(world, "tower_4", "cube", [-2.0, 1.5, 2.0], [2.5, 3.0, 2.5]);
        self.spawn_named(world, "tower_5", "cube", [5.0, 2.5, 3.0], [1.2, 5.0, 1.2]);

        self.spawn_named(world, "dome_1", "sphere", [-4.0, 6.0, -2.0], [1.0, 1.0, 1.0]);
        self.spawn_named(world, "dome_2", "sphere", [3.0, 8.0, -1.0], [0.9, 0.9, 0.9]);

        self.spawn_named(world, "tree_1", "cone", [6.0, 1.0, -4.0], [0.8, 2.0, 0.8]);
        self.spawn_named(world, "tree_2", "cone", [-6.0, 1.0, 4.0], [0.6, 1.5, 0.6]);
        self.spawn_named(world, "tree_3", "cone", [2.0, 0.8, 5.0], [0.7, 1.6, 0.7]);
    }

    fn assemble_solar_system(&mut self, world: &mut World) {
        self.setup_scene(world, 1);

        if let Some(camera) = self.scene.camera_entity
            && let Some(pan_orbit) = world.get_pan_orbit_camera_mut(camera)
        {
            pan_orbit.target_focus = nalgebra_glm::Vec3::new(0.0, 0.0, 0.0);
            pan_orbit.target_radius = 25.0;
            pan_orbit.target_yaw = 0.4;
            pan_orbit.target_pitch = 0.6;
        }

        self.spawn_named(world, "star", "sphere", [0.0, 0.0, 0.0], [3.0, 3.0, 3.0]);

        self.spawn_named(world, "planet_1", "sphere", [5.0, 0.0, 0.0], [0.5, 0.5, 0.5]);
        self.spawn_named(world, "planet_2", "sphere", [0.0, 0.0, 8.0], [0.8, 0.8, 0.8]);
        self.spawn_named(world, "planet_3", "sphere", [-10.0, 1.0, 2.0], [1.2, 1.2, 1.2]);
        self.spawn_named(world, "planet_4", "sphere", [3.0, 0.0, -13.0], [1.5, 1.5, 1.5]);

        self.spawn_named(world, "ring", "torus", [3.0, 0.0, -13.0], [2.5, 0.3, 2.5]);

        self.spawn_named(world, "moon_1", "sphere", [5.8, 0.5, 0.5], [0.15, 0.15, 0.15]);
        self.spawn_named(world, "moon_2", "sphere", [-10.5, 1.8, 3.0], [0.25, 0.25, 0.25]);
    }

    fn assemble_garden(&mut self, world: &mut World) {
        self.setup_scene(world, 2);

        self.spawn_named(world, "ground", "plane", [0.0, 0.0, 0.0], [15.0, 1.0, 15.0]);

        self.spawn_named(world, "fountain_base", "cylinder", [0.0, 0.3, 0.0], [2.0, 0.6, 2.0]);
        self.spawn_named(world, "fountain_ring", "torus", [0.0, 0.8, 0.0], [1.5, 0.3, 1.5]);
        self.spawn_named(world, "fountain_jet", "cylinder", [0.0, 1.5, 0.0], [0.15, 1.5, 0.15]);
        self.spawn_named(world, "fountain_top", "sphere", [0.0, 2.5, 0.0], [0.4, 0.4, 0.4]);

        self.spawn_named(world, "tree_1", "cone", [4.0, 1.5, 3.0], [1.0, 3.0, 1.0]);
        self.spawn_named(world, "trunk_1", "cylinder", [4.0, 0.4, 3.0], [0.25, 0.8, 0.25]);
        self.spawn_named(world, "tree_2", "cone", [-3.0, 2.0, -4.0], [1.2, 4.0, 1.2]);
        self.spawn_named(world, "trunk_2", "cylinder", [-3.0, 0.5, -4.0], [0.3, 1.0, 0.3]);
        self.spawn_named(world, "tree_3", "cone", [-5.0, 1.0, 2.0], [0.8, 2.0, 0.8]);
        self.spawn_named(world, "trunk_3", "cylinder", [-5.0, 0.3, 2.0], [0.2, 0.6, 0.2]);

        self.spawn_named(world, "bush_1", "sphere", [2.0, 0.4, -2.0], [0.8, 0.8, 0.8]);
        self.spawn_named(world, "bush_2", "sphere", [-1.0, 0.3, 5.0], [0.6, 0.6, 0.6]);
        self.spawn_named(world, "bush_3", "sphere", [5.0, 0.35, -1.0], [0.7, 0.7, 0.7]);

        self.spawn_named(world, "bench", "cube", [3.0, 0.3, -0.5], [1.5, 0.15, 0.5]);
        self.spawn_named(world, "bench_leg_1", "cube", [2.3, 0.15, -0.5], [0.1, 0.3, 0.4]);
        self.spawn_named(world, "bench_leg_2", "cube", [3.7, 0.15, -0.5], [0.1, 0.3, 0.4]);
    }

    fn assemble_abstract(&mut self, world: &mut World) {
        self.setup_scene(world, 3);

        if let Some(camera) = self.scene.camera_entity
            && let Some(pan_orbit) = world.get_pan_orbit_camera_mut(camera)
        {
            pan_orbit.target_focus = nalgebra_glm::Vec3::new(0.0, 3.0, 0.0);
            pan_orbit.target_radius = 20.0;
            pan_orbit.target_yaw = 0.8;
            pan_orbit.target_pitch = 0.4;
        }

        self.spawn_named(world, "base", "plane", [0.0, 0.0, 0.0], [12.0, 1.0, 12.0]);

        self.spawn_named(world, "pillar_1", "cylinder", [-3.0, 3.0, -3.0], [0.3, 6.0, 0.3]);
        self.spawn_named(world, "pillar_2", "cylinder", [3.0, 2.0, -3.0], [0.3, 4.0, 0.3]);
        self.spawn_named(world, "pillar_3", "cylinder", [-3.0, 2.5, 3.0], [0.3, 5.0, 0.3]);
        self.spawn_named(world, "pillar_4", "cylinder", [3.0, 3.5, 3.0], [0.3, 7.0, 0.3]);

        self.spawn_named(world, "orbit_1", "torus", [0.0, 4.0, 0.0], [3.0, 0.2, 3.0]);
        self.spawn_named(world, "orbit_2", "torus", [0.0, 6.0, 0.0], [2.0, 0.15, 2.0]);

        self.spawn_named(world, "core", "sphere", [0.0, 5.0, 0.0], [1.5, 1.5, 1.5]);

        self.spawn_named(world, "satellite_1", "sphere", [3.0, 4.0, 0.0], [0.4, 0.4, 0.4]);
        self.spawn_named(world, "satellite_2", "sphere", [-2.0, 6.0, 1.0], [0.3, 0.3, 0.3]);
        self.spawn_named(world, "satellite_3", "sphere", [0.0, 4.0, -2.5], [0.35, 0.35, 0.35]);

        self.spawn_named(world, "arch_left", "cube", [-5.0, 2.0, 0.0], [0.5, 4.0, 0.5]);
        self.spawn_named(world, "arch_right", "cube", [5.0, 2.0, 0.0], [0.5, 4.0, 0.5]);
        self.spawn_named(world, "arch_top", "cube", [0.0, 4.2, 0.0], [10.5, 0.4, 0.5]);

        self.spawn_named(world, "cone_1", "cone", [-6.0, 1.0, -5.0], [1.0, 2.0, 1.0]);
        self.spawn_named(world, "cone_2", "cone", [6.0, 1.5, 5.0], [1.2, 3.0, 1.2]);
        self.spawn_named(world, "cone_3", "cone", [0.0, 0.5, 6.0], [0.8, 1.0, 0.8]);
    }

    fn handle_mcp_command(&mut self, command: McpCommand, world: &mut World) {
        match command {
            McpCommand::ShowNotification { title, body } => {
                self.ctx.send(BackendEvent::Notification { title, body });
                self.respond_success("Notification shown");
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
                self.respond_success("Content displayed");
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
                self.respond_success("Status updated");
            }
            McpCommand::Open3dWindow { width, height } => {
                if self.scene.is_open() {
                    self.respond_success("3D window is already open");
                    return;
                }

                world.resources.secondary_windows.pending_spawns.push(WindowSpawnRequest {
                    title: "Watchtower 3D".to_string(),
                    width,
                    height,
                    egui_enabled: false,
                });

                let camera = spawn_pan_orbit_camera(
                    world,
                    nalgebra_glm::Vec3::new(0.0, 0.0, 0.0),
                    10.0,
                    0.0,
                    std::f32::consts::FRAC_PI_4,
                    "Scene Camera".to_string(),
                );
                world.resources.active_camera = Some(camera);

                let sun = spawn_sun(world);

                self.scene.window_count = 1;
                self.scene.camera_entity = Some(camera);
                self.scene.sun_entity = Some(sun);

                self.respond_success("3D window opened with camera and sun");
            }
            McpCommand::Close3dWindow => {
                if !self.scene.is_open() {
                    self.respond_success("3D window is not open");
                    return;
                }

                self.scene.teardown(world);
                self.respond_success("3D window closed");
            }
            McpCommand::SpawnEntity { name, shape, position, scale } => {
                if !self.scene.is_open() {
                    self.respond_success("Error: 3D window is not open");
                    return;
                }
                if self.scene.entities.contains_key(&name) {
                    self.respond_success(&format!("Error: entity '{name}' already exists"));
                    return;
                }

                let valid_shapes = ["cube", "sphere", "cylinder", "cone", "torus", "plane"];
                if !valid_shapes.contains(&shape.as_str()) {
                    self.respond_success(&format!("Error: unknown shape '{shape}'. Use: cube, sphere, cylinder, cone, torus, plane"));
                    return;
                }

                self.spawn_named(world, &name, &shape, position, scale);
                self.respond_success(&format!("Spawned {shape} entity '{name}'"));
            }
            McpCommand::RemoveEntity { name } => {
                if let Some(entity) = self.scene.entities.remove(&name) {
                    despawn_recursive_immediate(world, entity);
                    self.respond_success(&format!("Removed entity '{name}'"));
                } else {
                    self.respond_success(&format!("Error: entity '{name}' not found"));
                }
            }
            McpCommand::MoveEntity { name, position } => {
                if let Some(&entity) = self.scene.entities.get(&name) {
                    if let Some(transform) = world.get_local_transform_mut(entity) {
                        transform.translation = nalgebra_glm::Vec3::new(position[0], position[1], position[2]);
                    }
                    world.set_local_transform_dirty(entity, LocalTransformDirty);
                    self.respond_success(&format!("Moved entity '{name}' to [{}, {}, {}]", position[0], position[1], position[2]));
                } else {
                    self.respond_success(&format!("Error: entity '{name}' not found"));
                }
            }
            McpCommand::RotateEntity { name, rotation } => {
                if let Some(&entity) = self.scene.entities.get(&name) {
                    let radians_x = rotation[0].to_radians();
                    let radians_y = rotation[1].to_radians();
                    let radians_z = rotation[2].to_radians();
                    let quat = nalgebra_glm::quat_angle_axis(radians_z, &nalgebra_glm::Vec3::new(0.0, 0.0, 1.0))
                        * nalgebra_glm::quat_angle_axis(radians_y, &nalgebra_glm::Vec3::new(0.0, 1.0, 0.0))
                        * nalgebra_glm::quat_angle_axis(radians_x, &nalgebra_glm::Vec3::new(1.0, 0.0, 0.0));
                    if let Some(transform) = world.get_local_transform_mut(entity) {
                        transform.rotation = quat;
                    }
                    world.set_local_transform_dirty(entity, LocalTransformDirty);
                    self.respond_success(&format!("Rotated entity '{name}' to [{}, {}, {}] degrees", rotation[0], rotation[1], rotation[2]));
                } else {
                    self.respond_success(&format!("Error: entity '{name}' not found"));
                }
            }
            McpCommand::ScaleEntity { name, scale } => {
                if let Some(&entity) = self.scene.entities.get(&name) {
                    if let Some(transform) = world.get_local_transform_mut(entity) {
                        transform.scale = nalgebra_glm::Vec3::new(scale[0], scale[1], scale[2]);
                    }
                    world.set_local_transform_dirty(entity, LocalTransformDirty);
                    self.respond_success(&format!("Scaled entity '{name}' to [{}, {}, {}]", scale[0], scale[1], scale[2]));
                } else {
                    self.respond_success(&format!("Error: entity '{name}' not found"));
                }
            }
            McpCommand::SetCamera { focus, radius, yaw, pitch } => {
                if let Some(camera_entity) = self.scene.camera_entity {
                    let yaw_rad = yaw.to_radians();
                    let pitch_rad = pitch.to_radians();
                    if let Some(pan_orbit) = world.get_pan_orbit_camera_mut(camera_entity) {
                        pan_orbit.target_focus = nalgebra_glm::Vec3::new(focus[0], focus[1], focus[2]);
                        pan_orbit.target_radius = radius;
                        pan_orbit.target_yaw = yaw_rad;
                        pan_orbit.target_pitch = pitch_rad;
                    }
                    self.respond_success(&format!("Camera set: focus=[{}, {}, {}], radius={radius}, yaw={yaw}, pitch={pitch}", focus[0], focus[1], focus[2]));
                } else {
                    self.respond_success("Error: no camera (3D window not open)");
                }
            }
            McpCommand::ListEntities => {
                let mut entries = Vec::new();
                for (name, &entity) in &self.scene.entities {
                    let position = world.get_local_transform(entity)
                        .map(|transform| [transform.translation.x, transform.translation.y, transform.translation.z])
                        .unwrap_or([0.0, 0.0, 0.0]);
                    entries.push(serde_json::json!({
                        "name": name,
                        "position": position,
                    }));
                }
                let json = serde_json::to_string_pretty(&entries).unwrap_or_default();
                self.respond_success(&json);
            }
            McpCommand::ClearScene => {
                let count = self.scene.entities.len();
                for (_name, entity) in self.scene.entities.drain() {
                    despawn_recursive_immediate(world, entity);
                }
                self.respond_success(&format!("Cleared {count} entities from scene"));
            }
        }
    }

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
