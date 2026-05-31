use bevy::prelude::*;
use robsim::PlayerMoney;
use robsim::bank::BankPlugin;
use robsim::bank::guard::{GuardAlertState, update_guards};
use robsim::bank::img_layer::{
    BankIcon, apply_tile_layer_visual_specs, change_bank_img, enforce_locked_global_z,
    materialize_ldtk_entity_sprites,
};
use robsim::bank::render::maze::{collect_coins, update_maze_lighting};
use robsim::bank::teller::{BankHeistState, trigger_bank_heist};
use robsim::collision::CollisionPlugin;
use robsim::entity_dialogue::EntityDialoguePlugin;
use robsim::enter_interact::EnterInteractPlugin;
use robsim::hud::HudPlugin;
use robsim::map::scene::{apply_scene_background_spec, setup_bg, sync_bg_with_camera};
use robsim::map::{
    HeistLifetimeStats, HeistRunStats, MapCollisionState, MapPlugin, SceneTransferCooldown,
    SceneTransitionState, handle_guard_capture, handle_maze_exit, handle_scene_change_request,
    setup_camera_and_fade, sync_map_outline_collision, sync_scene_fade_overlay,
    trigger_scene_transfer, update_scene_transition,
};
use robsim::nine_slicing::NineSlicingPlugin;
use robsim::player::{PlayerPlugin, PlayerSystemSet, follow_player_camera};
use robsim::prompt_key;
use robsim::receipts::ReceiptCache;
use robsim::tavern::TavernPlugin;
use robsim::text_bubble::TextBubblePlugin;

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
        .add_plugins(EnterInteractPlugin)
        .add_plugins(NineSlicingPlugin)
        .add_plugins(prompt_key::PromptKeyPlugin)
        .insert_resource(BankIcon::BlueMoon)
        .init_resource::<BankHeistState>()
        .init_resource::<GuardAlertState>()
        .init_resource::<SceneTransferCooldown>()
        .init_resource::<SceneTransitionState>()
        .init_resource::<HeistLifetimeStats>()
        .init_resource::<HeistRunStats>()
        .init_resource::<MapCollisionState>()
        .init_resource::<PlayerMoney>()
        .init_resource::<ReceiptCache>()
        .add_systems(Startup, (setup_camera_and_fade, setup_bg))
        .add_systems(
            Update,
            (
                materialize_ldtk_entity_sprites,
                apply_tile_layer_visual_specs.after(materialize_ldtk_entity_sprites),
                enforce_locked_global_z.after(apply_tile_layer_visual_specs),
                sync_map_outline_collision,
                change_bank_img,
                trigger_bank_heist,
                collect_coins.after(PlayerSystemSet::Move),
                update_guards.after(PlayerSystemSet::Move),
                handle_guard_capture.after(update_guards),
                handle_maze_exit.after(PlayerSystemSet::Move),
                handle_scene_change_request.after(PlayerSystemSet::Move),
                update_scene_transition,
            ),
        )
        .add_systems(Update, trigger_scene_transfer.after(PlayerSystemSet::Move))
        .add_systems(
            PostUpdate,
            (
                sync_scene_fade_overlay.after(follow_player_camera),
                sync_bg_with_camera.after(follow_player_camera),
                apply_scene_background_spec.after(sync_bg_with_camera),
                update_maze_lighting.after(PlayerSystemSet::Move),
            ),
        )
        .run();
}
