use bevy::prelude::*;
use serde_json::Value;

use crate::collision::BoundingBox;
use crate::entity_dialogue::DisabledEntityDialogues;
use crate::map::{LdtkEntityByNameQuery, LoadedMap, loaded_map_path};
use crate::multiplayer::MultiplayerSession;
use crate::player::{LocalPlayer, Player};
use crate::random::random_range_usize;
use crate::text_bubble::TextBubble;

#[derive(Component)]
pub struct TellerSprite;

#[derive(Component)]
pub struct VaultSprite;

#[derive(Resource, Default)]
pub struct BankHeistState {
    pub vault_opened: bool,
}

fn random_rob_dialogue(asset_server: &AssetServer) -> (String, String) {
    let _ = asset_server;
    let cleaned = include_str!("../../assets/lines.jsonc")
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");

    let Ok(json) = serde_json::from_str::<Value>(&cleaned) else {
        return ("Hands Up!".to_string(), "O-Okay, okay!".to_string());
    };
    let Some(rob_lines) = json.get("rob_lines").and_then(Value::as_array) else {
        return ("Hands Up!".to_string(), "O-Okay, okay!".to_string());
    };
    if rob_lines.is_empty() {
        return ("Hands Up!".to_string(), "O-Okay, okay!".to_string());
    }

    let idx = random_range_usize(0..rob_lines.len());
    let picked = &rob_lines[idx];
    let player_line = picked
        .get("1")
        .and_then(Value::as_str)
        .unwrap_or("Hands Up!")
        .to_string();
    let teller_line = picked
        .get("2")
        .and_then(Value::as_str)
        .unwrap_or("O-Okay, okay!")
        .to_string();
    (player_line, teller_line)
}

pub fn trigger_bank_heist(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    player_entity_q: Query<Entity, (With<Player>, With<LocalPlayer>)>,
    loaded_map: Res<LoadedMap>,
    multiplayer_session: Option<Res<MultiplayerSession>>,
    mut heist_state: ResMut<BankHeistState>,
    mut disabled_dialogues: ResMut<DisabledEntityDialogues>,
    assets: Res<AssetServer>,
    ldtk_entities: LdtkEntityByNameQuery,
    mut sprites: Query<&mut Sprite>,
    mut commands: Commands,
) {
    if loaded_map_path(&loaded_map) != "maps/bank.ldtk" {
        heist_state.vault_opened = false;
        disabled_dialogues.clear();
        return;
    }

    if heist_state.vault_opened || !keyboard_input.just_pressed(KeyCode::KeyF) {
        return;
    }

    if multiplayer_session
        .as_deref()
        .map(|session| session.is_connected() && !session.local_is_host())
        .unwrap_or(false)
    {
        return;
    }

    let threatened = assets.load::<Image>("bank/bank_teller_threatened.png");
    let (player_line, teller_line) = random_rob_dialogue(assets.as_ref());
    for teller_name in ["Bear", "Teller", "Bank_teller"] {
        for (entity, _, _) in ldtk_entities.iter_named(teller_name) {
            if let Ok(mut sprite) = sprites.get_mut(entity) {
                sprite.image = threatened.clone();
            }
            commands.entity(entity).insert(TextBubble {
                message: teller_line.clone(),
                offset: Vec2::new(0.0, 22.0),
                visible: true,
            });
        }
    }

    disabled_dialogues.disable("Bear");

    if let Ok(player_entity) = player_entity_q.single() {
        commands.entity(player_entity).insert(TextBubble {
            message: player_line,
            offset: Vec2::new(0.0, 22.0),
            visible: true,
        });
    }

    for (entity, _, _) in ldtk_entities.iter_named("Vault") {
        let Ok(mut sprite) = sprites.get_mut(entity) else {
            continue;
        };
        commands.entity(entity).remove::<BoundingBox>();
        sprite.color = Color::BLACK;
    }

    heist_state.vault_opened = true;
}
