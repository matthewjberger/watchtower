use std::collections::HashMap;
use nightshade::prelude::*;

#[derive(Default)]
pub struct SceneState {
    pub window_count: u32,
    pub camera_entity: Option<Entity>,
    pub sun_entity: Option<Entity>,
    pub entities: HashMap<String, Entity>,
}

impl SceneState {
    pub fn is_open(&self) -> bool {
        self.window_count > 0
    }

    pub fn teardown(&mut self, world: &mut World) {
        for window_state in &mut world.resources.secondary_windows.states {
            window_state.close_requested = true;
        }
        for (_name, entity) in self.entities.drain() {
            despawn_recursive_immediate(world, entity);
        }
        if let Some(camera) = self.camera_entity.take() {
            despawn_recursive_immediate(world, camera);
        }
        if let Some(sun) = self.sun_entity.take() {
            despawn_recursive_immediate(world, sun);
        }
        world.resources.active_camera = None;
        self.window_count = 0;
    }
}
