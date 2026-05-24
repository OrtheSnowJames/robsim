use crate::bank::light::{self, DEFAULT_MAX_DISTANCE_TILES};
use crate::bank::guard::MazeGuard;
use crate::bank::{Grid, GridType};
use crate::collision::BoundingBox;
use crate::map::TransferPortal;
use crate::player::Player;
use crate::PlayerMoney;
use bevy::prelude::*;
use rand::RngExt;

pub const TILE_SIZE: f32 = 16.0;
const MAZE_Z_LAYER: f32 = 1.0;
const MIN_LIGHT: f32 = 0.0;
const COIN_MIN_VALUE: i32 = 1;
const COIN_MAX_VALUE: i32 = 25;
const HIDE_IMAGE_PATH: &str = "hide.png";
const FLOOR_COLOR: Color = Color::srgb(0.86, 0.84, 0.78);

#[derive(Component)]
pub struct MazeTile;

#[derive(Component)]
pub struct MazeTileCell {
    x: usize,
    y: usize,
    tile: u8,
}

#[derive(Component)]
pub struct CoinTileCell {
    x: usize,
    y: usize,
}

#[derive(Component)]
pub struct HideTileCell {
    x: usize,
    y: usize,
}

#[derive(Component, Clone, Copy)]
pub struct CoinValue(pub i32);

#[derive(Resource, Clone)]
pub struct MazeRenderState {
    pub grid: Grid,
    pub world_center: Vec2,
}

pub fn clear_maze(commands: &mut Commands, maze_tiles: &Query<Entity, With<MazeTile>>) {
    for entity in maze_tiles {
        commands.entity(entity).try_despawn();
    }
    commands.remove_resource::<MazeRenderState>();
}

pub fn render_maze(
    grid: &Grid,
    world_center: Vec2,
    commands: &mut Commands,
    asset_server: &AssetServer,
) {
    if grid.is_empty() || grid[0].is_empty() {
        return;
    }

    commands.insert_resource(MazeRenderState {
        grid: grid.clone(),
        world_center,
    });

    let tile_img: Handle<Image> = asset_server.load("bank/bank_tile.png");
    let coin_img: Handle<Image> = asset_server.load("bank/coin.png");
    let mut rng = rand::rng();
    let height = grid.len();
    let width = grid[0].len();

    let world_width = width as f32 * TILE_SIZE;
    let world_height = height as f32 * TILE_SIZE;
    let left = world_center.x - (world_width * 0.5) + (TILE_SIZE * 0.5);
    let top = world_center.y + (world_height * 0.5) - (TILE_SIZE * 0.5);

    for (y, row) in grid.iter().enumerate() {
        for (x, &tile) in row.iter().enumerate() {
            let base_tile = if tile == GridType::COIN as u8 {
                GridType::FLOOR as u8
            } else {
                tile
            };
            let base_color = tile_color(base_tile);
            let world_x = left + (x as f32 * TILE_SIZE);
            let world_y = top - (y as f32 * TILE_SIZE);
            let used_img = if tile == GridType::SHAFT as u8 {
                asset_server.load("bank/bank_tile_shaft.png")
            } else {
                tile_img.clone()
            };

            let mut entity = commands.spawn((
                MazeTile,
                MazeTileCell { x, y, tile: base_tile },
                Sprite {
                    image: used_img.clone(),
                    color: apply_light(base_color, MIN_LIGHT),
                    custom_size: Some(Vec2::splat(TILE_SIZE)),
                    ..default()
                },
                Transform::from_xyz(world_x, world_y, MAZE_Z_LAYER),
            ));

            if tile == GridType::WALL as u8 {
                entity.insert(BoundingBox {
                    width: TILE_SIZE,
                    height: TILE_SIZE,
                });
            }

            if tile == GridType::COIN as u8 {
                let value = rng.random_range(COIN_MIN_VALUE..=COIN_MAX_VALUE);
                entity.with_children(|parent| {
                    parent.spawn((
                        MazeTile,
                        CoinTileCell { x, y },
                        CoinValue(value),
                        Sprite {
                            image: coin_img.clone(),
                            color: apply_light(tile_color(GridType::COIN as u8), MIN_LIGHT),
                            custom_size: Some(Vec2::splat(TILE_SIZE)),
                            ..default()
                        },
                        Transform::from_xyz(0.0, 0.0, crate::player::PLAYER_Z_LAYER + 1.0),
                    ));
                });
            }
            else if tile == GridType::HIDE as u8 {
                let hide_img: Handle<Image> = asset_server.load(HIDE_IMAGE_PATH);
                entity.with_children(|parent| {
                    parent.spawn((
                        MazeTile,
                        HideTileCell { x, y },
                        Sprite {
                            image: hide_img.clone(),
                            color: Color::BLACK,
                            custom_size: Some(Vec2::splat(TILE_SIZE)),
                            ..default()
                        },
                        Transform::from_xyz(0.0, 0.0, 0.1),
                    ));
                });
            } else if tile == GridType::SHAFT as u8 {
               entity.with_children(|parent| {
                    parent.spawn((
                        TransferPortal {
                            scene: "soup_store".to_string(),
                            width: 16.,
                            height: 16.,
                        },
                        Transform::default(),
                        GlobalTransform::default(),
                    ));
               }); 
            }
        }
    }
}

