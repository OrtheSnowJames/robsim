use bevy::prelude::*;
use rand::RngExt;
use serde_json::Value;
use std::fs;
use std::path::Path;

use crate::bank::render::maze::{grid_cell_world_position, world_to_grid_cell, MazeRenderState, TILE_SIZE};
use crate::bank::{Grid, GridType};
use crate::entity_dialogue::PlayerMovementLock;
use crate::player::Player;
use crate::sprite_sheet::{
    apply_animator_to_sprite, tick_animator, Facing, FacingColumns, SpriteSheetAnimator,
    SpriteSheetConfig,
};

const GUARD_SPEED: f32 = 50.0;
const GUARD_REPATH_SECS: f32 = 0.35;
const GUARD_TOUCH_DISTANCE: f32 = 10.0;
const GUARD_SIGHT_RADIUS_TILES: f32 = 7.0;
const CHASE_SPEED_MULTIPLIER: f32 = 1.5;
const PATROL_RETARGET_DISTANCE: f32 = 4.0;
const PATH_DEBUG_Z: f32 = 13.0;
const SHOW_GUARD_PATH_DEBUG: bool = false;
// Toggle for testing: set false to disable all guard spawning/behavior.
const ENABLE_GUARDS: bool = false;
const GUARD_SPRITE_PATH: &str = "guard.png";
const EXCLAMATION_PATH: &str = "exclamation.png";
const GUARD_CAPTURE_EXCLAIM_SECS: f32 = 0.35;
const GUARD_CAPTURE_DIALOGUE: &str = "Hey!\nCaught you.\nPress ENTER.";

pub struct GuardPlugin;

impl Plugin for GuardPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GuardAlertState>()
            .init_resource::<GuardCaptureSequence>()
            .add_systems(Startup, setup_guard_capture_ui)
            .add_systems(Update, run_guard_capture_sequence);
    }
}

#[derive(Component)]
pub struct MazeGuard;

