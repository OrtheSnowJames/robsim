use bevy::prelude::Vec2;
use std::f32::consts::PI;

use super::{Grid, GridType, LightGrid};

const RAYS: usize = 720;
const STEP_SIZE: f32 = 0.1;
pub const DEFAULT_MAX_DISTANCE_TILES: f32 = 7.5;

pub fn compute_visibility(grid: &Grid, max_distance: f32) -> LightGrid {
    if grid.is_empty() || grid[0].is_empty() {
        return Vec::new();
    }

    let height = grid.len();
    let width = grid[0].len();

    let mut player_pos = None;
    for (y, row) in grid.iter().enumerate() {
        for (x, &tile) in row.iter().enumerate() {
            if tile == GridType::PLAYER as u8 {
                player_pos = Some((x as f32 + 0.5, y as f32 + 0.5));
                break;
            }
        }
        if player_pos.is_some() {
            break;
        }
    }

    let Some((player_x, player_y)) = player_pos else {
        return vec![vec![0.0; width]; height];
    };

    compute_visibility_from_grid_pos(grid, player_x, player_y, max_distance)
}

pub fn compute_visibility_from_world(
    grid: &Grid,
    player_world: Vec2,
    world_center: Vec2,
    tile_size: f32,
    max_distance_tiles: f32,
) -> LightGrid {
    if grid.is_empty() || grid[0].is_empty() {
        return Vec::new();
    }

    let height = grid.len();
    let width = grid[0].len();
    let world_width = width as f32 * tile_size;
    let world_height = height as f32 * tile_size;

    let left = world_center.x - (world_width * 0.5);
    let top = world_center.y + (world_height * 0.5);

    let player_x = (player_world.x - left) / tile_size;
    let player_y = (top - player_world.y) / tile_size;

    compute_visibility_from_grid_pos(grid, player_x, player_y, max_distance_tiles)
}

fn compute_visibility_from_grid_pos(
    grid: &Grid,
    player_x: f32,
    player_y: f32,
    max_distance: f32,
) -> LightGrid {
    let height = grid.len();
    let width = grid[0].len();

    let mut brightness = vec![vec![0.0; width]; height];

    if player_x < 0.0 || player_y < 0.0 || player_x >= width as f32 || player_y >= height as f32 {
        return brightness;
    }

    for i in 0..RAYS {
        let angle = (i as f32 / RAYS as f32) * 2.0 * PI;

        let dx = angle.cos();
        let dy = angle.sin();

        let mut x = player_x;
        let mut y = player_y;

        for _ in 0..(max_distance / STEP_SIZE) as usize {
            x += dx * STEP_SIZE;
            y += dy * STEP_SIZE;

            let gx = x.floor() as isize;
            let gy = y.floor() as isize;

            if gx < 0 || gy < 0 || gx >= width as isize || gy >= height as isize {
                break;
            }

            let gx = gx as usize;
            let gy = gy as usize;

            let dist_x = x - player_x;
            let dist_y = y - player_y;
            let distance = (dist_x * dist_x + dist_y * dist_y).sqrt();

            if distance > max_distance {
                break;
            }

            let light = 1.0 - (distance / max_distance);
            if light > brightness[gy][gx] {
                brightness[gy][gx] = light;
            }

            if grid[gy][gx] == GridType::WALL as u8 {
                break;
            }
        }
    }

    brightness
}
