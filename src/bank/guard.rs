use bevy::prelude::*;
use bevy::ui::widget::NodeImageMode;
use rand::RngExt;
use serde_json::Value;

use crate::bank::render::maze::{grid_cell_world_position, world_to_grid_cell, MazeRenderState, TILE_SIZE};
use crate::bank::{Grid, GridType};
use crate::entity_dialogue::PlayerMovementLock;
use crate::map::{HeistLifetimeStats, HeistRunStats};
use crate::player::Player;
use crate::receipts::Receipt;
use crate::sprite_sheet::{
    apply_animator_to_sprite, tick_animator, Facing, FacingColumns, SpriteSheetAnimator,
    SpriteSheetConfig,
};

const GUARD_SPEED: f32 = 50.0;
const GUARD_REPATH_SECS: f32 = 0.35;
const GUARD_TOUCH_DISTANCE: f32 = 16.0;
const GUARD_SIGHT_RADIUS_TILES: f32 = 6.5;
const CHASE_SPEED_MULTIPLIER: f32 = 1.5;
const PATROL_RETARGET_DISTANCE: f32 = 4.0;
const PATH_DEBUG_Z: f32 = 13.0;
const SHOW_GUARD_PATH_DEBUG: bool = false;
const ENABLE_GUARDS: bool = true;
const SHARED_GUARD_LOCK_ON: bool = true;
const GUARD_SPRITE_PATH: &str = "guard.png";
const EXCLAMATION_PATH: &str = "exclamation.png";
const GUARD_CAPTURE_EXCLAIM_SECS: f32 = 0.35;
const GUARD_CAPTURE_DIALOGUE: &str = "Hey!\nCaught you.\nPress ENTER.";

#[derive(Clone, Copy)]
struct GuardExclusionZoneConfig {
    tile: u8,
    radius: i32,
}

const GUARD_EXCLUSION_ZONES: &[GuardExclusionZoneConfig] = &[GuardExclusionZoneConfig {
    tile: GridType::ENTRANCE as u8,
    radius: 2,
}];

fn overlaps_aabb_centers(a_center: Vec2, a_size: Vec2, b_center: Vec2, b_size: Vec2) -> bool {
    let half = (a_size + b_size) * 0.5;
    let d = (a_center - b_center).abs();
    d.x <= half.x && d.y <= half.y
}

pub struct GuardPlugin;

impl Plugin for GuardPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GuardAlertState>()
            .init_resource::<SharedGuardLockState>()
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

#[derive(Resource, Default)]
pub struct SharedGuardLockState {
    active: bool,
    last_known_player_cell: Option<IVec2>,
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

fn find_tile_cell(grid: &Grid, tile_kind: u8) -> Option<IVec2> {
    for (y, row) in grid.iter().enumerate() {
        for (x, &tile) in row.iter().enumerate() {
            if tile == tile_kind {
                return Some(IVec2::new(x as i32, y as i32));
            }
        }
    }
    None
}

fn exclusion_zone_centers(grid: &Grid) -> Vec<(IVec2, i32)> {
    let mut zones = Vec::new();
    for zone in GUARD_EXCLUSION_ZONES {
        if let Some(center) = find_tile_cell(grid, zone.tile) {
            zones.push((center, zone.radius));
        }
    }
    zones
}

fn is_in_any_exclusion_zone(cell: IVec2, zone_centers: &[(IVec2, i32)]) -> bool {
    for (center, radius) in zone_centers {
        if (cell.x - center.x).abs() <= *radius && (cell.y - center.y).abs() <= *radius {
            return true;
        }
    }
    false
}

fn is_guard_walkable_at(grid: &Grid, cell: IVec2, zone_centers: &[(IVec2, i32)]) -> bool {
    if !grid_in_bounds(grid, cell) {
        return false;
    }
    let tile = grid[cell.y as usize][cell.x as usize];
    is_walkable(tile) && !is_in_any_exclusion_zone(cell, zone_centers)
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
    let zone_centers = exclusion_zone_centers(grid);
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
                if is_guard_walkable_at(grid, cell, &zone_centers) {
                    return cell;
                }
            }
        }
    }

    IVec2::new(1, 1)
}

fn corner_candidates(grid: &Grid, corner: usize) -> Vec<IVec2> {
    let zone_centers = exclusion_zone_centers(grid);
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
            let cell = IVec2::new(x, y);
            if is_guard_walkable_at(grid, cell, &zone_centers) {
                cells.push(IVec2::new(x, y));
            }
        }
    }
    cells
}

