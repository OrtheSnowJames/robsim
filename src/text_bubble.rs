use bevy::prelude::*;

const BUBBLE_Z: f32 = 40.0;
const BUBBLE_SCALE: f32 = 0.8;
const BUBBLE_MIN_WIDTH: f32 = 16.0 * BUBBLE_SCALE;
const BUBBLE_MIN_HEIGHT: f32 = 12.0 * BUBBLE_SCALE;
const BUBBLE_CHAR_WIDTH: f32 = 6.0 * BUBBLE_SCALE;
const BUBBLE_TEXT_SIZE: f32 = 9.0 * BUBBLE_SCALE;
const BUBBLE_MAX_CHARS_PER_LINE: usize = 28;
const BUBBLE_LINE_HEIGHT: f32 = 10.0 * BUBBLE_SCALE;
const BUBBLE_NINE_SLICE_INSET: f32 = 6.0;
const BUBBLE_STACK_GAP: f32 = 3.0;

pub struct TextBubblePlugin;

impl Plugin for TextBubblePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (spawn_text_bubbles, sync_text_bubbles, cleanup_text_bubbles),
        );
    }
}

#[derive(Component, Clone)]
pub struct TextBubble {
    pub message: String,
    pub offset: Vec2,
    pub visible: bool,
}

impl TextBubble {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            // Bottom-left anchor point in world space.
            offset: Vec2::new(-10.0, 10.0),
            visible: true,
        }
    }
}

#[derive(Component)]
struct TextBubbleNodes {
    root: Entity,
    bubble: Entity,
    label: Entity,
}

fn bubble_size(message: &str) -> Vec2 {
    let wrapped = wrap_text(message, BUBBLE_MAX_CHARS_PER_LINE);
    let lines: Vec<&str> = wrapped.lines().collect();
    let max_line_chars = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0) as f32;
    let width = (max_line_chars * BUBBLE_CHAR_WIDTH + 10.0).max(BUBBLE_MIN_WIDTH);
    let height = ((lines.len().max(1) as f32) * BUBBLE_LINE_HEIGHT + 6.0).max(BUBBLE_MIN_HEIGHT);
    Vec2::new(width, height)
}

fn wrap_text(message: &str, max_chars_per_line: usize) -> String {
    let mut out = String::new();
    let mut current_len = 0usize;

    for (i, raw_line) in message.split('\n').enumerate() {
        if i > 0 {
            out.push('\n');
            current_len = 0;
        }

        for word in raw_line.split_whitespace() {
            let word_len = word.chars().count();
            let sep = if current_len == 0 { 0 } else { 1 };
            if current_len + sep + word_len > max_chars_per_line && current_len > 0 {
                out.push('\n');
                out.push_str(word);
                current_len = word_len;
            } else {
                if sep == 1 {
                    out.push(' ');
                }
                out.push_str(word);
                current_len += sep + word_len;
            }
        }
    }

    out
}

fn bubble_root_translation(owner: Entity, bubble: &TextBubble, size: Vec2) -> Vec3 {
    let owner_idx = owner.to_bits();
    let side_sign = if owner_idx % 2 == 0 { 1.0 } else { -1.0 };
    let x = bubble.offset.x + size.x * 0.5 * side_sign;
    let y = bubble.offset.y + size.y * 0.5;
    Vec3::new(x, y, BUBBLE_Z)
}

fn bubble_on_left(owner: Entity) -> bool {
    owner.to_bits() % 2 != 0
}

