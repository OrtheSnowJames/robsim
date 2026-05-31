use bevy::prelude::*;
use bevy::ecs::system::SystemParam;
use bevy_ecs_ldtk::prelude::*;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

use crate::player::Player;
use super::scene::SceneChangeRequest;

#[derive(SystemParam)]
pub struct LdtkEntityByNameQuery<'w, 's> {
    entities: Query<
        'w,
        's,
        (Entity, &'static EntityInstance, &'static Transform),
        Without<Player>,
    >,
    parents: Query<'w, 's, &'static ChildOf>,
    level_iids: Query<'w, 's, (Entity, &'static LevelIid), Without<Player>>,
}

impl<'w, 's> LdtkEntityByNameQuery<'w, 's> {
    fn level_ancestor_iid(&self, entity: Entity) -> Option<String> {
        let mut current = entity;
        for _ in 0..16 {
            let Ok(parent) = self.parents.get(current) else {
                break;
            };
            let parent_entity = parent.parent();
            if let Ok((_, iid)) = self.level_iids.get(parent_entity) {
                return Some(iid.as_str().to_string());
            }
            current = parent_entity;
        }
        None
    }

    fn is_in_active_level(&self, entity: Entity) -> bool {
        self.level_ancestor_iid(entity).is_some()
    }

    pub fn first_named(&self, name: &str) -> Option<(Entity, &EntityInstance, &Transform)> {
        self.entities
            .iter()
            .find(|(entity, instance, _)| {
                instance.identifier == name && self.is_in_active_level(*entity)
            })
    }

    pub fn iter_named(
        &self,
        name: &str,
    ) -> impl Iterator<Item = (Entity, &EntityInstance, &Transform)> + '_ {
        let target = name.to_string();
        self.entities
            .iter()
            .filter(move |(entity, instance, _)| {
                instance.identifier == target && self.is_in_active_level(*entity)
            })
    }

    pub fn iter_prefix(
        &self,
        prefix: &str,
    ) -> impl Iterator<Item = (Entity, &EntityInstance, &Transform)> + '_ {
        let target = prefix.to_string();
        self.entities
            .iter()
            .filter(move |(entity, instance, _)| {
                instance.identifier.starts_with(&target) && self.is_in_active_level(*entity)
            })
    }

    pub fn iter_prefix_in_level(
        &self,
        prefix: &str,
        level_iid: &str,
    ) -> impl Iterator<Item = (Entity, &EntityInstance, &Transform)> + '_ {
        let target = prefix.to_string();
        let level_target = level_iid.to_string();
        self.entities.iter().filter(move |(entity, instance, _)| {
            instance.identifier.starts_with(&target)
                && self
                    .level_ancestor_iid(*entity)
                    .map(|iid| iid == level_target)
                    .unwrap_or(false)
        })
    }
}

pub const TOWN_MAP_ASSET_PATH: &str = "maps/town.ldtk";
const ROAD_COLLISION_VALUE: i32 = 1;
const ROAD_COLLISION_LAYER: &str = "RoadCollision";
const DEBUG_PLAYER_START: bool = true;

pub struct MapPlugin;

impl Plugin for MapPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(LdtkPlugin);
        app.add_message::<SceneChangeRequest>();
        app.init_resource::<LoadedMap>();
        app.init_resource::<PendingPlayerStartScene>();
        app.init_resource::<PlayerStartApplyState>();
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

#[derive(Resource, Default, Clone)]
pub struct PendingPlayerStartScene {
    pub from_scene: Option<String>,
}