fn pick_patrol_target(grid: &Grid, corner: usize, current: IVec2, previous_target: IVec2) -> IVec2 {
    let candidates = corner_candidates(grid, corner);
    if candidates.is_empty() {
        return current;
    }

    let mut ranked: Vec<(IVec2, f32)> = candidates
        .into_iter()
        .filter(|&c| c != previous_target)
        .map(|c| (c, (c - current).as_vec2().length()))
        .collect();

    if ranked.is_empty() {
        return nearest_walkable_from_corner(grid, corner);
    }

    ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
    let best_dist = ranked[0].1;

    if best_dist < PATROL_RETARGET_DISTANCE {
        return nearest_walkable_from_corner(grid, corner);
    }

    // Pick among the farthest few cells to avoid deterministic patrol loops.
    let top_n = ranked.len().min(8);
    let mut rng = rand::rng();
    let idx = rng.random_range(0..top_n);
    ranked[idx].0
}

fn flood_fill_distances(grid: &Grid, goal: IVec2) -> Option<Vec<Vec<i32>>> {
    let zone_centers = exclusion_zone_centers(grid);
    if !is_guard_walkable_at(grid, goal, &zone_centers) {
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
            if !is_guard_walkable_at(grid, next, &zone_centers) || distance[ny][nx] != i32::MAX {
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
    heist_stats: Option<Res<HeistRunStats>>,
    heist_lifetime: Option<Res<HeistLifetimeStats>>,
    player_money: Option<Res<crate::PlayerMoney>>,
    maze_state: Option<Res<MazeRenderState>>,
    mut alert: ResMut<GuardAlertState>,
    mut capture: ResMut<GuardCaptureSequence>,
    mut shared_lock_state: ResMut<SharedGuardLockState>,
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
        let tile = maze_state.grid[player_cell.y as usize][player_cell.x as usize];
        tile == GridType::HIDE as u8 || tile == GridType::SHAFT as u8
    } else {
        false
    };

    let mut best_path: Option<(f32, bool, Vec<IVec2>)> = None;
    let mut any_guard_has_lock = false;
    let mut shared_chase_target = player_cell;

    if SHARED_GUARD_LOCK_ON && !player_hidden {
        for (_, guard_tf, _, _, _, _) in &mut guards {
            let Some(guard_cell) = world_to_grid_cell(
                maze_state.grid[0].len(),
                maze_state.grid.len(),
                maze_state.world_center,
                guard_tf.translation.truncate(),
            ) else {
                continue;
            };

            let path_to_player = grid_flood_fill(&maze_state.grid, player_cell, guard_cell);
            let distance_cells = path_to_player.len() as f32;
            let player_in_radius = !player_hidden && distance_cells <= GUARD_SIGHT_RADIUS_TILES;
            if player_in_radius {
                any_guard_has_lock = true;
                break;
            }
        }

        if any_guard_has_lock {
            shared_lock_state.active = true;
            shared_lock_state.last_known_player_cell = Some(player_cell);
        } else {
            shared_lock_state.active = false;
            shared_lock_state.last_known_player_cell = None;
        }
    }

    if SHARED_GUARD_LOCK_ON && player_hidden {
        shared_lock_state.active = false;
        shared_lock_state.last_known_player_cell = None;
        shared_chase_target = player_cell;
    }

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

        let should_chase = if SHARED_GUARD_LOCK_ON {
            if player_hidden {
                shared_lock_state.active && shared_lock_state.last_known_player_cell.is_some()
            } else {
                any_guard_has_lock
            }
        } else {
            player_in_radius
        };
        brain.mode = if should_chase {
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
            brain.target_cell = if SHARED_GUARD_LOCK_ON {
                shared_chase_target
            } else {
                player_cell
            };
        } else if brain.repath_timer.just_finished() {
            brain.target_cell = if brain.mode == GuardMode::Chase {
                if SHARED_GUARD_LOCK_ON {
                    shared_chase_target
                } else {
                    player_cell
                }
            } else {
                pick_patrol_target(
                    &maze_state.grid,
                    brain.corner,
                    brain.current_cell,
                    brain.target_cell,
                )
            };
        }

        let path_goal = if brain.mode == GuardMode::Chase {
            if SHARED_GUARD_LOCK_ON {
                shared_chase_target
            } else {
                player_cell
            }
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

        let guard_center = guard_tf.translation.truncate();
        let guard_size = Vec2::splat(GUARD_TOUCH_DISTANCE.max(1.0));
        let player_size = Vec2::splat(GUARD_TOUCH_DISTANCE.max(1.0));
        if !player_hidden && overlaps_aabb_centers(guard_center, guard_size, player_world, player_size) {
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
            let money = player_money.as_ref().map(|m| m.amount).unwrap_or(0);
            let successful_robberies = heist_lifetime
                .as_ref()
                .map(|s| s.successful_robberies)
                .unwrap_or(0);
            let failed_robberies = heist_lifetime
                .as_ref()
                .map(|s| s.failed_robberies)
                .unwrap_or(0);
            let (survival_secs, stopped_at_shaft) = if let Some(stats) = heist_stats.as_ref() {
                let secs = if stats.active {
                    (time.elapsed_secs() - stats.start_elapsed_secs).max(0.0)
                } else {
                    0.0
                };
                (secs, stats.stopped_at_shaft)
            } else {
                (0.0, false)
            };
            capture.dialogue_line = random_capture_line(CaptureLineContext {
                money,
                survival_secs,
                stopped_at_shaft,
                successful_robberies,
                failed_robberies,
            });
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

fn setup_guard_capture_ui(mut commands: Commands, asset_server: Res<AssetServer>) {
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
                border: UiRect::all(Val::Px(0.0)),
                ..default()
            },
            BackgroundColor(Color::NONE),
            BorderColor::all(Color::NONE),
            ImageNode::new(asset_server.load("bubble.png")).with_mode(NodeImageMode::Sliced(
                TextureSlicer {
                    border: BorderRect::all(6.0),
                    center_scale_mode: SliceScaleMode::Stretch,
                    sides_scale_mode: SliceScaleMode::Stretch,
                    max_corner_scale: 1.0,
                },
            )),
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

#[derive(Clone, Copy)]
struct CaptureLineContext {
    money: i32,
    survival_secs: f32,
    stopped_at_shaft: bool,
    successful_robberies: u32,
    failed_robberies: u32,
}

fn random_capture_line(ctx: CaptureLineContext) -> String {
    let receipt = Receipt {
        successful: false,
        money: ctx.money,
        profit: 0,
        successful_robberies: ctx.successful_robberies,
        failed_robberies: ctx.failed_robberies,
        stopped_at_shaft: ctx.stopped_at_shaft,
        time_till_death_secs: Some(ctx.survival_secs),
        heist_duration_secs: ctx.survival_secs,
    };
    let lines_object = receipt.lines_object();
    let Some(json) = lines_object else {
        return GUARD_CAPTURE_DIALOGUE.to_string();
    };
    let mut candidates: Vec<String> = Vec::new();

    if let Some(conditional) = json.get("caught_conditions").and_then(Value::as_array) {
        for condition in conditional {
            let when = condition.get("when").unwrap_or(&Value::Null);
            if !caught_condition_matches(when, ctx) {
                continue;
            }
            if let Some(lines) = condition.get("lines").and_then(Value::as_array) {
                for line in lines {
                    if let Some(s) = line.as_str() {
                        candidates.push(s.to_string());
                    }
                }
            }
        }
    }

    if candidates.is_empty() {
        if let Some(caught) = json.get("caught").and_then(Value::as_array) {
            for line in caught {
                if let Some(s) = line.as_str() {
                    candidates.push(s.to_string());
                }
            }
        }
    }

    if candidates.is_empty() {
        return GUARD_CAPTURE_DIALOGUE.to_string();
    }

    let mut rng = rand::rng();
    let idx = rng.random_range(0..candidates.len());
    candidates[idx].clone()
}

fn caught_condition_matches(when: &Value, ctx: CaptureLineContext) -> bool {
    if let Some(required) = when.get("stopped_at_shaft").and_then(Value::as_bool) {
        if ctx.stopped_at_shaft != required {
            return false;
        }
    }
    if let Some(min) = when.get("money_min").and_then(Value::as_i64) {
        if i64::from(ctx.money) < min {
            return false;
        }
    }
    if let Some(max) = when.get("money_max").and_then(Value::as_i64) {
        if i64::from(ctx.money) > max {
            return false;
        }
    }
    if let Some(min) = when.get("survival_secs_min").and_then(Value::as_f64) {
        if (ctx.survival_secs as f64) < min {
            return false;
        }
    }
    if let Some(max) = when.get("survival_secs_max").and_then(Value::as_f64) {
        if (ctx.survival_secs as f64) > max {
            return false;
        }
    }
    if let Some(min) = when
        .get("successful_robberies_min")
        .and_then(Value::as_u64)
    {
        if u64::from(ctx.successful_robberies) < min {
            return false;
        }
    }
    if let Some(max) = when
        .get("successful_robberies_max")
        .and_then(Value::as_u64)
    {
        if u64::from(ctx.successful_robberies) > max {
            return false;
        }
    }
    if let Some(min) = when.get("failed_robberies_min").and_then(Value::as_u64) {
        if u64::from(ctx.failed_robberies) < min {
            return false;
        }
    }
    if let Some(max) = when.get("failed_robberies_max").and_then(Value::as_u64) {
        if u64::from(ctx.failed_robberies) > max {
            return false;
        }
    }
    true
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
