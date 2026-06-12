use bevy::prelude::*;
use bevy_ecs_ldtk::EntityInstance;

use crate::player::{LocalPlayer, Player};
use crate::prompt_key::KeyPrompt;

pub const CALLBACK_OPEN_DISPLAY: &str = "open_display";
pub const CALLBACK_OPEN_RECEIPTS: &str = "open_receipts";
pub const DISPLAY_ENTITY_IDENTIFIER: &str = "Newspapers";
pub const CHEST_ENTITY_IDENTIFIER: &str = "Chest";
pub const DISPLAY_INTERACT_RADIUS: f32 = 20.0;
pub const CHEST_INTERACT_RADIUS: f32 = 26.0;

#[derive(Message, Clone, Copy)]
pub enum EnterInteractCallbackEvent {
    OpenDisplay(Entity),
    OpenReceipts(Entity),
}

type EnterInteractCallback =
    fn(entity: Entity, writer: &mut MessageWriter<EnterInteractCallbackEvent>);

pub struct EnterInteractSpec {
    pub identifier: &'static str,
    pub radius: f32,
    pub world_offset: Vec2,
    pub callback_key: &'static str,
    pub callback: EnterInteractCallback,
}

fn emit_open_display(entity: Entity, writer: &mut MessageWriter<EnterInteractCallbackEvent>) {
    writer.write(EnterInteractCallbackEvent::OpenDisplay(entity));
}

fn emit_open_receipts(entity: Entity, writer: &mut MessageWriter<EnterInteractCallbackEvent>) {
    writer.write(EnterInteractCallbackEvent::OpenReceipts(entity));
}

const ENTER_INTERACT_SPECS: &[EnterInteractSpec] = &[
    EnterInteractSpec {
        identifier: DISPLAY_ENTITY_IDENTIFIER,
        radius: DISPLAY_INTERACT_RADIUS,
        world_offset: Vec2::ZERO,
        callback_key: CALLBACK_OPEN_DISPLAY,
        callback: emit_open_display,
    },
    EnterInteractSpec {
        identifier: CHEST_ENTITY_IDENTIFIER,
        radius: CHEST_INTERACT_RADIUS,
        world_offset: Vec2::ZERO,
        callback_key: CALLBACK_OPEN_RECEIPTS,
        callback: emit_open_receipts,
    },
];

#[derive(Component)]
struct EnterInteractTrigger {
    spec_idx: usize,
    radius: f32,
    world_offset: Vec2,
    half_extents: Vec2,
}

pub struct EnterInteractPlugin;

impl Plugin for EnterInteractPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<EnterInteractCallbackEvent>().add_systems(
            Update,
            (attach_enter_interact_triggers, handle_enter_interact),
        );
    }
}

fn find_spec_index(identifier: &str) -> Option<usize> {
    ENTER_INTERACT_SPECS
        .iter()
        .position(|spec| spec.identifier.eq_ignore_ascii_case(identifier))
}

fn attach_enter_interact_triggers(
    mut commands: Commands,
    query: Query<(Entity, &EntityInstance), (Added<EntityInstance>, Without<EnterInteractTrigger>)>,
) {
    for (entity, instance) in &query {
        let Some(spec_idx) = find_spec_index(&instance.identifier) else {
            continue;
        };
        let spec = &ENTER_INTERACT_SPECS[spec_idx];
        let half_extents = Vec2::new(
            (instance.width.max(1) as f32) * 0.5,
            (instance.height.max(1) as f32) * 0.5,
        );
        commands.entity(entity).insert((
            EnterInteractTrigger {
                spec_idx,
                radius: spec.radius,
                world_offset: spec.world_offset,
                half_extents,
            },
            KeyPrompt {
                key: "ENTER".to_string(),
                radius: spec.radius,
                world_offset: spec.world_offset,
                half_extents,
            },
        ));
    }
}

fn handle_enter_interact(
    keyboard: Res<ButtonInput<KeyCode>>,
    player_q: Query<&Transform, (With<Player>, With<LocalPlayer>)>,
    trigger_q: Query<(Entity, &GlobalTransform, &EnterInteractTrigger)>,
    mut callback_writer: MessageWriter<EnterInteractCallbackEvent>,
) {
    let pressed_enter =
        keyboard.just_pressed(KeyCode::Enter) || keyboard.just_pressed(KeyCode::NumpadEnter);
    if !pressed_enter {
        return;
    }
    let Ok(player_tf) = player_q.single() else {
        return;
    };
    let player_pos = player_tf.translation.truncate();

    let mut best: Option<(f32, Entity, &EnterInteractTrigger)> = None;
    for (entity, tf, trigger) in &trigger_q {
        let prompt_pos = tf.translation().truncate() + trigger.world_offset;
        let dist = distance_to_box(player_pos, prompt_pos, trigger.half_extents);
        if dist > trigger.radius {
            continue;
        }
        match best {
            Some((best_dist, _, _)) if dist >= best_dist => {}
            _ => best = Some((dist, entity, trigger)),
        }
    }

    if let Some((_, entity, trigger)) = best {
        let spec = &ENTER_INTERACT_SPECS[trigger.spec_idx];
        let _ = spec.callback_key;
        (spec.callback)(entity, &mut callback_writer);
    }
}

fn distance_to_box(point: Vec2, center: Vec2, half_extents: Vec2) -> f32 {
    let dx = (point.x - center.x).abs() - half_extents.x.max(0.0);
    let dy = (point.y - center.y).abs() - half_extents.y.max(0.0);
    Vec2::new(dx.max(0.0), dy.max(0.0)).length()
}
