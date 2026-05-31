use bevy::prelude::*;
use bevy::app::AppExit;
use bevy_ecs_ldtk::LdtkProjectHandle;

use crate::bank::generation;
use crate::bank::guard::{clear_guards, spawn_guards_for_maze, GuardAlertState, MazeGuard};
use crate::bank::render::maze::{
    clear_maze, grid_cell_world_position, render_maze, world_to_grid_cell, MazeRenderState,
    MazeTile,
};
use crate::bank::GridType;
use crate::entity_dialogue::PlayerMovementLock;
use crate::player::Player;
use crate::tavern::HeistReportMessage;
use crate::text_bubble::TextBubble;
use crate::PlayerMoney;

use super::ldtk::{
    despawn_ldtk_world, load_or_spawn_ldtk_world, loaded_map_path, map_asset_to_disk_path,
    scene_tag_from_map_asset_path, scene_to_asset_path, set_loaded_map, LoadedMap,
    PendingPlayerStartScene, TransferPortal, TOWN_MAP_ASSET_PATH,
};

const PLAYER_HITBOX_SIZE: f32 = 16.0;
const SCENE_TRANSFER_COOLDOWN_SECS: f32 = 0.35;
const GENERATED_MAZE_SIZE: f32 = 31.0;
const MAZE_SCENE_KEY: &str = "maze";
const SOUP_STORE_SCENE_KEY: &str = "soup_store";
const EXIT_SCENE_KEY: &str = "exit";
const JAIL_MAP_ASSET_PATH: &str = "maps/jail.ldtk";
const SCENE_FADE_OUT_SECS: f32 = 0.5;
const SCENE_FADE_IN_SECS: f32 = 0.5;
const SCENE_BLACK_HOLD_SECS: f32 = 0.6;
const SCENE_FADE_OVERLAY_SIZE: f32 = 10000.0;
const SOUP_STORE_MAP_ASSET_PATH: &str = "maps/soup_store.ldtk";

#[derive(Clone, Copy)]
pub struct SceneBackgroundSpec {
    pub color: Color,
}

fn scene_background_spec(scene: &str) -> SceneBackgroundSpec {
    match scene {
        // Maze: dark.
        MAZE_SCENE_KEY => SceneBackgroundSpec {
            color: Color::srgb(0., 0., 0.),
        },
        // Town: green
        TOWN_MAP_ASSET_PATH => SceneBackgroundSpec {
            color: Color::srgb(0.6, 0.722, 0.518),
        },
        // Soup store: slightly warm.
        SOUP_STORE_MAP_ASSET_PATH => SceneBackgroundSpec {
            color: Color::srgb(0.88, 0.86, 0.82),
        },
        // Jail: colder + flatter.
        JAIL_MAP_ASSET_PATH => SceneBackgroundSpec {
            color: Color::srgb(0.725, 0.769, 0.686),
        },
        // Fallback.
        _ => SceneBackgroundSpec {
            color: Color::srgb(0.6, 0.722, 0.518),
        },
    }
}

#[derive(Component)]
pub struct SceneFadeOverlay;
#[derive(Component)]
pub struct SceneBackground;

#[derive(Message, Clone)]
pub struct SceneChangeRequest {
    pub asset_path: String,
}

