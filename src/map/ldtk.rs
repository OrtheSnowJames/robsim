use bevy::prelude::*;
use bevy::ecs::system::SystemParam;
use bevy_ecs_ldtk::prelude::*;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

use crate::player::Player;

#[derive(SystemParam)]
pub struct LdtkEntityByNameQuery<'w, 's> {
    entities: Query<'w, 's, (Entity, &'static EntityInstance, &'static Transform)>,
}

impl<'w, 's> LdtkEntityByNameQuery<'w, 's> {
    pub fn first_named(&self, name: &str) -> Option<(Entity, &EntityInstance, &Transform)> {
        self.entities
            .iter()
            .find(|(_, instance, _)| instance.identifier == name)
    }

    pub fn iter_named(
        &self,
        name: &str,
    ) -> impl Iterator<Item = (Entity, &EntityInstance, &Transform)> + '_ {
        let target = name.to_string();
        self.entities
            .iter()
            .filter(move |(_, instance, _)| instance.identifier == target)
    }
}

pub const TOWN_MAP_ASSET_PATH: &str = "maps/town.ldtk";
const ROAD_COLLISION_VALUE: i32 = 1;
const ROAD_COLLISION_LAYER: &str = "RoadCollision";

pub struct MapPlugin;

impl Plugin for MapPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(LdtkPlugin);
        app.init_resource::<LoadedMap>();
        app.insert_resource(LevelSelection::index(0));
        app.register_ldtk_entity::<PlayerStartBundle>("PlayerStart");
        app.register_ldtk_int_cell_for_layer::<RoadCollisionBundle>(
            ROAD_COLLISION_LAYER,
            ROAD_COLLISION_VALUE,
        );
        app.add_systems(Startup, spawn_initial_ldtk_world);
        app.add_systems(Update, (materialize_transfer_portals, apply_player_start));
    }
}

#[derive(Component, Default)]
pub struct PlayerStartMarker;

#[derive(Component)]
pub struct TransferPortal {
    pub scene: String,
    pub width: f32,
    pub height: f32,
}

#[derive(Component, Default)]
struct RoadCollisionMarker;

#[derive(Default, Bundle, LdtkEntity)]
struct PlayerStartBundle {
    player_start: PlayerStartMarker,
}

#[derive(Default, Bundle, LdtkIntCell)]
struct RoadCollisionBundle {
    road_collision: RoadCollisionMarker,
}

#[derive(Resource, Clone)]
pub struct LoadedMap {
    pub asset_path: String,
}

impl Default for LoadedMap {
    fn default() -> Self {
        Self {
            asset_path: TOWN_MAP_ASSET_PATH.to_string(),
        }
    }
}

pub fn scene_to_asset_path(scene: &str) -> String {
    let trimmed = scene.trim();
    let with_ext = if trimmed.ends_with(".ldtk") {
        trimmed.to_string()
    } else {
        format!("{trimmed}.ldtk")
    };

    if with_ext.contains('/') {
        with_ext
    } else {
        format!("maps/{with_ext}")
    }
}

pub fn set_loaded_map(loaded_map: &mut LoadedMap, asset_path: impl Into<String>) {
    loaded_map.asset_path = asset_path.into();
}

pub fn loaded_map_path(loaded_map: &LoadedMap) -> &str {
    loaded_map.asset_path.as_str()
}

fn spawn_initial_ldtk_world(mut commands: Commands, assets: Res<AssetServer>) {
    commands.spawn(LdtkWorldBundle {
        ldtk_handle: assets.load(TOWN_MAP_ASSET_PATH).into(),
        ..Default::default()
    });
}

fn materialize_transfer_portals(
    mut commands: Commands,
    entities: Query<(Entity, &EntityInstance), Added<EntityInstance>>,
) {
    for (entity, entity_instance) in &entities {
        if !entity_instance.identifier.starts_with("Transfer") {
            continue;
        }

        let scene = match entity_instance.get_string_field("scene") {
            Ok(scene) if !scene.trim().is_empty() => scene.trim().to_string(),
            _ => continue,
        };

        commands.entity(entity).insert(TransferPortal {
            scene,
            width: entity_instance.width as f32,
            height: entity_instance.height as f32,
        });
    }
}

fn apply_player_start(
    mut players: Query<&mut Transform, With<Player>>,
    start_markers: Query<&Transform, (Added<PlayerStartMarker>, Without<Player>)>,
) {
    let Ok(start_transform) = start_markers.single() else {
        return;
    };
    let Ok(mut player_transform) = players.single_mut() else {
        return;
    };

    player_transform.translation.x = start_transform.translation.x;
    player_transform.translation.y = start_transform.translation.y;
}

pub fn load_or_spawn_ldtk_world(
    commands: &mut Commands,
    asset_server: &AssetServer,
    ldtk_world_query: &mut Query<(Entity, &mut LdtkProjectHandle)>,
    asset_path: String,
) {
    let mut found_world = false;
    for (_, mut ldtk_world_handle) in ldtk_world_query.iter_mut() {
        *ldtk_world_handle = asset_server.load::<LdtkProject>(asset_path.clone()).into();
        found_world = true;
        break;
    }
    if !found_world {
        commands.spawn(LdtkWorldBundle {
            ldtk_handle: asset_server.load::<LdtkProject>(asset_path).into(),
            ..Default::default()
        });
    }
}

pub fn despawn_ldtk_world(
    commands: &mut Commands,
    ldtk_world_query: &mut Query<(Entity, &mut LdtkProjectHandle)>,
) {
    for (world_entity, _) in ldtk_world_query.iter_mut() {
        commands.entity(world_entity).despawn();
    }
}

pub(crate) fn map_asset_to_disk_path(asset_path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("assets")
        .join(asset_path)
}