fn spawn_text_bubbles(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    query: Query<(Entity, &TextBubble), Without<TextBubbleNodes>>,
) {
    for (owner, bubble) in &query {
        let wrapped = wrap_text(&bubble.message, BUBBLE_MAX_CHARS_PER_LINE);
        let size = bubble_size(&wrapped);
        let flip_x = bubble_on_left(owner);

        let root_pos = bubble_root_translation(owner, bubble, size);
        let root = commands
            .spawn((Transform::from_translation(root_pos), Visibility::Inherited))
            .id();

        let bubble_sprite = commands
            .spawn((
                Sprite {
                    image: asset_server.load("bubble.png"),
                    custom_size: Some(size),
                    flip_x,
                    image_mode: SpriteImageMode::Sliced(TextureSlicer {
                        border: BorderRect::all(BUBBLE_NINE_SLICE_INSET),
                        center_scale_mode: SliceScaleMode::Stretch,
                        sides_scale_mode: SliceScaleMode::Stretch,
                        max_corner_scale: 1.0,
                    }),
                    ..default()
                },
                Transform::from_xyz(0.0, 0.0, 0.0),
            ))
            .id();

        let label = commands
            .spawn((
                Text2d::new(wrapped),
                TextFont {
                    font_size: BUBBLE_TEXT_SIZE,
                    ..default()
                },
                TextColor(Color::WHITE),
                TextLayout::new_with_justify(Justify::Left),
                Transform::from_xyz(0.0, -0.5, 0.1),
            ))
            .id();

        commands.entity(root).add_children(&[bubble_sprite, label]);
        commands.entity(owner).add_child(root);
        commands.entity(owner).insert(TextBubbleNodes {
            root,
            bubble: bubble_sprite,
            label,
        });
    }
}

fn sync_text_bubbles(
    mut root_query: Query<(&mut Transform, &mut Visibility)>,
    mut sprite_query: Query<&mut Sprite>,
    mut text_query: Query<&mut Text2d>,
    owner_global_query: Query<&GlobalTransform>,
    bubble_query: Query<(Entity, &TextBubble, &TextBubbleNodes)>,
) {
    let mut layout: Vec<(Entity, Entity, bool, Vec2, Vec2)> = Vec::new();
    for (owner, bubble, nodes) in &bubble_query {
        let wrapped = wrap_text(&bubble.message, BUBBLE_MAX_CHARS_PER_LINE);
        let size = bubble_size(&wrapped);
        let owner_world = owner_global_query
            .get(owner)
            .map(|t| t.translation().truncate())
            .unwrap_or(Vec2::ZERO);
        let desired_local = bubble_root_translation(owner, bubble, size).truncate();
        let desired_world = owner_world + desired_local;
        layout.push((owner, nodes.root, bubble.visible, size, desired_world));

        if let Ok(mut label_text) = text_query.get_mut(nodes.label) {
            label_text.0 = wrapped;
        }
        if let Ok(mut bubble_sprite) = sprite_query.get_mut(nodes.bubble) {
            bubble_sprite.custom_size = Some(size);
            bubble_sprite.flip_x = bubble_on_left(owner);
        }
    }

    layout.sort_by(|a, b| {
        a.4.y
            .partial_cmp(&b.4.y)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut placed: Vec<(Vec2, Vec2)> = Vec::new();

    for (owner, root, visible, size, mut world_pos) in layout {
        let Ok((mut root_tf, mut root_vis)) = root_query.get_mut(root) else {
            continue;
        };
        *root_vis = if visible {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        if !visible {
            continue;
        }

        loop {
            let mut bumped = false;
            for (other_pos, other_size) in &placed {
                let half = (size + *other_size) * 0.5;
                let d = (world_pos - *other_pos).abs();
                if d.x < half.x && d.y < half.y {
                    world_pos.y = other_pos.y + half.y + BUBBLE_STACK_GAP;
                    bumped = true;
                }
            }
            if !bumped {
                break;
            }
        }

        let owner_world = owner_global_query
            .get(owner)
            .map(|t| t.translation().truncate())
            .unwrap_or(Vec2::ZERO);
        let local = world_pos - owner_world;
        root_tf.translation = Vec3::new(local.x, local.y, BUBBLE_Z);
        placed.push((world_pos, size));
    }
}

fn cleanup_text_bubbles(
    mut commands: Commands,
    query: Query<(Entity, &TextBubbleNodes), Without<TextBubble>>,
) {
    for (entity, nodes) in &query {
        commands.entity(nodes.root).try_despawn();
        commands.entity(entity).remove::<TextBubbleNodes>();
    }
}