#[derive(Clone)]
enum SceneTransitionTarget {
    Maze { center: Vec2 },
    OtherMap { asset_path: String },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SceneTransitionPhase {
    Idle,
    FadeOut,
    HoldBlack,
    FadeIn,
}

#[derive(Resource)]
pub struct SceneTransitionState {
    phase: SceneTransitionPhase,
    timer: Timer,
    target: Option<SceneTransitionTarget>,
}

impl Default for SceneTransitionState {
    fn default() -> Self {
        let mut timer = Timer::from_seconds(0.0, TimerMode::Once);
        timer.finish();
        Self {
            phase: SceneTransitionPhase::Idle,
            timer,
            target: None,
        }
    }
}

#[derive(Resource)]
pub struct SceneTransferCooldown {
    timer: Timer,
}

impl Default for SceneTransferCooldown {
    fn default() -> Self {
        let mut timer = Timer::from_seconds(SCENE_TRANSFER_COOLDOWN_SECS, TimerMode::Once);
        timer.finish();
        Self { timer }
    }
}

#[derive(Resource, Default)]
pub struct HeistRunStats {
    pub active: bool,
    pub start_elapsed_secs: f32,
    pub stopped_at_shaft: bool,
}

#[derive(Resource, Default)]
pub struct HeistLifetimeStats {
    pub successful_robberies: u32,
    pub failed_robberies: u32,
}

fn heist_elapsed_secs(stats: &HeistRunStats, now: f32) -> f32 {
    if !stats.active {
        return 0.0;
    }
    (now - stats.start_elapsed_secs).max(0.0)
}

pub fn setup_camera_and_fade(mut commands: Commands, assets: Res<AssetServer>) {
    let _ = assets.load::<Image>("bank/moon.png");
    commands.spawn((
        Camera2d,
        Projection::Orthographic(OrthographicProjection {
            scale: 0.45,
            ..OrthographicProjection::default_2d()
        }),
    ));
    commands.spawn((
        SceneFadeOverlay,
        Sprite::from_color(
            Color::srgba(0.0, 0.0, 0.0, 0.0),
            Vec2::splat(SCENE_FADE_OVERLAY_SIZE),
        ),
        Transform::from_xyz(0.0, 0.0, 1000.0),
    ));
}

fn overlaps_aabb(a_center: Vec2, a_size: Vec2, b_center: Vec2, b_size: Vec2) -> bool {
    let dx = (a_center.x - b_center.x).abs();
    let dy = (a_center.y - b_center.y).abs();
    dx <= (a_size.x + b_size.x) * 0.5 && dy <= (a_size.y + b_size.y) * 0.5
}

fn request_scene_transition(
    transition: &mut SceneTransitionState,
    target: SceneTransitionTarget,
) -> bool {
    if transition.phase != SceneTransitionPhase::Idle {
        return false;
    }

    transition.phase = SceneTransitionPhase::FadeOut;
    transition.timer = Timer::from_seconds(SCENE_FADE_OUT_SECS, TimerMode::Once);
    transition.target = Some(target);
    true
}

pub fn trigger_scene_transfer(
    time: Res<Time>,
    mut cooldown: ResMut<SceneTransferCooldown>,
    loaded_map: Res<LoadedMap>,
    mut transition: ResMut<SceneTransitionState>,
    mut heist_stats: ResMut<HeistRunStats>,
    mut pending_player_start: ResMut<PendingPlayerStartScene>,
    mut app_exit: MessageWriter<AppExit>,
    player_query: Query<&Transform, With<Player>>,
    transfer_query: Query<(&GlobalTransform, &TransferPortal)>,
) {
    if transition.phase != SceneTransitionPhase::Idle {
        return;
    }
    cooldown.timer.tick(time.delta());
    if !cooldown.timer.is_finished() {
        return;
    }
    let Ok(player_transform) = player_query.single() else {
        return;
    };
    let player_center = player_transform.translation.truncate();
    let player_size = Vec2::splat(PLAYER_HITBOX_SIZE);

    for (transfer_tf, transfer) in &transfer_query {
        let transfer_center = transfer_tf.translation().truncate();
        let transfer_size = Vec2::new(transfer.width.max(1.0), transfer.height.max(1.0));
        if !overlaps_aabb(player_center, player_size, transfer_center, transfer_size) {
            continue;
        }

        if transfer.scene.trim().eq_ignore_ascii_case(EXIT_SCENE_KEY) {
            app_exit.write(AppExit::Success);
            break;
        }

        if transfer.scene.trim().eq_ignore_ascii_case(MAZE_SCENE_KEY) {
            if loaded_map_path(&loaded_map) == MAZE_SCENE_KEY {
                continue;
            }
            if request_scene_transition(
                &mut transition,
                SceneTransitionTarget::Maze {
                    center: player_transform.translation.truncate(),
                },
            ) {
                heist_stats.active = true;
                heist_stats.start_elapsed_secs = time.elapsed_secs();
                heist_stats.stopped_at_shaft = false;
                cooldown.timer = Timer::from_seconds(SCENE_TRANSFER_COOLDOWN_SECS, TimerMode::Once);
                break;
            }
        }

        let next_scene_asset_path = scene_to_asset_path(&transfer.scene);
        if next_scene_asset_path == loaded_map_path(&loaded_map) {
            continue;
        }
        let next_scene_fs_path = map_asset_to_disk_path(&next_scene_asset_path);
        if !next_scene_fs_path.exists() {
            continue;
        }
        if request_scene_transition(
            &mut transition,
            SceneTransitionTarget::OtherMap {
                asset_path: next_scene_asset_path,
            },
        ) {
            pending_player_start.from_scene =
                Some(scene_tag_from_map_asset_path(loaded_map_path(&loaded_map)));
            if loaded_map_path(&loaded_map) == MAZE_SCENE_KEY
                && transfer
                    .scene
                    .trim()
                    .eq_ignore_ascii_case(SOUP_STORE_SCENE_KEY)
            {
                heist_stats.stopped_at_shaft = true;
            }
            cooldown.timer = Timer::from_seconds(SCENE_TRANSFER_COOLDOWN_SECS, TimerMode::Once);
            break;
        }
    }
}

pub fn handle_scene_change_request(
    loaded_map: Res<LoadedMap>,
    mut transition: ResMut<SceneTransitionState>,
    mut pending_player_start: ResMut<PendingPlayerStartScene>,
    mut requests: MessageReader<SceneChangeRequest>,
) {
    if transition.phase != SceneTransitionPhase::Idle {
        return;
    }

    for req in requests.read() {
        if req.asset_path == loaded_map_path(&loaded_map) {
            continue;
        }
        if request_scene_transition(
            &mut transition,
            SceneTransitionTarget::OtherMap {
                asset_path: req.asset_path.clone(),
            },
        ) {
            pending_player_start.from_scene =
                Some(scene_tag_from_map_asset_path(loaded_map_path(&loaded_map)));
            break;
        }
    }
}

pub fn handle_guard_capture(
    alert: ResMut<GuardAlertState>,
    loaded_map: Res<LoadedMap>,
    time: Res<Time>,
    mut cooldown: ResMut<SceneTransferCooldown>,
    mut transition: ResMut<SceneTransitionState>,
    mut heist_stats: ResMut<HeistRunStats>,
    mut lifetime_stats: ResMut<HeistLifetimeStats>,
    mut pending_player_start: ResMut<PendingPlayerStartScene>,
    mut player_money: ResMut<PlayerMoney>,
    mut heist_report_writer: MessageWriter<HeistReportMessage>,
) {
    if transition.phase != SceneTransitionPhase::Idle
        || !alert.caught_player
        || loaded_map_path(&loaded_map) != MAZE_SCENE_KEY
    {
        return;
    }
    if request_scene_transition(
        &mut transition,
        SceneTransitionTarget::OtherMap {
            asset_path: JAIL_MAP_ASSET_PATH.to_string(),
        },
    ) {
        pending_player_start.from_scene =
            Some(scene_tag_from_map_asset_path(loaded_map_path(&loaded_map)));
        let elapsed = heist_elapsed_secs(&heist_stats, time.elapsed_secs());
        let money_before_reset = player_money.amount;
        heist_report_writer.write(HeistReportMessage {
            successful: false,
            money: 0,
            profit: -money_before_reset,
            successful_robberies: lifetime_stats.successful_robberies,
            stopped_at_shaft: heist_stats.stopped_at_shaft,
            time_till_death_secs: Some(elapsed),
            heist_duration_secs: elapsed,
        });
        heist_stats.active = false;
        heist_stats.stopped_at_shaft = false;
        heist_stats.start_elapsed_secs = 0.0;
        lifetime_stats.failed_robberies = lifetime_stats.failed_robberies.saturating_add(1);
        cooldown.timer = Timer::from_seconds(SCENE_TRANSFER_COOLDOWN_SECS, TimerMode::Once);
        player_money.amount = 0;
    }
}

pub fn handle_maze_exit(
    loaded_map: Res<LoadedMap>,
    time: Res<Time>,
    mut cooldown: ResMut<SceneTransferCooldown>,
    mut transition: ResMut<SceneTransitionState>,
    mut heist_stats: ResMut<HeistRunStats>,
    mut lifetime_stats: ResMut<HeistLifetimeStats>,
    mut pending_player_start: ResMut<PendingPlayerStartScene>,
    maze_state: Option<Res<MazeRenderState>>,
    player_query: Query<&Transform, With<Player>>,
    player_money: Res<PlayerMoney>,
    mut heist_report_writer: MessageWriter<HeistReportMessage>,
) {
    if transition.phase != SceneTransitionPhase::Idle {
        return;
    }
    if loaded_map_path(&loaded_map) != MAZE_SCENE_KEY {
        return;
    }
    let Some(maze_state) = maze_state else {
        return;
    };
    let Ok(player_transform) = player_query.single() else {
        return;
    };
    let Some(player_cell) = world_to_grid_cell(
        maze_state.grid[0].len(),
        maze_state.grid.len(),
        maze_state.world_center,
        player_transform.translation.truncate(),
    ) else {
        return;
    };
    if maze_state.grid[player_cell.y as usize][player_cell.x as usize] != GridType::EXIT as u8 {
        return;
    }
    if request_scene_transition(
        &mut transition,
        SceneTransitionTarget::OtherMap {
            asset_path: TOWN_MAP_ASSET_PATH.to_string(),
        },
    ) {
        pending_player_start.from_scene =
            Some(scene_tag_from_map_asset_path(loaded_map_path(&loaded_map)));
        let elapsed = heist_elapsed_secs(&heist_stats, time.elapsed_secs());
        heist_report_writer.write(HeistReportMessage {
            successful: true,
            money: player_money.amount,
            profit: player_money.amount,
            successful_robberies: lifetime_stats.successful_robberies.saturating_add(1),
            stopped_at_shaft: heist_stats.stopped_at_shaft,
            time_till_death_secs: None,
            heist_duration_secs: elapsed,
        });
        heist_stats.active = false;
        heist_stats.stopped_at_shaft = false;
        heist_stats.start_elapsed_secs = 0.0;
        lifetime_stats.successful_robberies =
            lifetime_stats.successful_robberies.saturating_add(1);
        cooldown.timer = Timer::from_seconds(SCENE_TRANSFER_COOLDOWN_SECS, TimerMode::Once);
    }
}

pub fn update_scene_transition(
    time: Res<Time>,
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut texture_atlas_layouts: ResMut<Assets<TextureAtlasLayout>>,
    mut transition: ResMut<SceneTransitionState>,
    mut movement_lock: Option<ResMut<PlayerMovementLock>>,
    mut loaded_map: ResMut<LoadedMap>,
    mut player_query: Query<&mut Transform, With<Player>>,
    mut ldtk_world_query: Query<(Entity, &mut LdtkProjectHandle)>,
    maze_tiles: Query<Entity, With<MazeTile>>,
    guards: Query<Entity, With<MazeGuard>>,
    bubble_owners: Query<Entity, With<TextBubble>>,
) {
    if transition.phase != SceneTransitionPhase::Idle {
        if let Some(lock) = movement_lock.as_deref_mut() {
            lock.active = true;
        }
    }

    if transition.phase == SceneTransitionPhase::Idle {
        return;
    }
    transition.timer.tick(time.delta());
    if !transition.timer.is_finished() {
        return;
    }
    match transition.phase {
        SceneTransitionPhase::Idle => {}
        SceneTransitionPhase::FadeOut => {
            if let Some(target) = transition.target.clone() {
                for entity in &bubble_owners {
                    commands.entity(entity).remove::<TextBubble>();
                }
                clear_maze(&mut commands, &maze_tiles);
                clear_guards(&mut commands, &guards);
                match target {
                    SceneTransitionTarget::Maze { center } => {
                        despawn_ldtk_world(&mut commands, &mut ldtk_world_query);
                        let maze = generation::generate(GENERATED_MAZE_SIZE);
                        render_maze(&maze, center, &mut commands, asset_server.as_ref());
                        if let Ok(mut player_transform) = player_query.single_mut() {
                            if let Some(entrance_cell) =
                                generation::find_tile(&maze, GridType::ENTRANCE as u8)
                            {
                                let entrance_world = grid_cell_world_position(
                                    maze[0].len(),
                                    maze.len(),
                                    center,
                                    entrance_cell.x.max(0) as usize,
                                    entrance_cell.y.max(0) as usize,
                                );
                                player_transform.translation.x = entrance_world.x;
                                player_transform.translation.y = entrance_world.y;
                            }
                        }
                        let maze_state = MazeRenderState {
                            grid: maze.clone(),
                            world_center: center,
                        };
                        spawn_guards_for_maze(
                            &mut commands,
                            asset_server.as_ref(),
                            &maze_state,
                            texture_atlas_layouts.as_mut(),
                        );
                        set_loaded_map(&mut loaded_map, MAZE_SCENE_KEY.to_string());
                    }
                    SceneTransitionTarget::OtherMap { asset_path } => {
                        load_or_spawn_ldtk_world(
                            &mut commands,
                            asset_server.as_ref(),
                            &mut ldtk_world_query,
                            asset_path.clone(),
                        );
                        set_loaded_map(&mut loaded_map, asset_path);
                    }
                }
            }
            transition.phase = SceneTransitionPhase::HoldBlack;
            transition.timer = Timer::from_seconds(SCENE_BLACK_HOLD_SECS, TimerMode::Once);
            if SCENE_BLACK_HOLD_SECS <= 0.0 {
                transition.timer.finish();
            }
        }
        SceneTransitionPhase::HoldBlack => {
            transition.phase = SceneTransitionPhase::FadeIn;
            transition.timer = Timer::from_seconds(SCENE_FADE_IN_SECS, TimerMode::Once);
        }
        SceneTransitionPhase::FadeIn => {
            transition.phase = SceneTransitionPhase::Idle;
            transition.target = None;
            transition.timer = Timer::from_seconds(0.0, TimerMode::Once);
            transition.timer.finish();
            if let Some(lock) = movement_lock.as_deref_mut() {
                lock.active = false;
            }
        }
    }
}

pub fn sync_scene_fade_overlay(
    transition: Res<SceneTransitionState>,
    camera_query: Query<&Transform, (With<Camera2d>, Without<SceneFadeOverlay>)>,
    mut overlay_query: Query<(&mut Sprite, &mut Transform), With<SceneFadeOverlay>>,
) {
    let Ok(camera_transform) = camera_query.single() else {
        return;
    };
    let Ok((mut sprite, mut overlay_transform)) = overlay_query.single_mut() else {
        return;
    };
    overlay_transform.translation.x = camera_transform.translation.x;
    overlay_transform.translation.y = camera_transform.translation.y;
    let alpha = match transition.phase {
        SceneTransitionPhase::Idle => 0.0,
        SceneTransitionPhase::FadeOut => {
            (transition.timer.elapsed_secs() / SCENE_FADE_OUT_SECS).clamp(0.0, 1.0)
        }
        SceneTransitionPhase::HoldBlack => 1.0,
        SceneTransitionPhase::FadeIn => {
            (1.0 - (transition.timer.elapsed_secs() / SCENE_FADE_IN_SECS)).clamp(0.0, 1.0)
        }
    };
    sprite.color = Color::srgba(0.0, 0.0, 0.0, alpha);
}

pub fn setup_bg(mut commands: Commands) {
    commands.spawn((
        SceneBackground,
        Sprite::from_color(
            scene_background_spec(TOWN_MAP_ASSET_PATH).color,
            Vec2::splat(4096.0),
        ),
        Transform::from_xyz(0.0, 0.0, -10.0),
    ));
}

pub fn sync_bg_with_camera(
    camera_query: Query<&Transform, (With<Camera2d>, Without<SceneBackground>)>,
    mut bg_query: Query<&mut Transform, With<SceneBackground>>,
) {
    let Ok(camera_tf) = camera_query.single() else {
        return;
    };
    let Ok(mut bg_tf) = bg_query.single_mut() else {
        return;
    };
    bg_tf.translation.x = camera_tf.translation.x;
    bg_tf.translation.y = camera_tf.translation.y;
}

pub fn apply_scene_background_spec(
    loaded_map: Res<LoadedMap>,
    mut bg_query: Query<&mut Sprite, With<SceneBackground>>,
) {
    let Ok(mut bg_sprite) = bg_query.single_mut() else {
        return;
    };
    bg_sprite.color = scene_background_spec(loaded_map_path(&loaded_map)).color;
}