pub(crate) fn read_map_json(loaded_map: &LoadedMap) -> Result<Value, String> {
    let asset_path = loaded_map_path(loaded_map);
    let disk_path = map_asset_to_disk_path(asset_path);
    if !disk_path.exists() {
        return Err(format!("Map file not found: {}", disk_path.display()));
    }
    let file = fs::read_to_string(&disk_path)
        .map_err(|e| format!("Failed reading map file {}: {e}", disk_path.display()))?;
    serde_json::from_str(&file).map_err(|e| format!("Failed parsing LDtk JSON: {e}"))
}

pub fn current_map_occupancy_grid(loaded_map: &LoadedMap) -> Result<Vec<Vec<u8>>, String> {
    let json = read_map_json(loaded_map)?;
    map_occupancy_grid_from_ldtk_json(&json)
}

pub fn map_occupancy_grid_from_ldtk_json(ldtk_json: &Value) -> Result<Vec<Vec<u8>>, String> {
    let first_level = ldtk_first_level(ldtk_json)?;
    let layer_instances = ldtk_layer_instances(first_level)?;
    let (width, height) = ldtk_level_dimensions(first_level, layer_instances)?;

    if width == 0 || height == 0 {
        return Ok(Vec::new());
    }

    let mut grid = vec![vec![0_u8; width]; height];
    for layer in layer_instances {
        apply_layer_occupancy(layer, &mut grid);
    }

    Ok(grid)
}

pub(crate) fn ldtk_first_level(ldtk_json: &Value) -> Result<&Value, String> {
    let levels = ldtk_json
        .get("levels")
        .and_then(Value::as_array)
        .ok_or_else(|| "LDtk JSON missing `levels` array".to_string())?;
    levels
        .first()
        .ok_or_else(|| "LDtk JSON has no levels".to_string())
}

pub(crate) fn ldtk_layer_instances(level: &Value) -> Result<&[Value], String> {
    let layers = level
        .get("layerInstances")
        .and_then(Value::as_array)
        .ok_or_else(|| "Level missing `layerInstances`".to_string())?;
    Ok(layers.as_slice())
}

pub(crate) fn ldtk_level_dimensions(level: &Value, layers: &[Value]) -> Result<(usize, usize), String> {
    let default_grid_size = layers
        .iter()
        .find_map(|layer| layer.get("__gridSize").and_then(Value::as_i64))
        .unwrap_or(16)
        .max(1) as usize;

    let width = level
        .get("__cWid")
        .and_then(Value::as_u64)
        .map(|v| v as usize)
        .or_else(|| {
            level
                .get("pxWid")
                .and_then(Value::as_u64)
                .map(|px| (px as usize) / default_grid_size)
        })
        .or_else(|| {
            layers
                .iter()
                .find_map(|layer| layer.get("__cWid").and_then(Value::as_u64).map(|v| v as usize))
        })
        .ok_or_else(|| "Level missing usable width (`__cWid` or `pxWid`)".to_string())?;

    let height = level
        .get("__cHei")
        .and_then(Value::as_u64)
        .map(|v| v as usize)
        .or_else(|| {
            level
                .get("pxHei")
                .and_then(Value::as_u64)
                .map(|px| (px as usize) / default_grid_size)
        })
        .or_else(|| {
            layers
                .iter()
                .find_map(|layer| layer.get("__cHei").and_then(Value::as_u64).map(|v| v as usize))
        })
        .ok_or_else(|| "Level missing usable height (`__cHei` or `pxHei`)".to_string())?;

    Ok((width, height))
}

fn apply_layer_occupancy(layer: &Value, grid: &mut [Vec<u8>]) {
    let layer_type = layer
        .get("__type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let layer_grid_size = layer
        .get("__gridSize")
        .and_then(Value::as_i64)
        .unwrap_or(16)
        .max(1) as i32;

    mark_cells_from_tile_array(layer.get("gridTiles"), layer_grid_size, grid);
    mark_cells_from_tile_array(layer.get("autoLayerTiles"), layer_grid_size, grid);

    if layer_type == "IntGrid" {
        apply_intgrid_occupancy(layer, grid);
    }
}

fn apply_intgrid_occupancy(layer: &Value, grid: &mut [Vec<u8>]) {
    if let (Some(csv), Some(c_wid)) = (
        layer.get("intGridCsv").and_then(Value::as_array),
        layer.get("__cWid").and_then(Value::as_u64),
    ) {
        let c_wid = c_wid as usize;
        for (idx, cell) in csv.iter().enumerate() {
            let val = cell.as_i64().unwrap_or(0);
            if val <= 0 {
                continue;
            }
            let x = idx % c_wid;
            let y = idx / c_wid;
            if y < grid.len() && x < grid[y].len() {
                grid[y][x] = 1;
            }
        }
    }
}

pub(crate) fn mark_cells_from_tile_array(tile_array: Option<&Value>, grid_size: i32, grid: &mut [Vec<u8>]) {
    let Some(tiles) = tile_array.and_then(Value::as_array) else {
        return;
    };

    for tile in tiles {
        let Some(px) = tile.get("px").and_then(Value::as_array) else {
            continue;
        };
        if px.len() < 2 {
            continue;
        }

        let Some(px_x) = px[0].as_i64() else {
            continue;
        };
        let Some(px_y) = px[1].as_i64() else {
            continue;
        };

        let x = (px_x as i32 / grid_size) as isize;
        let y = (px_y as i32 / grid_size) as isize;
        if x < 0 || y < 0 {
            continue;
        }
        let x = x as usize;
        let y = y as usize;
        if y < grid.len() && x < grid[y].len() {
            grid[y][x] = 1;
        }
    }
}
