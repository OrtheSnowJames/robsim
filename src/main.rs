use bevy::prelude::*;
use robsim::bank::guard::{update_guards, GuardAlertState};
use robsim::bank::img_layer::{
    change_bank_img, materialize_ldtk_entity_sprites, BankIcon,
};
use robsim::bank::render::maze::{collect_coins, update_maze_lighting};
use robsim::bank::teller::{trigger_bank_heist, BankHeistState};
use robsim::bank::BankPlugin;
use robsim::collision::CollisionPlugin;
use robsim::entity_dialogue::EntityDialoguePlugin;
use robsim::hud::HudPlugin;
use robsim::map::{
    handle_guard_capture, handle_maze_exit, setup_camera_and_fade, sync_map_outline_collision,
    sync_scene_fade_overlay, trigger_scene_transfer, update_scene_transition, HeistRunStats,
    MapCollisionState, MapPlugin, SceneTransferCooldown, SceneTransitionState,
};
use robsim::player::{follow_player_camera, PlayerPlugin, PlayerSystemSet};
use robsim::prompt_key;
use robsim::tavern::TavernPlugin;
use robsim::text_bubble::TextBubblePlugin;
use robsim::PlayerMoney;

fn main() {
    let asset_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("assets")
        .to_string_lossy()
        .to_string();

    App::new()
        .add_plugins(
            DefaultPlugins
                .set(AssetPlugin {
                    file_path: asset_root,
                    ..default()
                })
                .set(ImagePlugin::default_nearest()),
        )
        .add_plugins(CollisionPlugin)
        .add_plugins(MapPlugin)
        .add_plugins(PlayerPlugin)
        .add_plugins(BankPlugin)
        .add_plugins(HudPlugin)
        .add_plugins(TavernPlugin)
        .add_plugins(TextBubblePlugin)
        .add_plugins(EntityDialoguePlugin)
        .add_plugins(prompt_key::PromptKeyPlugin)
        .insert_resource(BankIcon::BlueMoon)
        .init_resource::<BankHeistState>()
        .init_resource::<GuardAlertState>()
        .init_resource::<SceneTransferCooldown>()
        .init_resource::<SceneTransitionState>()
        .init_resource::<HeistRunStats>()
        .init_resource::<MapCollisionState>()
        .init_resource::<PlayerMoney>()
        .add_systems(Startup, setup_camera_and_fade)
        .add_systems(
            Update,
            (
                materialize_ldtk_entity_sprites,
                sync_map_outline_collision,
                change_bank_img,
                trigger_bank_heist,
                collect_coins.after(PlayerSystemSet::Move),
                update_guards.after(PlayerSystemSet::Move),
                handle_guard_capture.after(update_guards),
                handle_maze_exit.after(PlayerSystemSet::Move),
                update_scene_transition,
            ),
        )
        .add_systems(Update, trigger_scene_transfer.after(PlayerSystemSet::Move))
        .add_systems(
            PostUpdate,
            (
                sync_scene_fade_overlay.after(follow_player_camera),
                update_maze_lighting.after(PlayerSystemSet::Move),
            ),
        )
        .run();
}