pub fn grid_cell_world_position(
    width: usize,
    height: usize,
    world_center: Vec2,
    cell_x: usize,
    cell_y: usize,
) -> Vec2 {
    let world_width = width as f32 * TILE_SIZE;
    let world_height = height as f32 * TILE_SIZE;
    let left = world_center.x - (world_width * 0.5) + (TILE_SIZE * 0.5);
    let top = world_center.y + (world_height * 0.5) - (TILE_SIZE * 0.5);

    Vec2::new(
        left + (cell_x as f32 * TILE_SIZE),
        top - (cell_y as f32 * TILE_SIZE),
    )
}

pub fn world_to_grid_cell(
    width: usize,
    height: usize,
    world_center: Vec2,
    world: Vec2,
) -> Option<IVec2> {
    if width == 0 || height == 0 {
        return None;
    }

    let world_width = width as f32 * TILE_SIZE;
    let world_height = height as f32 * TILE_SIZE;
    let left = world_center.x - (world_width * 0.5);
    let top = world_center.y + (world_height * 0.5);

    let gx = ((world.x - left) / TILE_SIZE).floor() as i32;
    let gy = ((top - world.y) / TILE_SIZE).floor() as i32;

    if gx < 0 || gy < 0 || gx >= width as i32 || gy >= height as i32 {
        return None;
    }

    Some(IVec2::new(gx, gy))
}

pub fn update_maze_lighting(
    maze_state: Option<Res<MazeRenderState>>,
    player_query: Query<&Transform, With<Player>>,
    mut maze_tiles: Query<
        (&MazeTileCell, &mut Sprite),
        (
            With<MazeTile>,
            Without<CoinTileCell>,
            Without<HideTileCell>,
            Without<MazeGuard>,
        ),
    >,
    mut coin_tiles: Query<
        (&CoinTileCell, &mut Sprite),
        (
            With<MazeTile>,
            Without<MazeTileCell>,
            Without<HideTileCell>,
            Without<MazeGuard>,
        ),
    >,
    mut hide_tiles: Query<
        (&HideTileCell, &mut Sprite),
        (
            With<MazeTile>,
            Without<CoinTileCell>,
            Without<MazeTileCell>,
            Without<MazeGuard>,
        ),
    >,
    mut guards: Query<(&Transform, &mut Sprite), With<MazeGuard>>,
) {
    let Some(maze_state) = maze_state else {
        return;
    };

    let Ok(player_transform) = player_query.single() else {
        return;
    };

    let visibility = light::compute_visibility_from_world(
        &maze_state.grid,
        player_transform.translation.truncate(),
        maze_state.world_center,
        TILE_SIZE,
        DEFAULT_MAX_DISTANCE_TILES,
    );

    if visibility.is_empty() {
        return;
    }

    for (cell, mut sprite) in &mut maze_tiles {
        let light_value = visibility
            .get(cell.y)
            .and_then(|row| row.get(cell.x))
            .copied()
            .unwrap_or(0.0);

        sprite.color = apply_light(tile_color(cell.tile), light_value);
    }

    for (cell, mut sprite) in &mut coin_tiles {
        let light_value = visibility
            .get(cell.y)
            .and_then(|row| row.get(cell.x))
            .copied()
            .unwrap_or(0.0);
        sprite.color = apply_light(tile_color(GridType::COIN as u8), light_value);
    }

    for (cell, mut sprite) in &mut hide_tiles {
        let light_value = visibility
            .get(cell.y)
            .and_then(|row| row.get(cell.x))
            .copied()
            .unwrap_or(0.0);
        sprite.color = apply_light(Color::WHITE, light_value);
    }

    for (guard_tf, mut sprite) in &mut guards {
        let visible = world_to_grid_cell(
            maze_state.grid[0].len(),
            maze_state.grid.len(),
            maze_state.world_center,
            guard_tf.translation.truncate(),
        )
        .and_then(|cell| {
            visibility
                .get(cell.y as usize)
                .and_then(|row| row.get(cell.x as usize))
                .copied()
        })
        .unwrap_or(0.0);

        let a = if visible > 0.0 { 1.0 } else { 0.0 };
        let s = sprite.color.to_srgba();
        sprite.color = Color::srgba(s.red, s.green, s.blue, a);
    }
}