#[derive(Resource, Default)]
pub struct GuardAlertState {
    pub caught_player: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum GuardCapturePhase {
    Idle,
    Exclaim,
    Dialogue,
}

#[derive(Resource)]
pub struct GuardCaptureSequence {
    phase: GuardCapturePhase,
    timer: Timer,
    exclamation_entity: Option<Entity>,
    dialogue_line: String,
}

impl Default for GuardCaptureSequence {
    fn default() -> Self {
        let mut timer = Timer::from_seconds(0.0, TimerMode::Once);
        timer.finish();
        Self {
            phase: GuardCapturePhase::Idle,
            timer,
            exclamation_entity: None,
            dialogue_line: GUARD_CAPTURE_DIALOGUE.to_string(),
        }
    }
}

#[derive(Component)]
struct GuardExclamation;

#[derive(Component)]
struct GuardCaptureUiRoot;

#[derive(Component)]
struct GuardCaptureUiText;

#[derive(Clone, Copy, PartialEq, Eq)]
enum GuardMode {
    Patrol,
    Chase,
}

#[derive(Component)]
pub struct GuardBrain {
    corner: usize,
    mode: GuardMode,
    current_cell: IVec2,
    previous_cell: IVec2,
    target_cell: IVec2,
    repath_timer: Timer,
}

#[derive(Component)]
pub struct GuardPathDebug;

#[derive(Component)]
pub struct GuardSpriteSheetConfig(pub SpriteSheetConfig);

fn is_walkable(tile: u8) -> bool {
    tile == GridType::FLOOR as u8
        || tile == GridType::ENTRANCE as u8
        || tile == GridType::EXIT as u8
        || tile == GridType::HIDE as u8
        || tile == GridType::COIN as u8
}

fn grid_in_bounds(grid: &Grid, cell: IVec2) -> bool {
    if cell.x < 0 || cell.y < 0 {
        return false;
    }

    let x = cell.x as usize;
    let y = cell.y as usize;
    y < grid.len() && x < grid[0].len()
}

fn nearest_walkable_from_corner(grid: &Grid, corner: usize) -> IVec2 {
    let h = grid.len() as i32;
    let w = grid[0].len() as i32;

    let (sx, sy) = match corner {
        0 => (1, 1),
        1 => (w - 2, 1),
        2 => (1, h - 2),
        _ => (w - 2, h - 2),
    };

    let max_radius = w.max(h);
    for r in 0..=max_radius {
        for y in (sy - r)..=(sy + r) {
            for x in (sx - r)..=(sx + r) {
                let cell = IVec2::new(x, y);
                if !grid_in_bounds(grid, cell) {
                    continue;
                }
                let tile = grid[y as usize][x as usize];
                if is_walkable(tile) {
                    return cell;
                }
            }
        }
    }

    IVec2::new(1, 1)
}

fn corner_candidates(grid: &Grid, corner: usize) -> Vec<IVec2> {
    let h = grid.len() as i32;
    let w = grid[0].len() as i32;
    let mid_x = w / 2;
    let mid_y = h / 2;

    let (min_x, max_x, min_y, max_y) = match corner {
        0 => (0, mid_x, 0, mid_y),
        1 => (mid_x, w, 0, mid_y),
        2 => (0, mid_x, mid_y, h),
        _ => (mid_x, w, mid_y, h),
    };

    let mut cells = Vec::new();
    for y in min_y..max_y {
        for x in min_x..max_x {
            let tile = grid[y as usize][x as usize];
            if is_walkable(tile) {
                cells.push(IVec2::new(x, y));
            }
        }
    }
    cells
}

fn pick_patrol_target(grid: &Grid, corner: usize, current: IVec2) -> IVec2 {
    let candidates = corner_candidates(grid, corner);
    if candidates.is_empty() {
        return current;
    }

    let mut best = candidates[0];
    let mut best_dist = 0.0_f32;
    for &c in &candidates {
        let d = (c - current).as_vec2().length();
        if d > best_dist {
            best = c;
            best_dist = d;
        }
    }

    if best_dist < PATROL_RETARGET_DISTANCE {
        nearest_walkable_from_corner(grid, corner)
    } else {
        best
    }
}

fn flood_fill_distances(grid: &Grid, goal: IVec2) -> Option<Vec<Vec<i32>>> {
    if !grid_in_bounds(grid, goal) {
        return None;
    }
    use std::collections::VecDeque;

    let h = grid.len();
    let w = grid[0].len();
    let mut distance = vec![vec![i32::MAX; w]; h];
    let mut q = VecDeque::new();
    distance[goal.y as usize][goal.x as usize] = 0;
    q.push_back(goal);

    let dirs = [IVec2::new(1, 0), IVec2::new(-1, 0), IVec2::new(0, 1), IVec2::new(0, -1)];

    while let Some(cell) = q.pop_front() {
        let d = distance[cell.y as usize][cell.x as usize];
        for dir in dirs {
            let next = cell + dir;
            if !grid_in_bounds(grid, next) {
                continue;
            }

            let nx = next.x as usize;
            let ny = next.y as usize;
            if !is_walkable(grid[ny][nx]) || distance[ny][nx] != i32::MAX {
                continue;
            }

            distance[ny][nx] = d + 1;
            q.push_back(next);
        }
    }

    Some(distance)
}

pub fn grid_flood_fill(grid: &Grid, from: IVec2, to: IVec2) -> Vec<Vec2> {
    if !grid_in_bounds(grid, from) || !grid_in_bounds(grid, to) {
        return Vec::new();
    }

    let Some(distance) = flood_fill_distances(grid, to) else {
        return Vec::new();
    };

    if distance[from.y as usize][from.x as usize] == i32::MAX {
        return Vec::new();
    }

    let mut path = Vec::new();
    let mut cur = from;
    path.push(Vec2::new(cur.x as f32, cur.y as f32));

    let dirs = [IVec2::new(1, 0), IVec2::new(-1, 0), IVec2::new(0, 1), IVec2::new(0, -1)];
    let max_steps = (grid.len() * grid[0].len()).max(1);
    let mut steps = 0usize;

    while cur != to && steps < max_steps {
        let cur_dist = distance[cur.y as usize][cur.x as usize];
        if cur_dist == i32::MAX {
            break;
        }

        let mut next_best = None;
        let mut next_dist = cur_dist;
        for dir in dirs {
            let n = cur + dir;
            if !grid_in_bounds(grid, n) {
                continue;
            }
            let nd = distance[n.y as usize][n.x as usize];
            if nd < next_dist {
                next_dist = nd;
                next_best = Some(n);
            }
        }

        let Some(step) = next_best else {
            break;
        };

        cur = step;
        path.push(Vec2::new(cur.x as f32, cur.y as f32));
        steps += 1;
    }

    if cur == to { path } else { Vec::new() }
}

fn flood_fill_next_step(
    grid: &Grid,
    start: IVec2,
    previous_cell: IVec2,
    distance: &[Vec<i32>],
    avoid_backtrack: bool,
) -> Option<IVec2> {
    if !grid_in_bounds(grid, start) {
        return None;
    }

    let dirs = [IVec2::new(1, 0), IVec2::new(-1, 0), IVec2::new(0, 1), IVec2::new(0, -1)];

    let mut best = None;
    let mut fallback_backtrack = None;
    let mut best_distance = i32::MAX;
    for dir in dirs {
        let next = start + dir;
        if !grid_in_bounds(grid, next) {
            continue;
        }
        let nd = distance[next.y as usize][next.x as usize];
        if nd == i32::MAX {
            continue;
        }

        if avoid_backtrack && next == previous_cell {
            if fallback_backtrack.is_none() || nd < best_distance {
                fallback_backtrack = Some(next);
            }
            continue;
        }

        if nd < best_distance {
            best_distance = nd;
            best = Some(next);
        }
    }

    if let Some(step) = best {
        Some(step)
    } else if let Some(step) = fallback_backtrack {
        Some(step)
    } else {
        None
    }
}

fn reconstruct_shortest_path(
    grid: &Grid,
    start: IVec2,
    goal: IVec2,
    distance: &[Vec<i32>],
) -> Vec<IVec2> {
    if distance.is_empty() {
        return Vec::new();
    }
    let path = grid_flood_fill(grid, start, goal);
    path.into_iter()
        .map(|p| IVec2::new(p.x as i32, p.y as i32))
        .collect()
}

pub fn clear_guards(commands: &mut Commands, guards: &Query<Entity, With<MazeGuard>>) {
    for e in guards {
        commands.entity(e).try_despawn();
    }
}

fn guard_sprite_sheet_config() -> SpriteSheetConfig {
    SpriteSheetConfig::from_grid_direction_by_column(
        UVec2::splat(16),
        4, // columns
        3, // rows
        FacingColumns::new(0, 1, 3, 2), // down, up, left, right columns
        0, // idle row
        1, // walk start row
        2, // walk frames
        0.10,
    )
}

pub fn spawn_guards_for_maze(
    commands: &mut Commands,
    asset_server: &AssetServer,
    maze_state: &MazeRenderState,
    texture_atlas_layouts: &mut Assets<TextureAtlasLayout>,
) {
    if !ENABLE_GUARDS {
        return;
    }

    let guard_img: Handle<Image> = asset_server.load(GUARD_SPRITE_PATH);
    let config = guard_sprite_sheet_config();
    let layout = config.layout(texture_atlas_layouts);

    for corner in 0..4 {
        let cell = nearest_walkable_from_corner(&maze_state.grid, corner);
        let pos = grid_cell_world_position(
            maze_state.grid[0].len(),
            maze_state.grid.len(),
            maze_state.world_center,
            cell.x as usize,
            cell.y as usize,
        );

        commands.spawn((
            MazeGuard,
            GuardBrain {
                corner,
                mode: GuardMode::Patrol,
                current_cell: cell,
                previous_cell: cell,
                target_cell: cell,
                repath_timer: Timer::from_seconds(GUARD_REPATH_SECS, TimerMode::Repeating),
            },
            GuardSpriteSheetConfig(config.clone()),
            SpriteSheetAnimator::new(Facing::Down, config.frame_time_secs),
            Sprite {
                image: guard_img.clone(),
                texture_atlas: Some(TextureAtlas {
                    layout: layout.clone(),
                    index: 0,
                }),
                ..default()
            },
            Transform::from_xyz(pos.x, pos.y, 12.0),
        ));
    }
}

pub fn update_guards(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    time: Res<Time>,
    maze_state: Option<Res<MazeRenderState>>,
    mut alert: ResMut<GuardAlertState>,
    mut capture: ResMut<GuardCaptureSequence>,
    mut movement_lock: Option<ResMut<PlayerMovementLock>>,
    mut player_query: Query<&mut Transform, With<Player>>,
    mut guards: Query<
        (Entity, &mut Transform, &mut GuardBrain, &mut Sprite, &mut SpriteSheetAnimator, &GuardSpriteSheetConfig),
        (With<MazeGuard>, Without<Player>),
    >,
    path_debug_query: Query<Entity, With<GuardPathDebug>>,
    mut path_redraw_timer: Local<Option<Timer>>,
) {
    if !ENABLE_GUARDS {
        for (guard_entity, _, _, _, _, _) in &mut guards {
            commands.entity(guard_entity).try_despawn();
        }
        for e in &path_debug_query {
            commands.entity(e).try_despawn();
        }
        alert.caught_player = false;
        capture.phase = GuardCapturePhase::Idle;
        capture.dialogue_line.clear();
        if let Some(exclaim) = capture.exclamation_entity.take() {
            commands.entity(exclaim).try_despawn();
        }
        if let Some(lock) = movement_lock.as_deref_mut() {
            lock.active = false;
        }
        return;
    }

    if maze_state.is_none() {
        alert.caught_player = false;
        capture.phase = GuardCapturePhase::Idle;
        if let Some(exclaim) = capture.exclamation_entity.take() {
            commands.entity(exclaim).try_despawn();
        }
        if let Some(lock) = movement_lock.as_deref_mut() {
            lock.active = false;
        }
        return;
    }

    if alert.caught_player {
        return;
    }

    if capture.phase != GuardCapturePhase::Idle {
        if let Some(lock) = movement_lock.as_deref_mut() {
            lock.active = true;
        }
        return;
    }

    let should_redraw_path = if SHOW_GUARD_PATH_DEBUG {
        let redraw_timer = path_redraw_timer
            .get_or_insert_with(|| Timer::from_seconds(0.1, TimerMode::Repeating));
        redraw_timer.tick(time.delta()).just_finished()
    } else {
        false
    };

    if should_redraw_path || !SHOW_GUARD_PATH_DEBUG {
        for e in &path_debug_query {
            commands.entity(e).despawn();
        }
    }

    let Some(maze_state) = maze_state else { return; };
    let Ok(mut player_transform) = player_query.single_mut() else {
        return;
    };
    let player_world = player_transform.translation.truncate();

    let Some(player_cell) = world_to_grid_cell(
        maze_state.grid[0].len(),
        maze_state.grid.len(),
        maze_state.world_center,
        player_world,
    ) else {
        return;
    };

    let player_hidden = if grid_in_bounds(&maze_state.grid, player_cell) {
        maze_state.grid[player_cell.y as usize][player_cell.x as usize] == GridType::HIDE as u8
    } else {
        false
    };

    let mut best_path: Option<(f32, bool, Vec<IVec2>)> = None;

    for (guard_entity, mut guard_tf, mut brain, mut sprite, mut animator, sheet_cfg) in &mut guards {
        if let Some(cell) = world_to_grid_cell(
            maze_state.grid[0].len(),
            maze_state.grid.len(),
            maze_state.world_center,
            guard_tf.translation.truncate(),
        ) {
            brain.current_cell = cell;
        }

        let path_to_player = grid_flood_fill(&maze_state.grid, player_cell, brain.current_cell);
        let distance_cells = path_to_player.len() as f32;
        let player_in_radius = !player_hidden && distance_cells <= GUARD_SIGHT_RADIUS_TILES;
        let can_see_player = player_in_radius;

        brain.mode = if player_in_radius {
            GuardMode::Chase
        } else {
            GuardMode::Patrol
        };

        let speed = if brain.mode == GuardMode::Chase {
            GUARD_SPEED * CHASE_SPEED_MULTIPLIER
        } else {
            GUARD_SPEED
        };

        brain.repath_timer.tick(time.delta());
        if brain.mode == GuardMode::Chase {
            // While chasing, always lock movement target to the live player cell.
            brain.target_cell = player_cell;
        } else if brain.repath_timer.just_finished() {
            brain.target_cell = if brain.mode == GuardMode::Chase {
                player_cell
            } else {
                pick_patrol_target(&maze_state.grid, brain.corner, brain.current_cell)
            };
        }

        let path_goal = if brain.mode == GuardMode::Chase {
            player_cell
        } else {
            brain.target_cell
        };
        let distances = flood_fill_distances(&maze_state.grid, path_goal);
        let next_cell = distances
            .as_ref()
            .and_then(|d| {
                flood_fill_next_step(
                    &maze_state.grid,
                    brain.current_cell,
                    brain.previous_cell,
                    d,
                    brain.mode == GuardMode::Patrol,
                )
            })
            .unwrap_or(brain.current_cell);

        if brain.mode == GuardMode::Chase {
            if let Some(d) = &distances {
                let path =
                    reconstruct_shortest_path(&maze_state.grid, brain.current_cell, player_cell, d);
                match &best_path {
                    Some((best_dist, _, _)) if distance_cells >= *best_dist => {}
                    _ => best_path = Some((distance_cells, can_see_player, path)),
                }
            }
        }

        let next_world = grid_cell_world_position(
            maze_state.grid[0].len(),
            maze_state.grid.len(),
            maze_state.world_center,
            next_cell.x.max(0) as usize,
            next_cell.y.max(0) as usize,
        );

        let to_target = next_world - guard_tf.translation.truncate();
        let mut move_vec = Vec2::ZERO;
        if to_target.length() > 0.1 {
            let step = to_target.normalize() * speed * time.delta_secs();
            move_vec = if step.length() >= to_target.length() {
                to_target
            } else {
                step
            };
            guard_tf.translation.x += move_vec.x;
            guard_tf.translation.y += move_vec.y;
        }

        if let Some(new_cell) = world_to_grid_cell(
            maze_state.grid[0].len(),
            maze_state.grid.len(),
            maze_state.world_center,
            guard_tf.translation.truncate(),
        ) {
            if new_cell != brain.current_cell {
                brain.previous_cell = brain.current_cell;
                brain.current_cell = new_cell;
            }
        }

        // Animate guard sprite sheet based on movement direction.
        animator.walking = to_target.length() > 0.1;
        if move_vec.x < -0.01 {
            animator.facing = Facing::Left;
        } else if move_vec.x > 0.01 {
            animator.facing = Facing::Right;
        } else if move_vec.y > 0.01 {
            animator.facing = Facing::Up;
        } else if move_vec.y < -0.01 {
            animator.facing = Facing::Down;
        }
        tick_animator(
            &mut animator,
            time.delta_secs(),
            sheet_cfg.0.walk_frames(),
        );
        apply_animator_to_sprite(&mut sprite, &sheet_cfg.0, &animator);

        let dist_to_player = (guard_tf.translation.truncate() - player_world).length();
        if dist_to_player <= GUARD_TOUCH_DISTANCE {
            // Keep player transform valid for callers that might inspect it in this frame.
            player_transform.translation.z = player_transform.translation.z.max(11.0);
            let exclamation = commands
                .spawn((
                    GuardExclamation,
                    Sprite {
                        image: asset_server.load(EXCLAMATION_PATH),
                        custom_size: Some(Vec2::splat(16.0)),
                        ..default()
                    },
                    Transform::from_xyz(0.0, 14.0, 2.0),
                ))
                .id();
            commands.entity(guard_entity).add_child(exclamation);
            capture.phase = GuardCapturePhase::Exclaim;
            capture.timer = Timer::from_seconds(GUARD_CAPTURE_EXCLAIM_SECS, TimerMode::Once);
            capture.exclamation_entity = Some(exclamation);
            capture.dialogue_line = random_capture_line(asset_server.as_ref());
            if let Some(lock) = movement_lock.as_deref_mut() {
                lock.active = true;
            }
            return;
        }
    }

    if SHOW_GUARD_PATH_DEBUG && should_redraw_path {
        if let Some((_, can_see_player, path)) = best_path {
        for cell in path {
            let pos = grid_cell_world_position(
                maze_state.grid[0].len(),
                maze_state.grid.len(),
                maze_state.world_center,
                cell.x.max(0) as usize,
                cell.y.max(0) as usize,
            );
            commands.spawn((
                GuardPathDebug,
                Sprite::from_color(
                    if can_see_player {
                        Color::srgba(1.0, 0.12, 0.12, 0.45)
                    } else {
                        Color::srgba(1.0, 0.72, 0.12, 0.35)
                    },
                    Vec2::splat(TILE_SIZE * 0.35),
                ),
                Transform::from_xyz(pos.x, pos.y, PATH_DEBUG_Z),
            ));
        }
    }
    }
}

fn setup_guard_capture_ui(mut commands: Commands) {
    commands
        .spawn((
            GuardCaptureUiRoot,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(24.0),
                right: Val::Px(24.0),
                bottom: Val::Px(20.0),
                min_height: Val::Px(96.0),
                padding: UiRect::all(Val::Px(12.0)),
                border: UiRect::all(Val::Px(3.0)),
                ..default()
            },
            BackgroundColor(Color::BLACK),
            BorderColor::all(Color::WHITE),
            Visibility::Hidden,
            ZIndex(10000),
        ))
        .with_children(|parent| {
            parent.spawn((
                GuardCaptureUiText,
                Text::new(""),
                TextFont {
                    font_size: 32.0,
                    ..default()
                },
                TextColor(Color::WHITE),
            ));
        });
}

