use std::collections::HashMap;

use nightshade::prelude::*;
use summoner_protocol::PlayState;

use crate::game::GameDefinition;
use crate::history::OperationHistory;

pub struct SceneState {
    pub camera_entity: Option<Entity>,
    pub sun_entity: Option<Entity>,
    pub entities: HashMap<String, Entity>,
    pub game_definition: Option<GameDefinition>,
    pub game_title: Option<String>,
    pub play_state: PlayState,
    pub game_entities: HashMap<String, Entity>,
    pub entity_definitions: HashMap<String, String>,
    pub history: OperationHistory,
    pub editor_window_title: Option<String>,
    pub play_window_title: Option<String>,
    pub last_notified_editor_open: bool,
}

impl Default for SceneState {
    fn default() -> Self {
        Self {
            camera_entity: None,
            sun_entity: None,
            entities: HashMap::new(),
            game_definition: None,
            game_title: None,
            play_state: PlayState::Stopped,
            game_entities: HashMap::new(),
            entity_definitions: HashMap::new(),
            history: OperationHistory::default(),
            editor_window_title: None,
            play_window_title: None,
            last_notified_editor_open: false,
        }
    }
}

impl SceneState {
    pub fn has_game(&self) -> bool {
        self.game_definition.is_some()
    }

    pub fn is_editor_window_open(&self, world: &World) -> bool {
        if let Some(title) = &self.editor_window_title {
            world.resources.secondary_windows.states
                .iter()
                .any(|state| state.title == *title)
        } else {
            false
        }
    }

    pub fn is_play_window_open(&self, world: &World) -> bool {
        if let Some(title) = &self.play_window_title {
            world.resources.secondary_windows.states
                .iter()
                .any(|state| state.title == *title)
        } else {
            false
        }
    }

    pub fn close_play_window(&mut self, world: &mut World) {
        if let Some(title) = &self.play_window_title {
            for window_state in &mut world.resources.secondary_windows.states {
                if window_state.title == *title {
                    window_state.close_requested = true;
                }
            }
        }
        self.play_window_title = None;
    }

    pub fn is_open(&self) -> bool {
        self.editor_window_title.is_some() || self.play_window_title.is_some() || !self.entities.is_empty()
    }

    pub fn teardown(&mut self, world: &mut World) {
        self.despawn_all(world);
    }

    pub fn teardown_game_only(&mut self, world: &mut World) {
        self.despawn_game_entities(world);
        world.resources.entity_names.clear();
    }

    fn despawn_game_entities(&mut self, world: &mut World) {
        for (_name, entity) in self.game_entities.drain() {
            despawn_recursive_immediate(world, entity);
        }
        if let Some(camera) = self.camera_entity.take() {
            despawn_recursive_immediate(world, camera);
        }
        if let Some(sun) = self.sun_entity.take() {
            despawn_recursive_immediate(world, sun);
        }
        world.resources.active_camera = None;
    }

    fn despawn_all(&mut self, world: &mut World) {
        for window_state in &mut world.resources.secondary_windows.states {
            window_state.close_requested = true;
        }
        for (_name, entity) in self.entities.drain() {
            despawn_recursive_immediate(world, entity);
        }
        self.despawn_game_entities(world);
        self.editor_window_title = None;
        self.play_window_title = None;
        self.play_state = PlayState::Stopped;
        self.last_notified_editor_open = false;
    }
}
