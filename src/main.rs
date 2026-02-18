#![windows_subsystem = "windows"]

mod cli;
mod game;
mod history;
mod mcp_server;
mod scene;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Instant;

use include_dir::{Dir, include_dir};
use nightshade::ecs::camera::spawn_pan_orbit_camera;
use nightshade::ecs::scene::commands::spawn_scene;
use nightshade::ecs::script::components::{Script, ScriptSource};
use nightshade::ecs::script::systems::run_scripts_system;
use nightshade::prelude::*;
use nightshade::webview::{WebviewContext, serve_embedded_dir};
use summoner_protocol::{AgentStatus, BackendEvent, ContentFormat, FrontendCommand, PlayState};

use crate::cli::{CliCommand, CliEvent, spawn_cli_worker};
use crate::game::{EntityDefinition, GameDefinition, build_entity, build_scene, expand_entity_definitions};
use nightshade::ecs::world::SCRIPT;
use crate::history::Operation;
use crate::mcp_server::{
    McpCommand, McpResponse, SummonerCommandQueue, SummonerResponseQueue,
    create_summoner_mcp_queues, start_summoner_mcp_server,
};
use crate::scene::SceneState;

static DIST: Dir = include_dir!("$CARGO_MANIFEST_DIR/site/dist");

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (cli_cmd_tx, cli_cmd_rx) = mpsc::channel::<CliCommand>();
    let (cli_event_tx, cli_event_rx) = mpsc::channel::<CliEvent>();

    spawn_cli_worker(cli_cmd_rx, cli_event_tx);

    let (mcp_command_queue, mcp_response_queue) = create_summoner_mcp_queues();
    start_summoner_mcp_server(mcp_command_queue.clone(), mcp_response_queue.clone());

    let (test_result_tx, test_result_rx) = mpsc::channel::<BackendEvent>();

    launch(Summoner {
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

struct Summoner {
    port: u16,
    ctx: WebviewContext<FrontendCommand, BackendEvent>,
    connected: bool,
    cli_cmd_tx: mpsc::Sender<CliCommand>,
    cli_event_rx: mpsc::Receiver<CliEvent>,
    mcp_command_queue: SummonerCommandQueue,
    mcp_response_queue: SummonerResponseQueue,
    test_result_tx: mpsc::Sender<BackendEvent>,
    test_result_rx: mpsc::Receiver<BackendEvent>,
    cli_prompt_test_running: Arc<AtomicBool>,
    scene: SceneState,
    assemble_counter: u32,
}

impl State for Summoner {
    fn title(&self) -> &str {
        "Summoner"
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

    fn run_systems(&mut self, world: &mut World) {
        if self.scene.play_state == PlayState::Playing {
            if let Some(play_title) = &self.scene.play_window_title
                && let Some(window_state) = world.resources.secondary_windows.states
                    .iter()
                    .find(|state| state.title == *play_title)
            {
                let secondary_keys = window_state.input.keyboard_keystates.clone();
                for (key, state) in secondary_keys {
                    world.resources.input.keyboard.keystates.insert(key, state);
                }
            }

            let mut runtime = std::mem::take(&mut world.resources.script_runtime);
            run_scripts_system(world, &mut runtime);
            world.resources.script_runtime = runtime;
        }

        self.detect_window_closes(world);
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
                FrontendCommand::PlayGame => {
                    self.handle_play_game(world);
                }
                FrontendCommand::PauseGame => {
                    self.handle_pause_game(world);
                }
                FrontendCommand::StopGame => {
                    self.handle_stop_game(world);
                }
                FrontendCommand::OpenEditorWindow => {
                    self.handle_open_editor_window(world);
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

impl Summoner {
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

        for window_index in 0..window_count {
            world.resources.secondary_windows.pending_spawns.push(WindowSpawnRequest {
                title: format!("Summoner 3D #{}", window_index + 1),
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

    fn spawn_game_from_definition(&mut self, world: &mut World, definition: &GameDefinition) -> Result<String, String> {
        if self.scene.is_open() {
            self.scene.teardown_game_only(world);
        }

        let scene = build_scene(definition);
        let title = definition.title.clone();
        let editor_title = format!("Summoner - {title}");

        let editor_already_open = self.scene.editor_window_title.as_ref()
            .is_some_and(|existing| *existing == editor_title && self.scene.is_editor_window_open(world));

        if !editor_already_open {
            if let Some(existing_title) = &self.scene.editor_window_title
                && *existing_title != editor_title
            {
                for window_state in &mut world.resources.secondary_windows.states {
                    if window_state.title == *existing_title {
                        window_state.close_requested = true;
                    }
                }
            }

            world.resources.secondary_windows.pending_spawns.push(WindowSpawnRequest {
                title: editor_title.clone(),
                width: 800,
                height: 600,
                egui_enabled: false,
            });
        }

        world.resources.graphics.atmosphere = scene.atmosphere;

        match spawn_scene(world, &scene, None) {
            Ok(result) => {
                for (uuid, entity) in &result.uuid_to_entity {
                    let scene_entity = scene.entities.iter().find(|scene_entity| scene_entity.uuid == *uuid);
                    if let Some(scene_entity) = scene_entity
                        && let Some(name) = &scene_entity.name
                    {
                        if scene_entity.components.camera.is_some() || name == "Camera_Lens" {
                            if name == "Camera" || name == "Camera_Lens" {
                                self.scene.camera_entity = Some(*entity);
                                if name == "Camera_Lens" {
                                    world.resources.active_camera = Some(*entity);
                                }
                            }
                        } else if scene_entity.components.light.is_some() || name == "Sun" || name == "SunLight" {
                            self.scene.sun_entity = Some(*entity);
                        } else {
                            self.scene.game_entities.insert(name.clone(), *entity);
                        }
                    }

                    if let Some(scene_entity) = scene_entity
                        && let Some(script) = &scene_entity.components.script
                    {
                        world.add_components(*entity, SCRIPT);
                        world.set_script(*entity, Script {
                            source: script.source.clone(),
                            enabled: true,
                        });
                    }
                }

                for (key, value) in &definition.initial_state {
                    world.resources.script_runtime.game_state.insert(key.clone(), *value);
                }

                self.scene.entity_definitions.clear();
                let expanded = expand_entity_definitions(&definition.entities);
                for entity_def in &expanded {
                    if let Ok(json) = serde_json::to_string(entity_def) {
                        self.scene.entity_definitions.insert(entity_def.name.clone(), json);
                    }
                }

                self.scene.play_state = PlayState::Stopped;
                self.scene.editor_window_title = Some(editor_title);
                self.scene.game_title = Some(title.clone());
                self.scene.game_definition = Some(definition.clone());

                self.send_game_state_changed(world);

                let entity_count = self.scene.game_entities.len();
                let script_count: usize = world.query_entities(SCRIPT).count();
                Ok(format!("Game '{title}' created with {entity_count} entities and {script_count} active scripts. Editor window opened."))
            }
            Err(err) => Err(format!("Error spawning game scene: {err:?}")),
        }
    }

    fn handle_create_game(&mut self, world: &mut World, definition_json: &str) -> String {
        let definition: GameDefinition = match serde_json::from_str(definition_json) {
            Ok(def) => def,
            Err(err) => return format!("Error parsing game definition: {err}"),
        };

        if self.scene.play_state != PlayState::Stopped {
            self.scene.close_play_window(world);
            self.scene.play_state = PlayState::Stopped;
        }

        match self.spawn_game_from_definition(world, &definition) {
            Ok(message) => {
                self.scene.history.clear();
                self.scene.history.push(Operation::CreateGame {
                    definition: definition_json.to_string(),
                });
                message
            }
            Err(message) => message,
        }
    }

    fn handle_update_entity_script(&mut self, world: &mut World, entity_name: &str, script_source: &str) -> String {
        let entity = match self.scene.game_entities.get(entity_name) {
            Some(&entity) => entity,
            None => return format!("Error: entity '{entity_name}' not found"),
        };

        let old_script = world.get_script(entity).and_then(|script| {
            match &script.source {
                ScriptSource::Embedded { source } => Some(source.clone()),
                ScriptSource::File { .. } => None,
            }
        });

        let new_script = Script {
            source: ScriptSource::Embedded {
                source: script_source.to_string(),
            },
            enabled: true,
        };
        world.set_script(entity, new_script);

        let script_key = format!("entity_{entity_name}");
        world.resources.script_runtime.invalidate_script(&script_key);
        world.resources.script_runtime.remove_entity_scope(entity);

        self.scene.history.push(Operation::UpdateScript {
            entity_name: entity_name.to_string(),
            old_script,
            new_script: script_source.to_string(),
        });

        format!("Updated script on entity '{entity_name}'")
    }

    fn spawn_single_entity(&mut self, world: &mut World, entity_json: &str) -> Result<String, String> {
        let entity_def: EntityDefinition = match serde_json::from_str(entity_json) {
            Ok(def) => def,
            Err(err) => return Err(format!("Error parsing entity definition: {err}")),
        };

        if self.scene.game_entities.contains_key(&entity_def.name) {
            return Err(format!("Error: entity '{}' already exists", entity_def.name));
        }

        let name = entity_def.name.clone();
        let scene_entity = build_entity(&entity_def, None);

        let single_scene = nightshade::ecs::scene::components::Scene {
            header: nightshade::ecs::scene::components::SceneHeader::default(),
            atmosphere: Atmosphere::None,
            hdr_skybox: None,
            entities: vec![scene_entity],
            joints: Vec::new(),
            layers: Vec::new(),
            chunks: Vec::new(),
            embedded_textures: std::collections::HashMap::new(),
            embedded_audio: std::collections::HashMap::new(),
            metadata: std::collections::HashMap::new(),
            navmesh: None,
            spawn_order: Vec::new(),
            uuid_index: std::collections::HashMap::new(),
            chunk_index: std::collections::HashMap::new(),
        };

        match spawn_scene(world, &single_scene, None) {
            Ok(result) => {
                for entity in result.uuid_to_entity.values() {
                    self.scene.game_entities.insert(name.clone(), *entity);
                }
                self.scene.entity_definitions.insert(name.clone(), entity_json.to_string());
                Ok(name)
            }
            Err(err) => Err(format!("Error spawning entity: {err:?}")),
        }
    }

    fn handle_add_game_entity(&mut self, world: &mut World, entity_json: &str) -> String {
        match self.spawn_single_entity(world, entity_json) {
            Ok(name) => {
                self.scene.history.push(Operation::AddEntity {
                    name: name.clone(),
                    entity_json: entity_json.to_string(),
                });
                format!("Added entity '{name}' to game")
            }
            Err(message) => message,
        }
    }

    fn handle_remove_game_entity(&mut self, world: &mut World, name: &str) -> String {
        if let Some(entity) = self.scene.game_entities.remove(name) {
            let entity_json = self.scene.entity_definitions.remove(name)
                .unwrap_or_else(|| serde_json::json!({"name": name}).to_string());

            despawn_recursive_immediate(world, entity);
            world.resources.entity_names.remove(name);

            self.scene.history.push(Operation::RemoveEntity {
                name: name.to_string(),
                entity_json,
            });

            format!("Removed entity '{name}' from game")
        } else {
            format!("Error: entity '{name}' not found")
        }
    }

    fn handle_set_game_state(&mut self, world: &mut World, key: &str, value: f64) -> String {
        let old_value = world.resources.script_runtime.game_state.get(key).copied();
        world.resources.script_runtime.game_state.insert(key.to_string(), value);

        self.scene.history.push(Operation::SetGameState {
            key: key.to_string(),
            old_value,
            new_value: value,
        });

        format!("Set state '{key}' = {value}")
    }

    fn handle_get_game_state(&self, world: &World) -> String {
        let state = &world.resources.script_runtime.game_state;
        serde_json::to_string_pretty(state).unwrap_or_default()
    }

    fn handle_get_scene_info(&self, world: &World) -> String {
        let mut entities_info = Vec::new();

        for (name, &entity) in &self.scene.game_entities {
            let transform = world.get_local_transform(entity);
            let position = transform
                .map(|t| [t.translation.x, t.translation.y, t.translation.z])
                .unwrap_or([0.0, 0.0, 0.0]);
            let scale = transform
                .map(|t| [t.scale.x, t.scale.y, t.scale.z])
                .unwrap_or([1.0, 1.0, 1.0]);

            let script_source = world.get_script(entity).map(|script| {
                match &script.source {
                    ScriptSource::Embedded { source } => source.clone(),
                    ScriptSource::File { path } => format!("file:{path}"),
                }
            });

            entities_info.push(serde_json::json!({
                "name": name,
                "position": position,
                "scale": scale,
                "has_script": script_source.is_some(),
                "script": script_source,
            }));
        }

        let game_state = &world.resources.script_runtime.game_state;

        let result = serde_json::json!({
            "game_title": self.scene.game_title,
            "play_state": format!("{:?}", self.scene.play_state),
            "atmosphere": self.scene.game_definition.as_ref().map(|def| def.atmosphere.as_str()),
            "entity_count": self.scene.game_entities.len(),
            "entities": entities_info,
            "game_state": game_state,
        });

        serde_json::to_string_pretty(&result).unwrap_or_default()
    }

    fn handle_reset_game(&mut self, world: &mut World) -> String {
        let definition = match &self.scene.game_definition {
            Some(def) => def.clone(),
            None => return "Error: no game to reset (create one first)".to_string(),
        };

        if self.scene.play_state != PlayState::Stopped {
            self.scene.close_play_window(world);
            self.scene.play_state = PlayState::Stopped;
        }

        world.resources.script_runtime.reset_game_state();
        world.resources.script_runtime.reset_time();

        match self.spawn_game_from_definition(world, &definition) {
            Ok(message) => {
                self.scene.history.push(Operation::ResetGame);
                message
            }
            Err(message) => message,
        }
    }

    fn handle_undo(&mut self, world: &mut World) -> String {
        let operation = match self.scene.history.undo() {
            Some(op) => op.clone(),
            None => return "Nothing to undo".to_string(),
        };

        let description = operation.description();

        match operation {
            Operation::UpdateScript { entity_name, old_script, .. } => {
                if let Some(&entity) = self.scene.game_entities.get(&entity_name) {
                    match old_script {
                        Some(source) => {
                            let script = Script {
                                source: ScriptSource::Embedded { source },
                                enabled: true,
                            };
                            world.set_script(entity, script);
                        }
                        None => {
                            let script = Script {
                                source: ScriptSource::Embedded { source: String::new() },
                                enabled: false,
                            };
                            world.set_script(entity, script);
                        }
                    }
                    world.resources.script_runtime.remove_entity_scope(entity);
                }
            }
            Operation::AddEntity { name, .. } => {
                if let Some(entity) = self.scene.game_entities.remove(&name) {
                    despawn_recursive_immediate(world, entity);
                    world.resources.entity_names.remove(&name);
                }
            }
            Operation::RemoveEntity { entity_json, .. } => {
                let _ = self.spawn_single_entity(world, &entity_json);
            }
            Operation::SetGameState { key, old_value, .. } => {
                match old_value {
                    Some(value) => {
                        world.resources.script_runtime.game_state.insert(key, value);
                    }
                    None => {
                        world.resources.script_runtime.game_state.remove(&key);
                    }
                }
            }
            Operation::CreateGame { .. } | Operation::ResetGame => {
                self.scene.teardown_game_only(world);
                world.resources.script_runtime.reset_game_state();
            }
        }

        format!("Undone: {description}")
    }

    fn handle_redo(&mut self, world: &mut World) -> String {
        let operation = match self.scene.history.redo() {
            Some(op) => op.clone(),
            None => return "Nothing to redo".to_string(),
        };

        let description = operation.description();

        match operation {
            Operation::UpdateScript { entity_name, new_script, .. } => {
                if let Some(&entity) = self.scene.game_entities.get(&entity_name) {
                    let script = Script {
                        source: ScriptSource::Embedded { source: new_script },
                        enabled: true,
                    };
                    world.set_script(entity, script);
                    world.resources.script_runtime.remove_entity_scope(entity);
                }
            }
            Operation::AddEntity { entity_json, .. } => {
                let _ = self.spawn_single_entity(world, &entity_json);
            }
            Operation::RemoveEntity { name, .. } => {
                if let Some(entity) = self.scene.game_entities.remove(&name) {
                    despawn_recursive_immediate(world, entity);
                    world.resources.entity_names.remove(&name);
                }
            }
            Operation::SetGameState { key, new_value, .. } => {
                world.resources.script_runtime.game_state.insert(key, new_value);
            }
            Operation::CreateGame { definition } => {
                if let Ok(def) = serde_json::from_str::<GameDefinition>(&definition) {
                    let _ = self.spawn_game_from_definition(world, &def);
                }
            }
            Operation::ResetGame => {
                if let Some(definition) = self.scene.game_definition.clone() {
                    self.scene.teardown_game_only(world);
                    world.resources.script_runtime.reset_game_state();
                    world.resources.script_runtime.reset_time();
                    let _ = self.spawn_game_from_definition(world, &definition);
                }
            }
        }

        format!("Redone: {description}")
    }

    fn handle_export_scene(&self, world: &World, path: &str) -> String {
        let definition = match &self.scene.game_definition {
            Some(def) => def,
            None => return "Error: no game to export (create one first)".to_string(),
        };

        let mut scene = build_scene(definition);

        for (name, &entity) in &self.scene.game_entities {
            if let Some(scene_entity) = scene.entities.iter_mut().find(|scene_entity| scene_entity.name.as_deref() == Some(name))
                && let Some(transform) = world.get_local_transform(entity)
            {
                scene_entity.transform = *transform;
            }
        }

        match serde_json::to_string_pretty(&scene) {
            Ok(json) => {
                match std::fs::write(path, &json) {
                    Ok(()) => format!("Exported scene to '{path}' ({} bytes)", json.len()),
                    Err(err) => format!("Error writing file '{path}': {err}"),
                }
            }
            Err(err) => format!("Error serializing scene: {err}"),
        }
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
                    title: "Summoner 3D".to_string(),
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
                for (name, &entity) in &self.scene.game_entities {
                    let position = world.get_local_transform(entity)
                        .map(|transform| [transform.translation.x, transform.translation.y, transform.translation.z])
                        .unwrap_or([0.0, 0.0, 0.0]);
                    entries.push(serde_json::json!({
                        "name": name,
                        "position": position,
                        "game_entity": true,
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
            McpCommand::CreateGame { definition } => {
                let result = self.handle_create_game(world, &definition);
                self.respond_success(&result);
            }
            McpCommand::UpdateEntityScript { entity_name, script } => {
                let result = self.handle_update_entity_script(world, &entity_name, &script);
                self.respond_success(&result);
            }
            McpCommand::AddGameEntity { entity_json } => {
                let result = self.handle_add_game_entity(world, &entity_json);
                self.respond_success(&result);
            }
            McpCommand::RemoveGameEntity { name } => {
                let result = self.handle_remove_game_entity(world, &name);
                self.respond_success(&result);
            }
            McpCommand::SetGameState { key, value } => {
                let result = self.handle_set_game_state(world, &key, value);
                self.respond_success(&result);
            }
            McpCommand::GetGameState => {
                let result = self.handle_get_game_state(world);
                self.respond_success(&result);
            }
            McpCommand::GetSceneInfo => {
                let result = self.handle_get_scene_info(world);
                self.respond_success(&result);
            }
            McpCommand::ResetGame => {
                let result = self.handle_reset_game(world);
                self.respond_success(&result);
            }
            McpCommand::Undo => {
                let result = self.handle_undo(world);
                self.respond_success(&result);
            }
            McpCommand::Redo => {
                let result = self.handle_redo(world);
                self.respond_success(&result);
            }
            McpCommand::GetHistory => {
                let result = self.scene.history.to_json();
                self.respond_success(&result);
            }
            McpCommand::ExportScene { path } => {
                let result = self.handle_export_scene(world, &path);
                self.respond_success(&result);
            }
        }
    }

    fn send_game_state_changed(&self, world: &World) {
        self.ctx.send(BackendEvent::GameStateChanged {
            has_game: self.scene.has_game(),
            play_state: self.scene.play_state,
            editor_window_open: self.scene.is_editor_window_open(world),
        });
    }

    fn detect_window_closes(&mut self, world: &mut World) {
        if self.scene.play_window_title.is_some() && !self.scene.is_play_window_open(world) {
            self.scene.play_window_title = None;
            if self.scene.play_state != PlayState::Stopped {
                self.scene.play_state = PlayState::Stopped;
                if let Some(definition) = self.scene.game_definition.clone() {
                    world.resources.script_runtime.reset_game_state();
                    world.resources.script_runtime.reset_time();
                    let _ = self.spawn_game_from_definition(world, &definition);
                }
            }
        }

        if self.scene.editor_window_title.is_some() && !self.scene.is_editor_window_open(world) {
            self.scene.editor_window_title = None;
        }

        let editor_open = self.scene.is_editor_window_open(world);
        if editor_open != self.scene.last_notified_editor_open {
            self.scene.last_notified_editor_open = editor_open;
            self.send_game_state_changed(world);
        }
    }

    fn handle_play_game(&mut self, world: &mut World) {
        if !self.scene.has_game() || self.scene.play_state == PlayState::Playing {
            return;
        }

        let title = self.scene.game_title.as_deref().unwrap_or("Game");
        let play_title = format!("Summoner - {title} [Play]");

        if self.scene.play_state == PlayState::Stopped {
            world.resources.secondary_windows.pending_spawns.push(WindowSpawnRequest {
                title: play_title.clone(),
                width: 800,
                height: 600,
                egui_enabled: false,
            });
            self.scene.play_window_title = Some(play_title);
            world.resources.script_runtime.reset_time();
        }

        self.scene.play_state = PlayState::Playing;
        self.send_game_state_changed(world);
    }

    fn handle_pause_game(&mut self, world: &World) {
        if self.scene.play_state != PlayState::Playing {
            return;
        }

        self.scene.play_state = PlayState::Paused;
        self.send_game_state_changed(world);
    }

    fn handle_stop_game(&mut self, world: &mut World) {
        if self.scene.play_state == PlayState::Stopped {
            return;
        }

        self.scene.close_play_window(world);
        self.scene.play_state = PlayState::Stopped;

        if let Some(definition) = self.scene.game_definition.clone() {
            world.resources.script_runtime.reset_game_state();
            world.resources.script_runtime.reset_time();
            let _ = self.spawn_game_from_definition(world, &definition);
        }
    }

    fn handle_open_editor_window(&mut self, world: &mut World) {
        if self.scene.is_editor_window_open(world) {
            return;
        }

        let title = self.scene.game_title.as_deref().unwrap_or("Game");
        let editor_title = format!("Summoner - {title}");

        world.resources.secondary_windows.pending_spawns.push(WindowSpawnRequest {
            title: editor_title.clone(),
            width: 800,
            height: 600,
            egui_enabled: false,
        });
        self.scene.editor_window_title = Some(editor_title);
        self.send_game_state_changed(world);
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
                                "name": "summoner-test",
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
