use bevy::{color::palettes::css::*, math::Isometry2d, prelude::*};

pub struct CollisionPlugin;

impl Plugin for CollisionPlugin {
    fn build(&self, _app: &mut App) {
        if BOUNDING_GIZMOS {
            _app.add_systems(Update, display_boxes);
        }
    }
}

const BOTTOM_MARGIN: f32 = 5.0;
const BOUNDING_GIZMOS: bool = false;

#[derive(Component, Clone, Copy)]
pub struct BoundingBox {
    pub width: f32,
    pub height: f32,
}

impl BoundingBox {
    pub fn to_vec2(self) -> Vec2 {
        Vec2::new(self.width, self.height)
    }
}

#[derive(Clone, Copy)]
pub struct CanMove {
    pub w: bool,
    pub s: bool,
    pub a: bool,
    pub d: bool,
}

pub fn can_move(
    wall: BoundingBox,
    wall_pos: Vec2,
    player_pos: Vec2,
    player_bounds: Vec2,
) -> CanMove {
    let mut movement = CanMove {
        w: true,
        s: true,
        a: true,
        d: true,
    };

    // Bounds are provided as bottom-left + size in world coordinates.
    let wall_left = wall_pos.x;
    let wall_right = wall_pos.x + wall.width;
    let wall_bottom = wall_pos.y - BOTTOM_MARGIN;
    let wall_top = wall_pos.y + wall.height;

    // Player bounds are also bottom-left + size.
    let player_left = player_pos.x;
    let player_right = player_pos.x + player_bounds.x;
    let player_bottom = player_pos.y;
    let player_top = player_pos.y + player_bounds.y;

    // Axis-aligned overlap test.
    let overlapping = player_right > wall_left
        && player_left < wall_right
        && player_top > wall_bottom
        && player_bottom < wall_top;

    if overlapping {
        let overlap_x = (player_right.min(wall_right) - player_left.max(wall_left)).max(0.0);
        let overlap_y = (player_top.min(wall_top) - player_bottom.max(wall_bottom)).max(0.0);
        let player_center = Vec2::new(
            player_left + (player_bounds.x * 0.5),
            player_bottom + (player_bounds.y * 0.5),
        );
        let wall_center = Vec2::new(
            wall_left + (wall.width * 0.5),
            wall_bottom + (wall.height * 0.5),
        );

        // Resolve along the axis with smallest penetration.
        if overlap_x < overlap_y {
            // Horizontal collision.
            if player_center.x < wall_center.x {
                movement.d = false;
            } else {
                movement.a = false;
            }
        } else {
            // Vertical collision.
            if player_center.y < wall_center.y {
                movement.w = false;
            } else {
                movement.s = false;
            }
        }
    }

    // Extra conservative side checks for edge cases near corners.
    if overlapping {
        if player_right > wall_left && player_left < wall_left {
            movement.d = false;
        }
        if player_left < wall_right && player_right > wall_right {
            movement.a = false;
        }
        if player_top > wall_bottom && player_bottom < wall_bottom {
            movement.w = false;
        }
        if player_bottom < wall_top && player_top > wall_top {
            movement.s = false;
        }
    }

    movement
}

pub fn display_boxes(box_q: Query<(&Transform, &BoundingBox)>, mut gizmos: Gizmos) {
    for _box in box_q {
        gizmos.rect_2d(
            Isometry2d::from_translation(_box.0.translation.truncate()),
            _box.1.to_vec2(),
            RED,
        );
    }
}