fn random_capture_line(asset_server: &AssetServer) -> String {
    let _ = asset_server;
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("assets")
        .join("lines.jsonc");
    let Ok(raw) = fs::read_to_string(path) else {
        return GUARD_CAPTURE_DIALOGUE.to_string();
    };
    let cleaned = raw
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    let Ok(json) = serde_json::from_str::<Value>(&cleaned) else {
        return GUARD_CAPTURE_DIALOGUE.to_string();
    };
    let Some(caught) = json.get("caught").and_then(Value::as_array) else {
        return GUARD_CAPTURE_DIALOGUE.to_string();
    };
    if caught.is_empty() {
        return GUARD_CAPTURE_DIALOGUE.to_string();
    }

    let mut rng = rand::rng();
    let idx = rng.random_range(0..caught.len());
    caught[idx]
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| GUARD_CAPTURE_DIALOGUE.to_string())
}

fn run_guard_capture_sequence(
    time: Res<Time>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut capture: ResMut<GuardCaptureSequence>,
    mut alert: ResMut<GuardAlertState>,
    mut movement_lock: Option<ResMut<PlayerMovementLock>>,
    mut ui_root_q: Query<&mut Visibility, With<GuardCaptureUiRoot>>,
    mut ui_text_q: Query<&mut Text, With<GuardCaptureUiText>>,
    mut commands: Commands,
) {
    let Ok(mut ui_vis) = ui_root_q.single_mut() else {
        return;
    };
    let Ok(mut ui_text) = ui_text_q.single_mut() else {
        return;
    };

    if capture.phase == GuardCapturePhase::Idle {
        *ui_vis = Visibility::Hidden;
        return;
    }

    if let Some(lock) = movement_lock.as_deref_mut() {
        lock.active = true;
    }

    match capture.phase {
        GuardCapturePhase::Idle => {}
        GuardCapturePhase::Exclaim => {
            *ui_vis = Visibility::Hidden;
            capture.timer.tick(time.delta());
            if capture.timer.is_finished() {
                capture.phase = GuardCapturePhase::Dialogue;
            }
        }
        GuardCapturePhase::Dialogue => {
            *ui_vis = Visibility::Visible;
            ui_text.0 = capture.dialogue_line.clone();
            if keyboard.just_pressed(KeyCode::Enter) || keyboard.just_pressed(KeyCode::NumpadEnter)
            {
                if let Some(exclaim) = capture.exclamation_entity.take() {
                    commands.entity(exclaim).try_despawn();
                }
                capture.phase = GuardCapturePhase::Idle;
                capture.dialogue_line.clear();
                *ui_vis = Visibility::Hidden;
                if let Some(lock) = movement_lock.as_deref_mut() {
                    lock.active = false;
                }
                alert.caught_player = true;
            }
        }
    }
}
