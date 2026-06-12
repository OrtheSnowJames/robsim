pub mod bank;
pub mod collision;
pub mod display_ui;
pub mod enter_interact;
pub mod entity_dialogue;
pub mod hud;
pub mod map;
pub mod multiplayer;
pub mod nine_slicing;
pub mod player;
pub mod prompt_key;
pub mod random;
pub mod receipts;
pub mod sprite_sheet;
pub mod tavern;
pub mod text_bubble;

#[derive(bevy::prelude::Resource, Default)]
pub struct PlayerMoney {
    pub amount: i32,
}

#[macro_export]
macro_rules! hex_color {
    ($hex:expr) => {{
        let hex = $hex;

        Color::srgb(
            ((hex >> 16) & 0xFF) as f32 / 255.0,
            ((hex >> 8) & 0xFF) as f32 / 255.0,
            (hex & 0xFF) as f32 / 255.0,
        )
    }};
}
