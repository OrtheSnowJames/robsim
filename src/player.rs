use crate::bank::render::maze::MazeTile;
use crate::collision::BoundingBox;
use crate::entity_dialogue::PlayerMovementLock;
use crate::sprite_sheet::{
    apply_animator_to_sprite, facing_and_movement_from_input, make_sprite_with_animator,
    tick_animator, Facing, SpriteSheetAnimator, SpriteSheetConfig,
};
use bevy::prelude::*;

#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub enum PlayerSystemSet {
    Move,
    Camera,
}

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_player);
        app.add_systems(Update, move_player.in_set(PlayerSystemSet::Move));
        app.add_systems(
            PostUpdate,
            follow_player_camera
                .in_set(PlayerSystemSet::Camera)
                .after(PlayerSystemSet::Move),
        );
    }
}

const PLAYER_HITBOX_DEFAULT: f32 = 13.0;
const PLAYER_HITBOX_MAZE: f32 = 8.0;
const WORLD_COLLISION_BOTTOM_MARGIN: f32 = 10.0;
pub const PLAYER_Z_LAYER: f32 = 11.0;

fn overlaps_aabb_centers(a_center: Vec2, a_size: Vec2, b_center: Vec2, b_size: Vec2) -> bool {
    let half = (a_size + b_size) * 0.5;
    let d = (a_center - b_center).abs();
    d.x < half.x && d.y < half.y
}

fn collides_with_walls(
    candidate_center: Vec2,
    player_size: Vec2,
    walls: &Query<(&GlobalTransform, &BoundingBox, Option<&MazeTile>), Without<Player>>,
) -> bool {
    for (wall_transform, wall, maze_tile) in walls {
        let mut wall_center = wall_transform.translation().truncate();
        let mut wall_size = Vec2::new(wall.width.max(1.0), wall.height.max(1.0));

        // Keep the previous "enter a bit from below" behavior for world/building collisions,
        // but use exact wall bounds inside the maze.
        if maze_tile.is_none() {
            let margin = WORLD_COLLISION_BOTTOM_MARGIN.min(wall_size.y * 0.9);
            wall_size.y = (wall_size.y - margin).max(1.0);
            wall_center.y += margin * 0.5;
        }

        if overlaps_aabb_centers(candidate_center, player_size, wall_center, wall_size) {
            return true;
        }
    }

    false
}

#[derive(Component)]
pub struct Player;

#[derive(Component)]
pub struct PlayerAnimState {
    pub sheet: SpriteSheetConfig,
    pub animator: SpriteSheetAnimator,
}

pub fn get_player_sheet(
    mut texture_atlas_layouts: ResMut<Assets<TextureAtlasLayout>>
) -> Handle<TextureAtlasLayout> {
    let sheet = SpriteSheetConfig::simple_4dir_rows(16, 3, 3, 0, 1, 2, 0.15);
    sheet.layout(&mut texture_atlas_layouts)
}

pub fn setup_player(
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut texture_atlas_layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    let sprite_image = assets.load("robber.png");
    let player_sheet = SpriteSheetConfig::simple_4dir_rows(16, 3, 3, 0, 1, 2, 0.15);
    let (sprite, animator, sheet) = make_sprite_with_animator(
        sprite_image,
        player_sheet,
        Facing::Down,
        &mut texture_atlas_layouts,
    );

    commands.spawn((
        Player,
        PlayerAnimState {
            sheet,
            animator,
        },
        sprite,
        Transform::from_xyz(0.0, -80.0, PLAYER_Z_LAYER),
    ));
}

pub fn move_player(
    time: Res<Time>,
    keyboard_input: Res<ButtonInput<KeyCode>>,
    movement_lock: Option<Res<PlayerMovementLock>>,
    mut players: Query<(&mut Transform, &mut Sprite, &mut PlayerAnimState), With<Player>>,
    walls: Query<(&GlobalTransform, &BoundingBox, Option<&MazeTile>), Without<Player>>,
    maze_tiles: Query<Entity, With<MazeTile>>,
) {
    let Ok((mut transform, mut sprite, mut anim)) = players.single_mut() else {
        return;
    };

    let in_maze = maze_tiles.iter().next().is_some();
    let player_bounds = Vec2::splat(if in_maze {
        PLAYER_HITBOX_MAZE
    } else {
        PLAYER_HITBOX_DEFAULT
    });

    let mut direction = Vec2::ZERO;
    let locked = movement_lock.as_deref().map(|l| l.active).unwrap_or(false);
    if !locked {
        if keyboard_input.pressed(KeyCode::KeyW) || keyboard_input.pressed(KeyCode::ArrowUp) {
            direction.y += 1.0;
        }
        if keyboard_input.pressed(KeyCode::KeyS) || keyboard_input.pressed(KeyCode::ArrowDown) {
            direction.y -= 1.0;
        }
        if keyboard_input.pressed(KeyCode::KeyA) || keyboard_input.pressed(KeyCode::ArrowLeft) {
            direction.x -= 1.0;
        }
        if keyboard_input.pressed(KeyCode::KeyD) || keyboard_input.pressed(KeyCode::ArrowRight) {
            direction.x += 1.0;
        }
    }

    let is_walking = direction != Vec2::ZERO;
    if is_walking {
        let speed = 90.0;
        let delta = direction.normalize() * speed * time.delta_secs();
        let mut next_center = transform.translation.truncate();

        if delta.x != 0.0 {
            let candidate = Vec2::new(next_center.x + delta.x, next_center.y);
            if !collides_with_walls(candidate, player_bounds, &walls) {
                next_center.x = candidate.x;
            }
        }

        if delta.y != 0.0 {
            let candidate = Vec2::new(next_center.x, next_center.y + delta.y);
            if !collides_with_walls(candidate, player_bounds, &walls) {
                next_center.y = candidate.y;
            }
        }

        transform.translation.x = next_center.x;
        transform.translation.y = next_center.y;
    }

    if !locked {
        let (new_facing, walking, flip_x) = facing_and_movement_from_input(&keyboard_input);
        anim.animator.facing = new_facing;
        anim.animator.walking = walking;
        sprite.flip_x = flip_x;
    } else {
        anim.animator.walking = false;
    }
    let walk_frames = anim.sheet.walk_frames();
    tick_animator(
        &mut anim.animator,
        time.delta_secs(),
        walk_frames,
    );
    apply_animator_to_sprite(&mut sprite, &anim.sheet, &anim.animator);
}

pub fn follow_player_camera(
    mut camera_query: Query<&mut Transform, (With<Camera2d>, Without<Player>)>,
    player_query: Query<&Transform, With<Player>>,
) {
    let Ok(player_transform) = player_query.single() else {
        return;
    };
    let Ok(mut camera_transform) = camera_query.single_mut() else {
        return;
    };

    camera_transform.translation.x = player_transform.translation.x;
    camera_transform.translation.y = player_transform.translation.y;
}
