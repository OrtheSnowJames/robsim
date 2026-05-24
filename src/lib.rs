pub mod bank;
pub mod player;
pub mod collision;
pub mod map;
pub mod prompt_key;
pub mod hud;
pub mod sprite_sheet;
pub mod text_bubble;
pub mod tavern;
pub mod entity_dialogue;

use bevy::math::Vec2;
use rand::{RngExt, prelude::ThreadRng};

#[derive(bevy::prelude::Resource, Default)]
pub struct PlayerMoney {
    pub amount: i32,
}

pub fn rand_vec2(mut rng: ThreadRng, range: std::ops::Range<f32>) -> Vec2 {
    let x = rng.random_range(range.clone());
    let y = rng.random_range(range.clone());
    Vec2::new(x, y)
}
