use bevy::{
    asset::RenderAssetUsages,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages},
};

use crate::bank::guard::MazeGuard;
use crate::bank::render::maze::{world_to_grid_cell, MazeRenderState};
use crate::bank::GridType;
use crate::player::Player;

const RADAR_DIAMETER_TILES: i32 = 21;
const RADAR_TILE_PIXELS: i32 = 4;
const RADAR_BG_ALPHA: u8 = 0;
const RADAR_GREEN_R: u8 = 55;
const RADAR_GREEN_G: u8 = 220;
const RADAR_GREEN_B: u8 = 90;
const RADAR_WALL_ALPHA: u8 = 55;
const RADAR_GUARD_ALPHA: u8 = 185;
const RADAR_PLAYER_ALPHA: u8 = 255;

pub struct RadarPlugin;

impl Plugin for RadarPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GameGrid>()
            .add_systems(Startup, init_radar_ui)
            .add_systems(Update, update_radar_canvas);
    }
}

#[derive(Resource)]
pub struct GameGrid {
    // Kept as a shared radar data buffer resource.
    pub tiles: Vec<Vec<u8>>,
    pub tile_size: i32,
}

impl Default for GameGrid {
    fn default() -> Self {
        Self {
            tiles: Vec::new(),
            tile_size: RADAR_TILE_PIXELS,
        }
    }
}

#[derive(Resource)]
pub struct RadarCanvas {
    pub handle: Handle<Image>,
    pub width: u32,
    pub height: u32,
}

#[derive(Component)]
struct RadarNode;

fn init_radar_ui(mut commands: Commands, mut images: ResMut<Assets<Image>>, grid: Res<GameGrid>) {
    let width = (RADAR_DIAMETER_TILES * grid.tile_size) as u32;
    let height = (RADAR_DIAMETER_TILES * grid.tile_size) as u32;
    let mut image = Image::new_fill(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0, 0, 0, 0],
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::default(),
    );
    image.texture_descriptor.label = Some("custom_radar_canvas".into());
    image.texture_descriptor.usage = TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST;

    let handle = images.add(image);
    commands.insert_resource(RadarCanvas { handle: handle.clone(), width, height });

    // Spawn HUD layout anchoring the radar screen cleanly in the top-right corner
    commands.spawn(Node {
        position_type: PositionType::Absolute,
        top: Val::Px(20.0),
        right: Val::Px(20.0),
        padding: UiRect::all(Val::Px(6.0)),
        border: UiRect::all(Val::Px(1.0)),
        ..default()
    })
    .insert(BorderColor::all(Color::srgba(0.35, 1.0, 0.35, 0.55)))
    .insert(BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.8)))
    .insert(RadarNode)
    .insert(Visibility::Hidden)
    .with_children(|parent| {
        parent.spawn((
            Node {
                width: Val::Px(width as f32),
                height: Val::Px(height as f32),
                ..default()
            },
            ImageNode::new(handle),
        ));
    });
}

