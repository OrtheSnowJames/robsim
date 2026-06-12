use crate::bank::img_layer::LockGlobalZ;
use crate::bank::render::maze::MazeTile;
use crate::collision::BoundingBox;
use crate::entity_dialogue::PlayerMovementLock;
use crate::map::scene::MainCamera;
use crate::sprite_sheet::{
    Facing, SpriteSheetAnimator, SpriteSheetConfig, apply_animator_to_sprite,
    facing_and_movement_from_input, make_sprite_with_animator, tick_animator,
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
        app.add_systems(Update, check_global_z);
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
pub struct LocalPlayer;

#[derive(Component)]
pub struct RemotePlayer;

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerIdentity {
    pub id: u64,
    pub is_host: bool,
}

#[derive(Component)]
pub struct PlayerAnimState {
    pub sheet: SpriteSheetConfig,
    pub animator: SpriteSheetAnimator,
}

pub fn get_player_sheet(
    mut texture_atlas_layouts: ResMut<Assets<TextureAtlasLayout>>,
) -> Handle<TextureAtlasLayout> {
    let sheet = SpriteSheetConfig::simple_4dir_rows(16, 3, 3, 0, 1, 2, 0.15);
    sheet.layout(&mut texture_atlas_layouts)
}

pub fn setup_player(
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut texture_atlas_layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    spawn_player_entity(
        &mut commands,
        assets.as_ref(),
        texture_atlas_layouts.as_mut(),
        Vec3::new(0.0, -80.0, PLAYER_Z_LAYER),
        Some((0, false)),
        true,
    );
}

pub fn spawn_player_entity(
    commands: &mut Commands,
    assets: &AssetServer,
    texture_atlas_layouts: &mut Assets<TextureAtlasLayout>,
    translation: Vec3,
    identity: Option<(u64, bool)>,
    local: bool,
) -> Entity {
    let sprite_image = assets.load("robber.png");
    let player_sheet = SpriteSheetConfig::simple_4dir_rows(16, 3, 3, 0, 1, 2, 0.15);
    let (mut sprite, animator, sheet) = make_sprite_with_animator(
        sprite_image,
        player_sheet,
        Facing::Down,
        texture_atlas_layouts,
    );

    if identity.map(|(_, is_host)| is_host).unwrap_or(false) {
        sprite.color = Color::srgb(1.0, 0.76, 0.18);
    }

    let mut entity = commands.spawn((
        Player,
        PlayerAnimState { sheet, animator },
        sprite,
        Transform::from_translation(translation),
    ));

    if let Some((id, is_host)) = identity {
        entity.insert(PlayerIdentity { id, is_host });
    }

    if local {
        entity.insert(LocalPlayer);
    } else {
        entity.insert(RemotePlayer);
    }

    entity.id()
}

pub fn move_player(
    time: Res<Time>,
    keyboard_input: Res<ButtonInput<KeyCode>>,
    movement_lock: Option<Res<PlayerMovementLock>>,
    mut players: Query<
        (&mut Transform, &mut Sprite, &mut PlayerAnimState),
        (With<Player>, With<LocalPlayer>),
    >,
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
    tick_animator(&mut anim.animator, time.delta_secs(), walk_frames);
    apply_animator_to_sprite(&mut sprite, &anim.sheet, &anim.animator);
}

pub fn follow_player_camera(
    mut camera_query: Query<&mut Transform, (With<Camera2d>, With<MainCamera>, Without<Player>)>,
    player_query: Query<&Transform, (With<Player>, With<LocalPlayer>)>,
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

fn check_global_z(
    player_query: Query<(Entity, &GlobalTransform, Option<&Name>), With<Player>>,
    named_query: Query<(Entity, &GlobalTransform, Option<&Name>), Without<Player>>,
    local_tf_query: Query<&Transform>,
    lock_z_query: Query<&LockGlobalZ>,
    parent_query: Query<&ChildOf>,
    parent_global_query: Query<&GlobalTransform>,
    children_query: Query<&Children>,
    keyboard_input: Res<ButtonInput<KeyCode>>,
) {
    if !keyboard_input.pressed(KeyCode::Equal) {
        return;
    }

    for (entity, global_transform, name) in player_query.iter() {
        let name_str = name.map(|n| n.as_str()).unwrap_or("Player");
        let global_z = global_transform.translation().z;
        println!(
            "Entity: {:?} ({}), Global Z: {}, Local Z: {}",
            entity,
            name_str,
            global_z,
            local_tf_query
                .get(entity)
                .map(|t| t.translation.z)
                .unwrap_or(0.0)
        );
    }

    // Also print likely bars/tileset actors so their render order can be debugged.
    for (entity, global_transform, name) in named_query.iter() {
        let Some(name) = name else {
            continue;
        };
        let name_str = name.as_str();
        let lower = name_str.to_ascii_lowercase();
        if !lower.contains("bar")
            && !lower.contains("jail")
            && !lower.contains("tile")
            && !lower.contains("layer")
        {
            continue;
        }

        let global_z = global_transform.translation().z;
        let local_z = local_tf_query
            .get(entity)
            .map(|t| t.translation.z)
            .unwrap_or(0.0);
        let lock_target = lock_z_query.get(entity).ok().map(|z| z.0);
        let parent_info = parent_query.get(entity).ok().and_then(|p| {
            let parent_e = p.parent();
            parent_global_query
                .get(parent_e)
                .ok()
                .map(|pg| (parent_e, pg.translation().z))
        });

        println!(
            "Entity: {:?} ({}), Global Z: {}, Local Z: {}, LockGlobalZ: {:?}, Parent: {:?}",
            entity, name_str, global_z, local_z, lock_target, parent_info
        );

        if let Some((parent_e, parent_global_z)) = parent_info {
            let expected_global_z = parent_global_z + local_z;
            println!(
                "  Parent {:?} Global Z: {}, expected child global from local: {}",
                parent_e, parent_global_z, expected_global_z
            );

            if let Ok(grand_parent) = parent_query.get(parent_e) {
                let gp_e = grand_parent.parent();
                if let Ok(gp_gt) = parent_global_query.get(gp_e) {
                    println!(
                        "  GrandParent {:?} Global Z: {}",
                        gp_e,
                        gp_gt.translation().z
                    );
                }
            }
        }

        if let Ok(children) = children_query.get(entity) {
            println!("  Children count: {}", children.len());
        }
    }
}