pub fn collect_coins(
    mut commands: Commands,
    mut money: ResMut<PlayerMoney>,
    player_query: Query<&Transform, With<Player>>,
    coin_query: Query<(Entity, &GlobalTransform, &CoinValue), With<CoinTileCell>>,
) {
    let Ok(player_transform) = player_query.single() else {
        return;
    };
    let player_pos = player_transform.translation.truncate();
    let pickup_radius = 8.0;

    for (coin_entity, coin_tf, coin_value) in &coin_query {
        let coin_pos = coin_tf.translation().truncate();
        if player_pos.distance(coin_pos) <= pickup_radius {
            money.amount += coin_value.0;
            commands.entity(coin_entity).despawn();
        }
    }
}

fn apply_light(base: Color, light_value: f32) -> Color {
    let srgb = base.to_srgba();
    Color::srgba(
        (srgb.red * light_value).clamp(0.0, 1.0),
        (srgb.green * light_value).clamp(0.0, 1.0),
        (srgb.blue * light_value).clamp(0.0, 1.0),
        1.0,
    )
}

fn tile_color(tile: u8) -> Color {
    if tile == GridType::WALL as u8 {
        Color::srgb(0.22, 0.22, 0.24)
    } else if tile == GridType::FLOOR as u8 {
        FLOOR_COLOR
    } else if tile == GridType::PLAYER as u8 {
        Color::srgb(0.40, 0.75, 1.0)
    } else if tile == GridType::CHEST as u8 {
        Color::srgb(0.95, 0.75, 0.16)
    } else if tile == GridType::ENTRANCE as u8 {
        Color::srgb(0.25, 0.9, 0.35)
    } else if tile == GridType::EXIT as u8 {
        Color::srgb(0.95, 0.3, 0.35)
    } else if tile == GridType::HIDE as u8 {
        // Color::srgb(0.22, 0.45, 0.65) // we got an image for hide
        FLOOR_COLOR
    } else if tile == GridType::COIN as u8 {
        Color::srgb(1.0, 0.84, 0.0)
    } else if tile == GridType::SHAFT as u8 {
        FLOOR_COLOR
    } else {
        Color::srgb(0.15, 0.15, 0.15)
    }
}

pub fn outline(grid: &Vec<Vec<u8>>) -> Vec<Vec<u8>> {
    let h = grid.len();

    if h == 0 {
        return vec![];
    }

    let w = grid[0].len();

    let mut out = vec![vec![0u8; w]; h];

    for y in 0..h {
        for x in 0..w {
            if grid[y][x] != 1 {
                continue;
            }

            // all 8 directions
            for dy in -1isize..=1 {
                for dx in -1isize..=1 {
                    let nx = x as isize + dx;
                    let ny = y as isize + dy;

                    if nx >= 0 && ny >= 0 && nx < w as isize && ny < h as isize {
                        out[ny as usize][nx as usize] = 1;
                    }
                }
            }
        }
    }

    // remove original shape
    for y in 0..h {
        for x in 0..w {
            if grid[y][x] == 1 {
                out[y][x] = 0;
            }
        }
    }

    out
}