fn update_radar_canvas(
    mut grid: ResMut<GameGrid>,
    radar: Res<RadarCanvas>,
    maze_state: Option<Res<MazeRenderState>>,
    player_query: Query<&Transform, With<Player>>,
    guard_query: Query<&Transform, With<MazeGuard>>,
    mut radar_ui_visibility: Query<&mut Visibility, With<RadarNode>>,
    mut images: ResMut<Assets<Image>>,
) {
    let Some(image) = images.get_mut(&radar.handle) else { return };
    let canvas_w = radar.width as i32;
    let canvas_h = radar.height as i32;

    let Some(data) = image.data.as_mut() else {
        return;
    };

    let mut set_ui_visible = |visible: bool| {
        if let Ok(mut v) = radar_ui_visibility.single_mut() {
            *v = if visible {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }
    };

    // Fill with black first.
    for px in data.chunks_exact_mut(4) {
        px[0] = 0;
        px[1] = 0;
        px[2] = 0;
        px[3] = RADAR_BG_ALPHA;
    }

    let (Some(maze_state), Ok(player_tf)) = (maze_state, player_query.single()) else {
        set_ui_visible(false);
        return;
    };
    set_ui_visible(true);

    if maze_state.grid.is_empty() || maze_state.grid[0].is_empty() {
        return;
    }

    grid.tiles = maze_state.grid.clone();
    let map_h = grid.tiles.len();
    let map_w = grid.tiles[0].len();

    let Some(player_cell) = world_to_grid_cell(
        map_w,
        map_h,
        maze_state.world_center,
        player_tf.translation.truncate(),
    ) else {
        return;
    };

    let mut guard_cells = std::collections::HashSet::new();
    for guard_tf in &guard_query {
        if let Some(cell) = world_to_grid_cell(
            map_w,
            map_h,
            maze_state.world_center,
            guard_tf.translation.truncate(),
        ) {
            guard_cells.insert(cell);
        }
    }

    let tile_px = grid.tile_size.max(1);
    let half_tiles = RADAR_DIAMETER_TILES / 2;
    let center_px_x = canvas_w / 2;
    let center_px_y = canvas_h / 2;
    let radar_radius = (canvas_w.min(canvas_h) / 2) - 2;
    let radar_radius_sq = radar_radius * radar_radius;

    let mut draw_pixel = |x: i32, y: i32, r: u8, g: u8, b: u8, a: u8| {
        if x < 0 || y < 0 || x >= canvas_w || y >= canvas_h {
            return;
        }
        let idx = ((y * canvas_w + x) * 4) as usize;
        if idx + 3 < data.len() {
            data[idx] = r;
            data[idx + 1] = g;
            data[idx + 2] = b;
            data[idx + 3] = a;
        }
    };

    for ry in 0..canvas_h {
        for rx in 0..canvas_w {
            let dx = rx - center_px_x;
            let dy = ry - center_px_y;
            if dx * dx + dy * dy > radar_radius_sq {
                draw_pixel(rx, ry, 0, 0, 0, 0);
            }
        }
    }

    for dy_tile in -half_tiles..=half_tiles {
        for dx_tile in -half_tiles..=half_tiles {
            let map_x = player_cell.x + dx_tile;
            let map_y = player_cell.y + dy_tile;
            if map_x < 0 || map_y < 0 || map_x >= map_w as i32 || map_y >= map_h as i32 {
                continue;
            }

            let tile_center_x = center_px_x + dx_tile * tile_px + tile_px / 2;
            let tile_center_y = center_px_y + dy_tile * tile_px + tile_px / 2;
            let cdx = tile_center_x - center_px_x;
            let cdy = tile_center_y - center_px_y;
            if cdx * cdx + cdy * cdy > radar_radius_sq {
                continue;
            }

            let tile = grid.tiles[map_y as usize][map_x as usize];
            let mut alpha = 0_u8;
            let mut color = (RADAR_GREEN_R, RADAR_GREEN_G, RADAR_GREEN_B);
            if tile == GridType::WALL as u8 {
                alpha = RADAR_WALL_ALPHA;
            }
            if tile == GridType::SHAFT as u8 {
                alpha = RADAR_PLAYER_ALPHA;
                color = (255, 255, 255);
            }
            if guard_cells.contains(&IVec2::new(map_x, map_y)) {
                alpha = alpha.max(RADAR_GUARD_ALPHA);
                color = (RADAR_GREEN_R, RADAR_GREEN_G, RADAR_GREEN_B);
            }
            if map_x == player_cell.x && map_y == player_cell.y {
                alpha = RADAR_PLAYER_ALPHA;
                color = (RADAR_GREEN_R, RADAR_GREEN_G, RADAR_GREEN_B);
            }
            if alpha == 0 {
                continue;
            }

            let blip_size = (tile_px / 2).max(1);
            for py in (tile_center_y - blip_size)..=(tile_center_y + blip_size) {
                for px in (tile_center_x - blip_size)..=(tile_center_x + blip_size) {
                    let rdx = px - center_px_x;
                    let rdy = py - center_px_y;
                    if rdx * rdx + rdy * rdy <= radar_radius_sq {
                        draw_pixel(px, py, color.0, color.1, color.2, alpha);
                    }
                }
            }
        }
    }
}
