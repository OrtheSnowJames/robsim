use bevy::prelude::*;

use crate::map::scene::MainCamera;
use crate::player::{LocalPlayer, Player};

#[derive(Component)]
pub struct KeycapPrompt;

#[derive(Component, Clone)]
pub struct KeyPrompt {
    pub key: String,
    pub radius: f32,
    pub world_offset: Vec2,
    pub half_extents: Vec2,
}

impl KeyPrompt {
    pub fn new(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            radius: 26.0,
            world_offset: Vec2::ZERO,
            half_extents: Vec2::ZERO,
        }
    }
}

#[derive(Component)]
struct KeycapPromptAnim {
    alpha: f32,
    target_alpha: f32,
    bob_phase: f32,
}

pub struct PromptKeyPlugin;

impl Plugin for PromptKeyPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_space_prompt);
        app.add_systems(Update, update_key_prompt_ui);
    }
}

fn setup_space_prompt(mut commands: Commands) {
    commands
        .spawn((
            KeycapPrompt,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                width: Val::Px(72.0),
                height: Val::Px(34.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                border: UiRect::all(Val::Px(2.0)),
                border_radius: BorderRadius::all(Val::Px(8.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.07, 0.07, 0.09, 0.9)),
            BorderColor::all(Color::srgba(0.95, 0.95, 1.0, 0.9)),
            Visibility::Hidden,
            ZIndex(999),
            KeycapPromptAnim {
                alpha: 0.0,
                target_alpha: 0.0,
                bob_phase: 0.0,
            },
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new("SPACE"),
                TextFont {
                    font_size: 14.0,
                    ..default()
                },
                TextColor(Color::WHITE),
            ));
        });
}

fn update_key_prompt_ui(
    time: Res<Time>,
    player_query: Query<&Transform, (With<Player>, With<LocalPlayer>, Without<KeycapPrompt>)>,
    key_prompt_query: Query<(&GlobalTransform, &KeyPrompt), Without<Player>>,
    camera_query: Query<(&Camera, &GlobalTransform), (With<Camera2d>, With<MainCamera>)>,
    mut prompt_query: Query<
        (
            &mut Node,
            &mut Visibility,
            &mut BackgroundColor,
            &mut BorderColor,
            &mut KeycapPromptAnim,
            &Children,
        ),
        With<KeycapPrompt>,
    >,
    mut text_query: Query<(&mut Text, &mut TextColor)>,
) {
    let Ok(player_tf) = player_query.single() else {
        return;
    };
    let Ok((camera, camera_tf)) = camera_query.single() else {
        return;
    };
    let Ok((mut prompt_node, mut prompt_vis, mut bg, mut border, mut anim, children)) =
        prompt_query.single_mut()
    else {
        return;
    };

    let player_pos = player_tf.translation.truncate();
    let mut active_prompt: Option<(&str, Vec3)> = None;
    let mut best_dist = f32::MAX;
    for (tf, key_prompt) in &key_prompt_query {
        let prompt_pos = tf.translation() + key_prompt.world_offset.extend(0.0);
        let d = distance_to_box(player_pos, prompt_pos.truncate(), key_prompt.half_extents);
        if d <= key_prompt.radius && d < best_dist {
            best_dist = d;
            active_prompt = Some((&key_prompt.key, prompt_pos));
        }
    }

    anim.target_alpha = if active_prompt.is_some() { 1.0 } else { 0.0 };
    let fade_speed = 9.0;
    anim.alpha = anim.alpha
        + (anim.target_alpha - anim.alpha) * (1.0 - (-fade_speed * time.delta_secs()).exp());
    anim.alpha = anim.alpha.clamp(0.0, 1.0);
    anim.bob_phase += time.delta_secs() * 4.5;

    let world_prompt_pos = active_prompt
        .map(|(_, pos)| pos + Vec3::new(-14.0, 20.0, 0.0))
        .unwrap_or(player_tf.translation + Vec3::new(-14.0, 20.0, 0.0));
    if let Ok(screen_pos) = camera.world_to_viewport(camera_tf, world_prompt_pos) {
        prompt_node.left = Val::Px(screen_pos.x - 36.0);
        let bob = anim.bob_phase.sin() * 2.0 * anim.alpha;
        let fade_slide = (1.0 - anim.alpha) * 6.0;
        prompt_node.top = Val::Px(screen_pos.y - 17.0 + bob + fade_slide);

        let bg_alpha = 0.9 * anim.alpha;
        let border_alpha = 0.9 * anim.alpha;
        bg.0 = Color::srgba(0.07, 0.07, 0.09, bg_alpha);
        let border_color = Color::srgba(0.95, 0.95, 1.0, border_alpha);
        border.top = border_color;
        border.right = border_color;
        border.bottom = border_color;
        border.left = border_color;
        for child in children.iter() {
            if let Ok((mut text, mut text_color)) = text_query.get_mut(child) {
                if let Some((key, _)) = active_prompt {
                    text.0 = key.to_string();
                }
                text_color.0 = Color::srgba(1.0, 1.0, 1.0, anim.alpha);
            }
        }

        *prompt_vis = if anim.alpha > 0.02 {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    } else {
        *prompt_vis = Visibility::Hidden;
    }
}

fn distance_to_box(point: Vec2, center: Vec2, half_extents: Vec2) -> f32 {
    let dx = (point.x - center.x).abs() - half_extents.x.max(0.0);
    let dy = (point.y - center.y).abs() - half_extents.y.max(0.0);
    Vec2::new(dx.max(0.0), dy.max(0.0)).length()
}
