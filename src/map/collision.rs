use bevy::prelude::*;
use serde_json::Value;
use std::collections::HashSet;

use crate::collision::BoundingBox;

use super::ldtk::{
    ldtk_first_level, ldtk_layer_instances, ldtk_level_dimensions, loaded_map_path, read_map_json,
    LoadedMap,
};

const MAZE_SCENE_KEY: &str = "maze";
const MAP_GRID_TILE_SIZE: f32 = 16.0;
const MAP_GRID_ORIGIN_X: f32 = 0.0;
const MAP_GRID_ORIGIN_Y: f32 = 0.0;
const MAP_COLLISION_Z: f32 = 2.0;
const MAP_COLLISION_LAYER_PHRASE: &str = "bounding";

#[derive(Resource, Default)]
pub struct MapCollisionState {
    pub(crate) last_asset_path: String,
}

#[derive(Component)]
pub struct MapOutlineCollision;

#[derive(Component)]
pub struct MapLayerCollision;

pub fn collision_box_outline(
    loaded_map: &LoadedMap,
    tile_size: f32,
) -> Result<Vec<(IVec2, BoundingBox)>, String> {
    let (width, height) = current_map_dimensions(loaded_map)?;
    let mut boxes = Vec::new();

    for cell in outer_border_cells(width as i32, height as i32) {
        boxes.push((
            cell,
            BoundingBox {
                width: tile_size,
                height: tile_size,
            },
        ));
    }

    Ok(boxes)
}

fn current_map_dimensions(loaded_map: &LoadedMap) -> Result<(usize, usize), String> {
    let json = read_map_json(loaded_map)?;
    let first_level = ldtk_first_level(&json)?;
    let layers = ldtk_layer_instances(first_level)?;
    ldtk_level_dimensions(first_level, layers)
}

fn outer_border_cells(width: i32, height: i32) -> Vec<IVec2> {
    let mut cells = Vec::new();
    if width <= 0 || height <= 0 {
        return cells;
    }

    for x in -1..=width {
        cells.push(IVec2::new(x, -1));
        cells.push(IVec2::new(x, height));
    }
    for y in 0..height {
        cells.push(IVec2::new(-1, y));
        cells.push(IVec2::new(width, y));
    }

    cells
}

pub fn spawn_collision_box_outline(
    commands: &mut Commands,
    loaded_map: &LoadedMap,
    tile_size: f32,
    world_origin: Vec2,
    z: f32,
) -> Result<usize, String> {
    let cells = collision_box_outline(loaded_map, tile_size)?;
    let mut spawned = 0usize;

    for (cell, bounds) in cells {
        let x = world_origin.x + (cell.x as f32 * tile_size) + (tile_size * 0.5);
        let y = world_origin.y + (cell.y as f32 * tile_size) + (tile_size * 0.5);
        commands.spawn((MapOutlineCollision, bounds, Transform::from_xyz(x, y, z)));
        spawned += 1;
    }

    Ok(spawned)
}

pub fn spawn_collision_boxes_for_layers_containing(
    commands: &mut Commands,
    loaded_map: &LoadedMap,
    phrase: &str,
    tile_size: f32,
    world_origin: Vec2,
    z: f32,
) -> Result<usize, String> {
    let json = read_map_json(loaded_map)?;
    let level = ldtk_first_level(&json)?;
    let layers = ldtk_layer_instances(level)?;
    let (_, level_height) = ldtk_level_dimensions(level, layers)?;

    let needle = phrase.to_ascii_lowercase();
    let mut cells = HashSet::<IVec2>::new();

    for layer in layers {
        let name = layer
            .get("__identifier")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !name.to_ascii_lowercase().contains(&needle) {
            continue;
        }

        let grid_size = layer
            .get("__gridSize")
            .and_then(Value::as_i64)
            .unwrap_or(16)
            .max(1) as i32;

        collect_cells_from_tile_array(layer.get("gridTiles"), grid_size, &mut cells);
        collect_cells_from_tile_array(layer.get("autoLayerTiles"), grid_size, &mut cells);
        collect_cells_from_intgrid(layer, &mut cells);
    }

    let mut spawned = 0usize;
    for cell in cells {
        let world_cell_y = (level_height as i32 - 1) - cell.y;
        let x = world_origin.x + (cell.x as f32 * tile_size) + (tile_size * 0.5);
        let y = world_origin.y + (world_cell_y as f32 * tile_size) + (tile_size * 0.5);
        commands.spawn((
            MapLayerCollision,
            BoundingBox {
                width: tile_size,
                height: tile_size,
            },
            Transform::from_xyz(x, y, z),
        ));
        spawned += 1;
    }

    Ok(spawned)
}

fn collect_cells_from_intgrid(layer: &Value, cells: &mut HashSet<IVec2>) {
    let Some(csv) = layer.get("intGridCsv").and_then(Value::as_array) else {
        return;
    };
    let Some(c_wid) = layer.get("__cWid").and_then(Value::as_u64) else {
        return;
    };
    let c_wid = c_wid as usize;
    for (idx, cell) in csv.iter().enumerate() {
        if cell.as_i64().unwrap_or(0) <= 0 {
            continue;
        }
        let x = (idx % c_wid) as i32;
        let y = (idx / c_wid) as i32;
        cells.insert(IVec2::new(x, y));
    }
}

fn collect_cells_from_tile_array(
    tile_array: Option<&Value>,
    grid_size: i32,
    cells: &mut HashSet<IVec2>,
) {
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
        cells.insert(IVec2::new(x as i32, y as i32));
    }
}

pub fn sync_map_outline_collision(
    mut commands: Commands,
    loaded_map: Res<LoadedMap>,
    mut map_collision_state: ResMut<MapCollisionState>,
    existing_outline: Query<Entity, With<MapOutlineCollision>>,
    existing_layer_collision: Query<Entity, With<MapLayerCollision>>,
) {
    let current = loaded_map_path(&loaded_map);
    if current == MAZE_SCENE_KEY {
        for e in &existing_outline {
            commands.entity(e).try_despawn();
        }
        for e in &existing_layer_collision {
            commands.entity(e).try_despawn();
        }
        map_collision_state.last_asset_path.clear();
        return;
    }

    if map_collision_state.last_asset_path == current {
        return;
    }
    for e in &existing_outline {
        commands.entity(e).try_despawn();
    }
    for e in &existing_layer_collision {
        commands.entity(e).try_despawn();
    }
    let _ = spawn_collision_box_outline(
        &mut commands,
        &loaded_map,
        MAP_GRID_TILE_SIZE,
        Vec2::new(MAP_GRID_ORIGIN_X, MAP_GRID_ORIGIN_Y),
        MAP_COLLISION_Z,
    );
    let _ = spawn_collision_boxes_for_layers_containing(
        &mut commands,
        &loaded_map,
        MAP_COLLISION_LAYER_PHRASE,
        MAP_GRID_TILE_SIZE,
        Vec2::new(MAP_GRID_ORIGIN_X, MAP_GRID_ORIGIN_Y),
        MAP_COLLISION_Z,
    );
    map_collision_state.last_asset_path = current.to_string();
}