#[derive(Resource, Default, Clone)]
struct PlayerStartApplyState {
    last_applied_map: Option<String>,
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

pub fn scene_tag_from_map_asset_path(asset_path: &str) -> String {
    let raw = asset_path.trim();
    let leaf = raw.rsplit('/').next().unwrap_or(raw);
    leaf.strip_suffix(".ldtk")
        .unwrap_or(leaf)
        .to_ascii_lowercase()
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
    loaded_map: Res<LoadedMap>,
    mut players: Query<&mut Transform, With<Player>>,
    mut pending_scene: ResMut<PendingPlayerStartScene>,
    mut apply_state: ResMut<PlayerStartApplyState>,
) {
    let current_map = loaded_map_path(&loaded_map).to_string();
    let current_scene_tag = scene_tag_from_map_asset_path(&current_map);
    let has_pending_scene = pending_scene.from_scene.is_some();

    // During fade-out, pending `from_scene` is set before the map is swapped.
    // If we're still on that source scene, do not apply starts yet.
    if let Some(from_scene) = pending_scene.from_scene.as_deref() {
        let from_scene_tag = scene_tag_from_map_asset_path(from_scene);
        if current_scene_tag == from_scene_tag {
            return;
        }
    }

    let already_applied_for_map = apply_state
        .last_applied_map
        .as_deref()
        .map(|m| m == current_map.as_str())
        .unwrap_or(false);
    if already_applied_for_map && !has_pending_scene {
        return;
    }

    let mut default_start: Option<Vec2> = None;
    let mut scene_matched_start: Option<Vec2> = None;

    let desired_scene = pending_scene
        .from_scene
        .as_deref()
        .map(scene_tag_from_map_asset_path);

    let map_json = match read_map_json(&loaded_map) {
        Ok(v) => v,
        Err(_) => return,
    };
    let level = match map_json
        .get("levels")
        .and_then(Value::as_array)
        .and_then(|levels| levels.first())
    {
        Some(level) => level,
        None => return,
    };
    let current_level_iid = level
        .get("iid")
        .and_then(Value::as_str)
        .map(str::to_string);
    let level_px_hei = level
        .get("pxHei")
        .and_then(Value::as_i64)
        .map(|v| v as f32)
        .unwrap_or(0.0);

    if DEBUG_PLAYER_START {
        if let Some(ref iid) = current_level_iid {
            println!(
                "[PlayerStart] Resolving starts for map `{}` level iid `{}`",
                current_map, iid
            );
        } else {
            println!(
                "[PlayerStart] Could not read level iid from map `{}`; using active-level fallback",
                current_map
            );
        }
    }

    let mut saw_start = false;
    let Some(layer_instances) = level.get("layerInstances").and_then(Value::as_array) else {
        return;
    };
    for layer in layer_instances {
        let Some(entity_instances) = layer.get("entityInstances").and_then(Value::as_array) else {
            continue;
        };
        for instance in entity_instances {
            let Some(identifier) = instance.get("__identifier").and_then(Value::as_str) else {
                continue;
            };
            if !identifier.starts_with("PlayerStart") {
                continue;
            }
            saw_start = true;

            let Some(px) = instance.get("px").and_then(Value::as_array) else {
                continue;
            };
            let px_x = px.first().and_then(Value::as_i64).unwrap_or(0) as f32;
            let px_y = px.get(1).and_then(Value::as_i64).unwrap_or(0) as f32;
            let width = instance.get("width").and_then(Value::as_i64).unwrap_or(16) as f32;
            let height = instance.get("height").and_then(Value::as_i64).unwrap_or(16) as f32;
            let (pivot_x, pivot_y) = instance
                .get("__pivot")
                .and_then(Value::as_array)
                .map(|pivot| {
                    (
                        pivot.first().and_then(Value::as_f64).unwrap_or(0.5) as f32,
                        pivot.get(1).and_then(Value::as_f64).unwrap_or(0.5) as f32,
                    )
                })
                .unwrap_or((0.5, 0.5));
            // Match bevy_ecs_ldtk::utils::ldtk_pixel_coords_to_translation_pivoted.
            let pos = Vec2::new(
                px_x + (width * (0.5 - pivot_x)),
                (level_px_hei - px_y) + (height * (pivot_y - 0.5)),
            );

            if identifier == "PlayerStart" {
                if default_start.is_none() {
                    default_start = Some(pos);
                    if DEBUG_PLAYER_START {
                        println!(
                            "[PlayerStart] Found default PlayerStart in `{}` at ({:.1}, {:.1})",
                            current_map, pos.x, pos.y
                        );
                    }
                }
                continue;
            }

            let scene_field = instance
                .get("fieldInstances")
                .and_then(Value::as_array)
                .and_then(|fields| {
                    fields.iter().find_map(|f| {
                        let is_scene = f
                            .get("__identifier")
                            .and_then(Value::as_str)
                            .map(|s| s == "scene")
                            .unwrap_or(false);
                        if !is_scene {
                            return None;
                        }
                        f.get("__value")
                            .and_then(Value::as_str)
                            .map(|s| s.trim().to_string())
                    })
                });

            let Some(scene_field) = scene_field else {
                eprintln!(
                    "PlayerStart variant `{}` missing required `scene` field; ignored.",
                    identifier
                );
                continue;
            };

            if let Some(ref desired) = desired_scene {
                let candidate_scene = scene_tag_from_map_asset_path(&scene_field);
                if candidate_scene == *desired && scene_matched_start.is_none() {
                    scene_matched_start = Some(pos);
                    if DEBUG_PLAYER_START {
                        println!(
                            "[PlayerStart] Matched scene-tagged start `{}` for scene `{}` at ({:.1}, {:.1})",
                            identifier, candidate_scene, pos.x, pos.y
                        );
                    }
                }
            }
        }
    }

    if !saw_start {
        if DEBUG_PLAYER_START {
            eprintln!(
                "[PlayerStart] No PlayerStart entities found for map `{}`",
                current_map
            );
        }
        apply_state.last_applied_map = Some(current_map);
        return;
    }

    let Ok(mut player_transform) = players.single_mut() else {
        return;
    };

    let used_scene_match = scene_matched_start.is_some();
    let chosen = scene_matched_start.or(default_start);
    let Some(chosen_pos) = chosen else {
        return;
    };

    if DEBUG_PLAYER_START {
        if used_scene_match {
            println!(
                "[PlayerStart] Applying scene-matched spawn in `{}` at ({:.1}, {:.1})",
                current_map, chosen_pos.x, chosen_pos.y
            );
        } else {
            println!(
                "[PlayerStart] Falling back to default PlayerStart in `{}` at ({:.1}, {:.1})",
                current_map, chosen_pos.x, chosen_pos.y
            );
        }
    }

    player_transform.translation.x = chosen_pos.x;
    player_transform.translation.y = chosen_pos.y;
    pending_scene.from_scene = None;
    apply_state.last_applied_map = Some(current_map);
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
