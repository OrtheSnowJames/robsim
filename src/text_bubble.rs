use bevy::prelude::*;

const BUBBLE_Z: f32 = 40.0;
const BUBBLE_SCALE: f32 = 0.75;
const BUBBLE_BORDER_THICKNESS: f32 = 1.5 * BUBBLE_SCALE;
const BUBBLE_MIN_WIDTH: f32 = 16.0 * BUBBLE_SCALE;
const BUBBLE_MIN_HEIGHT: f32 = 12.0 * BUBBLE_SCALE;
const BUBBLE_CHAR_WIDTH: f32 = 6.0 * BUBBLE_SCALE;
const BUBBLE_TEXT_SIZE: f32 = 9.0 * BUBBLE_SCALE;
const BUBBLE_MAX_CHARS_PER_LINE: usize = 28;
const BUBBLE_LINE_HEIGHT: f32 = 10.0 * BUBBLE_SCALE;

pub struct TextBubblePlugin;

impl Plugin for TextBubblePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                spawn_text_bubbles,
                sync_text_bubbles,
                cleanup_text_bubbles,
            ),
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
    border: Entity,
    fill: Entity,
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

fn spawn_text_bubbles(
    mut commands: Commands,
    query: Query<(Entity, &TextBubble), Without<TextBubbleNodes>>,
) {
    for (owner, bubble) in &query {
        let wrapped = wrap_text(&bubble.message, BUBBLE_MAX_CHARS_PER_LINE);
        let size = bubble_size(&wrapped);

        let root = commands
            .spawn((
                Transform::from_xyz(
                    bubble.offset.x + size.x * 0.5,
                    bubble.offset.y + size.y * 0.5,
                    BUBBLE_Z,
                ),
                Visibility::Inherited,
            ))
            .id();

        let border = commands
            .spawn((
                Sprite::from_color(
                    Color::BLACK,
                    size + Vec2::splat(BUBBLE_BORDER_THICKNESS * 2.0),
                ),
                Transform::from_xyz(0.0, 0.0, 0.0),
            ))
            .id();

        let fill = commands
            .spawn((
                Sprite::from_color(Color::srgba(0.0, 0.0, 0.0, 0.92), size),
                Transform::from_xyz(0.0, 0.0, 0.1),
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
                Transform::from_xyz(0.0, -0.5, 0.2),
            ))
            .id();

        commands.entity(root).add_children(&[border, fill, label]);
        commands.entity(owner).add_child(root);
        commands.entity(owner).insert(TextBubbleNodes {
            root,
            border,
            fill,
            label,
        });
    }
}

fn sync_text_bubbles(
    mut root_query: Query<(&mut Transform, &mut Visibility)>,
    mut sprite_query: Query<&mut Sprite>,
    mut text_query: Query<&mut Text2d>,
    bubble_query: Query<(&TextBubble, &TextBubbleNodes)>,
) {
    for (bubble, nodes) in &bubble_query {
        let wrapped = wrap_text(&bubble.message, BUBBLE_MAX_CHARS_PER_LINE);
        let size = bubble_size(&wrapped);
        let Ok((mut root_tf, mut root_vis)) = root_query.get_mut(nodes.root) else {
            continue;
        };
        root_tf.translation.x = bubble.offset.x + size.x * 0.5;
        root_tf.translation.y = bubble.offset.y + size.y * 0.5;
        *root_vis = if bubble.visible {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        if let Ok(mut border_sprite) = sprite_query.get_mut(nodes.border) {
            border_sprite.custom_size = Some(size + Vec2::splat(BUBBLE_BORDER_THICKNESS * 2.0));
        }
        if let Ok(mut fill_sprite) = sprite_query.get_mut(nodes.fill) {
            fill_sprite.custom_size = Some(size);
        }
        if let Ok(mut label_text) = text_query.get_mut(nodes.label) {
            label_text.0 = wrapped;
        }
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
