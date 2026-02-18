use std::collections::HashMap;

use nightshade::ecs::scene::components::{
    Scene, SceneCamera, SceneComponents, SceneEntity, SceneHeader, SceneLight, SceneMaterial,
    SceneMesh,
};
use nightshade::ecs::script::components::{Script, ScriptSource};
use nightshade::prelude::*;

#[derive(Clone, serde::Deserialize, serde::Serialize)]
pub struct GameDefinition {
    pub title: String,
    #[serde(default = "default_atmosphere_name")]
    pub atmosphere: String,
    #[serde(default)]
    pub camera: CameraDefinition,
    #[serde(default)]
    pub sun: SunDefinition,
    #[serde(default)]
    pub initial_state: HashMap<String, f64>,
    #[serde(default)]
    pub entities: Vec<EntityDefinition>,
}

fn default_atmosphere_name() -> String {
    "Sky".to_string()
}

#[derive(Clone, serde::Deserialize, serde::Serialize)]
pub struct CameraDefinition {
    #[serde(default = "default_camera_position")]
    pub position: [f32; 3],
    #[serde(default = "default_fov")]
    pub fov: f32,
}

fn default_camera_position() -> [f32; 3] {
    [0.0, 5.0, 18.0]
}

fn default_fov() -> f32 {
    1.0
}

impl Default for CameraDefinition {
    fn default() -> Self {
        Self {
            position: default_camera_position(),
            fov: default_fov(),
        }
    }
}

#[derive(Clone, serde::Deserialize, serde::Serialize)]
pub struct SunDefinition {
    #[serde(default = "default_sun_direction")]
    pub direction: [f32; 3],
    #[serde(default = "default_sun_intensity")]
    pub intensity: f32,
}

fn default_sun_direction() -> [f32; 3] {
    [5.0, 10.0, 5.0]
}

fn default_sun_intensity() -> f32 {
    5.0
}

impl Default for SunDefinition {
    fn default() -> Self {
        Self {
            direction: default_sun_direction(),
            intensity: default_sun_intensity(),
        }
    }
}

#[derive(Clone, serde::Deserialize, serde::Serialize)]
pub struct EntityDefinition {
    pub name: String,
    #[serde(default = "default_mesh")]
    pub mesh: String,
    #[serde(default)]
    pub position: [f32; 3],
    #[serde(default = "default_scale")]
    pub scale: [f32; 3],
    #[serde(default = "default_color")]
    pub color: [f32; 4],
    #[serde(default = "default_roughness")]
    pub roughness: f32,
    #[serde(default)]
    pub metallic: f32,
    #[serde(default)]
    pub emissive: [f32; 3],
    #[serde(default)]
    pub script: Option<String>,
    #[serde(default)]
    pub grid: Option<GridDefinition>,
}

#[derive(Clone, serde::Deserialize, serde::Serialize)]
pub struct GridDefinition {
    pub count: [u32; 2],
    #[serde(default = "default_grid_spacing")]
    pub spacing: [f32; 2],
}

fn default_grid_spacing() -> [f32; 2] {
    [2.0, 1.0]
}

pub fn expand_entity_definitions(definitions: &[EntityDefinition]) -> Vec<EntityDefinition> {
    let mut expanded = Vec::new();
    for definition in definitions {
        if let Some(grid) = &definition.grid {
            let cols = grid.count[0];
            let rows = grid.count[1];
            let spacing_x = grid.spacing[0];
            let spacing_y = grid.spacing[1];
            let total_width = (cols as f32 - 1.0) * spacing_x;
            let start_x = definition.position[0] - total_width / 2.0;
            let start_y = definition.position[1];
            for row in 0..rows {
                for col in 0..cols {
                    let mut instance = definition.clone();
                    instance.name = format!("{}_{}", definition.name, row * cols + col);
                    instance.position = [
                        start_x + col as f32 * spacing_x,
                        start_y + row as f32 * spacing_y,
                        definition.position[2],
                    ];
                    instance.grid = None;
                    expanded.push(instance);
                }
            }
        } else {
            expanded.push(definition.clone());
        }
    }
    expanded
}

fn default_mesh() -> String {
    "Cube".to_string()
}

fn default_scale() -> [f32; 3] {
    [1.0, 1.0, 1.0]
}

fn default_color() -> [f32; 4] {
    [1.0, 1.0, 1.0, 1.0]
}

fn default_roughness() -> f32 {
    0.5
}

pub fn parse_atmosphere(name: &str) -> Atmosphere {
    match name {
        "None" => Atmosphere::None,
        "Sky" => Atmosphere::Sky,
        "CloudySky" => Atmosphere::CloudySky,
        "Space" => Atmosphere::Space,
        "Nebula" => Atmosphere::Nebula,
        "Sunset" => Atmosphere::Sunset,
        "DayNight" => Atmosphere::DayNight,
        _ => Atmosphere::Sky,
    }
}

