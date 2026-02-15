use std::collections::HashMap;
use nightshade::prelude::*;

#[derive(Default)]
pub struct SceneState {
    pub window_open: bool,
    pub camera_entity: Option<Entity>,
    pub sun_entity: Option<Entity>,
    pub entities: HashMap<String, Entity>,
}
