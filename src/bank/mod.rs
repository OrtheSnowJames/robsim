pub mod img_layer;
pub mod light;
pub mod generation;
pub mod render;
pub mod guard;
pub mod teller;

use bevy::prelude::*;

use self::{guard::GuardPlugin, render::radar::RadarPlugin};

pub struct BankPlugin;

impl Plugin for BankPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((GuardPlugin, RadarPlugin));
    }
}

#[derive(Component)]
pub struct BankBuilding;

#[derive(Component)]
pub struct TavernBuilding;

pub enum GridType {
    WALL = 0,
    FLOOR = 1,
    PLAYER = 2,
    CHEST = 3,
    ENTRANCE = 4,
    EXIT = 5,
    HIDE = 6,
    COIN = 7,
    SHAFT = 8
}

pub type Grid = Vec<Vec<u8>>;
pub type LightGrid = Vec<Vec<f32>>;

fn get_grid_at(grid: &Grid, x: isize, y: isize) -> Option<u8> {
    if x < 0 || y < 0 {
        return None;
    }

    let x = x as usize;
    let y = y as usize;

    grid.get(y)
        .and_then(|row| row.get(x))
        .copied()
}

fn set_grid_at(grid: &mut Grid, at: Vec2, value: u8) -> bool {
    let x = at.x as isize;
    let y = at.y as isize;

    if x < 0 || y < 0 {
        return false;
    }

    let x = x as usize;
    let y = y as usize;

    if let Some(row) = grid.get_mut(y) {
        if let Some(cell) = row.get_mut(x) {
            *cell = value;
            return true;
        }
    }

    false
}