pub fn build_scene(definition: &GameDefinition) -> Scene {
    let mut entities = Vec::new();

    let camera_parent_uuid = AssetUuid::new();
    let camera_lens_uuid = AssetUuid::new();

    let camera_parent = SceneEntity {
        uuid: camera_parent_uuid,
        parent: None,
        name: Some("Camera".to_string()),
        transform: LocalTransform {
            translation: nalgebra_glm::Vec3::new(
                definition.camera.position[0],
                definition.camera.position[1],
                definition.camera.position[2],
            ),
            rotation: nalgebra_glm::Quat::identity(),
            scale: nalgebra_glm::Vec3::new(1.0, 1.0, 1.0),
        },
        layer: None,
        chunk_id: None,
        components: SceneComponents::new(),
    };
    entities.push(camera_parent);

    let camera_lens = SceneEntity {
        uuid: camera_lens_uuid,
        parent: Some(camera_parent_uuid),
        name: Some("Camera_Lens".to_string()),
        transform: LocalTransform::default(),
        layer: None,
        chunk_id: None,
        components: SceneComponents {
            camera: Some(SceneCamera::Perspective {
                aspect_ratio: None,
                y_fov_rad: definition.camera.fov,
                z_far: Some(1000.0),
                z_near: 0.1,
            }),
            ..SceneComponents::new()
        },
    };
    entities.push(camera_lens);

    let sun_parent_uuid = AssetUuid::new();
    let sun_light_uuid = AssetUuid::new();

    let sun_parent = SceneEntity {
        uuid: sun_parent_uuid,
        parent: None,
        name: Some("Sun".to_string()),
        transform: LocalTransform {
            translation: nalgebra_glm::Vec3::new(
                definition.sun.direction[0],
                definition.sun.direction[1],
                definition.sun.direction[2],
            ),
            rotation: nalgebra_glm::Quat::identity(),
            scale: nalgebra_glm::Vec3::new(1.0, 1.0, 1.0),
        },
        layer: None,
        chunk_id: None,
        components: SceneComponents::new(),
    };
    entities.push(sun_parent);

    let sun_light = SceneEntity {
        uuid: sun_light_uuid,
        parent: Some(sun_parent_uuid),
        name: Some("SunLight".to_string()),
        transform: LocalTransform::default(),
        layer: None,
        chunk_id: None,
        components: SceneComponents {
            light: Some(SceneLight::Directional {
                color: [1.0, 0.95, 0.8],
                intensity: definition.sun.intensity,
                cast_shadows: true,
                shadow_bias: 0.0005,
            }),
            ..SceneComponents::new()
        },
    };
    entities.push(sun_light);

    let expanded_entities = expand_entity_definitions(&definition.entities);
    for entity_def in &expanded_entities {
        let entity = build_entity(entity_def, None);
        entities.push(entity);
    }

    let mut scene = Scene {
        header: SceneHeader::default(),
        atmosphere: parse_atmosphere(&definition.atmosphere),
        hdr_skybox: None,
        entities,
        joints: Vec::new(),
        layers: Vec::new(),
        chunks: Vec::new(),
        embedded_textures: HashMap::new(),
        embedded_audio: HashMap::new(),
        metadata: HashMap::new(),
        navmesh: None,
        spawn_order: Vec::new(),
        uuid_index: HashMap::new(),
        chunk_index: HashMap::new(),
    };

    scene.header.name = definition.title.clone();
    scene.compute_spawn_order();

    scene
}

pub fn build_entity(entity_def: &EntityDefinition, parent: Option<AssetUuid>) -> SceneEntity {
    let mesh_name = capitalize_mesh_name(&entity_def.mesh);

    let material = SceneMaterial {
        base_color: entity_def.color,
        roughness: entity_def.roughness,
        metallic: entity_def.metallic,
        emissive_factor: entity_def.emissive,
        ..SceneMaterial::default()
    };

    let script = entity_def.script.as_ref().map(|source| Script {
        source: ScriptSource::Embedded {
            source: source.clone(),
        },
        enabled: true,
    });

    SceneEntity {
        uuid: AssetUuid::new(),
        parent,
        name: Some(entity_def.name.clone()),
        transform: LocalTransform {
            translation: nalgebra_glm::Vec3::new(
                entity_def.position[0],
                entity_def.position[1],
                entity_def.position[2],
            ),
            rotation: nalgebra_glm::Quat::identity(),
            scale: nalgebra_glm::Vec3::new(
                entity_def.scale[0],
                entity_def.scale[1],
                entity_def.scale[2],
            ),
        },
        layer: None,
        chunk_id: None,
        components: SceneComponents {
            mesh: Some(SceneMesh::from_name(mesh_name).with_material(material)),
            script,
            ..SceneComponents::new()
        },
    }
}

fn capitalize_mesh_name(name: &str) -> String {
    match name.to_lowercase().as_str() {
        "cube" => "Cube".to_string(),
        "sphere" => "Sphere".to_string(),
        "cylinder" => "Cylinder".to_string(),
        "cone" => "Cone".to_string(),
        "torus" => "Torus".to_string(),
        "plane" => "Plane".to_string(),
        other => other.to_string(),
    }
}
