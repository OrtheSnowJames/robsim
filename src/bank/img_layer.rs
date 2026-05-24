use bevy::prelude::*;
use bevy_ecs_ldtk::{
    EntityInstance, LdtkProjectHandle, prelude::LdtkProject, ldtk::TileRenderMode,
};
use bevy::asset::RenderAssetUsages;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use rand::RngExt;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use strum::EnumCount;

use crate::bank::BankBuilding;
use crate::bank::render::maze::TILE_SIZE;
use crate::bank::teller::{TellerSprite, VaultSprite};
use crate::collision::BoundingBox;

fn combine_images(base: &Image, overlay: &Image) -> Image {
    // 1. Create a copy of the base image data to work on
    let mut combined_data = match &base.data {
        Some(data) => data.clone(),
        None => {
            return Image::new(
                Extent3d {
                    width: base.width(),
                    height: base.height(),
                    ..default()
                },
                TextureDimension::D2,
                Vec::new(),
                TextureFormat::Rgba8UnormSrgb,
                RenderAssetUsages::default(),
            );
        }
    };
    let overlay_data = match &overlay.data {
        Some(data) => data,
        None => {
            return Image::new(
                Extent3d {
                    width: base.width(),
                    height: base.height(),
                    ..default()
                },
                TextureDimension::D2,
                combined_data,
                TextureFormat::Rgba8UnormSrgb,
                RenderAssetUsages::default(),
            );
        }
    };
    
    let base_width = base.width() as usize;
    let overlay_width = overlay.width() as usize;
    let overlay_height = overlay.height() as usize;

    // 2. Iterate through overlay pixels (assumes RGBA8 format)
    for y in 0..overlay_height {
        for x in 0..overlay_width {
            let overlay_idx = (y * overlay_width + x) * 4;
            let base_idx = (y * base_width + x) * 4;

            // Simple bounds check
            if base_idx + 3 >= combined_data.len() || overlay_idx + 3 >= overlay_data.len() {
                continue;
            }

            // 3. Perform Alpha Blending
            let overlay_alpha = overlay_data[overlay_idx + 3] as f32 / 255.0;
            
            if overlay_alpha > 0.0 {
                for i in 0..3 { // RGB channels
                    let base_pixel = combined_data[base_idx + i] as f32;
                    let overlay_pixel = overlay_data[overlay_idx + i] as f32;
                    
                    // Standard linear blend formula
                    combined_data[base_idx + i] = (
                        (base_pixel * (1.0 - overlay_alpha)) + 
                        (overlay_pixel * overlay_alpha)
                    ) as u8;
                }
                // Set final alpha to opaque or blend it if needed
                combined_data[base_idx + 3] = 255; 
            }
        }
    }

    // 4. Return the new combined Image
    Image::new(
        Extent3d {
            width: base.width(),
            height: base.height(),
            ..default()
        },
        TextureDimension::D2,
        combined_data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    )
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Resource, EnumCount)]
pub enum BankIcon {
    Blank,
    BlueMoon,
    PleaseDontRobUs,
}

impl BankIcon {
    pub fn get_img(self) -> Option<&'static str> {
        match self {
            BankIcon::Blank => None,
            BankIcon::BlueMoon => Some("bank/moon.png"),
            BankIcon::PleaseDontRobUs => Some("bank/please_dont_rob_us.png"),
        }
    }
}


pub struct BankImgOpt {
    pub icon: BankIcon,
    pub open: bool,
}

#[derive(Message, Clone, Copy)]
pub struct RandomizeBankImgMessage {
    pub open: bool,
}

pub struct BankImgLayerPlugin;

impl Plugin for BankImgLayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<RandomizeBankImgMessage>();
        app.add_systems(Update, randomize_bank_img);
    }
}

static BANK_IMAGE_CACHE: OnceLock<Mutex<HashMap<(bool, BankIcon), Handle<Image>>>> = OnceLock::new();
static BANK_IMAGE_MISS_LOGGED: OnceLock<Mutex<HashSet<(bool, BankIcon)>>> = OnceLock::new();
static BANK_ICON_HANDLE_CACHE: OnceLock<Mutex<HashMap<BankIcon, Handle<Image>>>> = OnceLock::new();

pub fn get_bank_img(
    opt: BankImgOpt, 
    assets: &AssetServer,
    images: &mut Assets<Image>,
) -> Handle<Image> {
    let bank_img = if opt.open {
        "bank/bank_open.png"
    } else {
        "bank/bank.png"
    };
    let bank_img: Handle<Image> = assets.load(bank_img);

    if opt.icon == BankIcon::Blank {
        return bank_img;
    }

    let cache = BANK_IMAGE_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let cache_key = (opt.open, opt.icon);
    if let Some(cached) = cache.lock().ok().and_then(|m| m.get(&cache_key).cloned()) {
        return cached;
    }

    let Some(mut icon_path) = opt.icon.get_img() else {
        return bank_img;
    };
    // If a requested icon asset is missing (e.g. only .aseprite exists), fall back to moon
    // so the bank never loses its logo entirely.
    let icon_fs_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("assets")
        .join(icon_path);
    if !icon_fs_path.exists() {
        icon_path = "bank/moon.png";
    }
    let icon_cache = BANK_ICON_HANDLE_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let icon_img: Handle<Image> = if let Ok(mut icons) = icon_cache.lock() {
        if icon_path == "bank/moon.png" {
            assets.load(icon_path)
        } else if let Some(existing) = icons.get(&opt.icon) {
            existing.clone()
        } else {
            let loaded: Handle<Image> = assets.load(icon_path);
            icons.insert(opt.icon, loaded.clone());
            loaded
        }
    } else {
        assets.load(icon_path)
    };

    let Some(base_image) = images.get(&bank_img) else {
        return bank_img;
    };
    let Some(overlay_image) = images.get(&icon_img) else {
        let miss_cache = BANK_IMAGE_MISS_LOGGED.get_or_init(|| Mutex::new(HashSet::new()));
        if let Ok(mut misses) = miss_cache.lock() {
            if misses.insert(cache_key) {
                eprintln!("No overlay image found for {:?}", icon_path);
            }
        }
        return bank_img;
    };

    // layer images
    let combined_img = combine_images(base_image, overlay_image);
    let combined_handle = images.add(combined_img);
    if let Ok(mut map) = cache.lock() {
        map.insert(cache_key, combined_handle.clone());
    }
    if let Ok(mut misses) = BANK_IMAGE_MISS_LOGGED
        .get_or_init(|| Mutex::new(HashSet::new()))
        .lock()
    {
        misses.remove(&cache_key);
    }

    combined_handle
}

pub fn randomize_bank_img(
    mut messages: MessageReader<RandomizeBankImgMessage>,
    assets: Res<AssetServer>,
    mut images: ResMut<Assets<Image>>,
    mut bank_sprites: Query<&mut Sprite, With<BankBuilding>>,
    mut bank_icon: Option<ResMut<BankIcon>>,
) {
    for message in messages.read() {
        let mut rng = rand::rng();
        let picked_icon = if rng.random_bool(0.5) {
            BankIcon::Blank
        } else {
            BankIcon::BlueMoon
        };

        if let Some(icon) = bank_icon.as_deref_mut() {
            *icon = picked_icon;
        }

        let image = get_bank_img(
            BankImgOpt {
                icon: picked_icon,
                open: message.open,
            },
            assets.as_ref(),
            images.as_mut(),
        );

        for mut sprite in &mut bank_sprites {
            sprite.image = image.clone();
        }
    }
}

#[derive(Component)]
pub struct BankSprite;

#[derive(Clone, Copy)]
enum EntityVisualKind {
    Generic,
    Bank,
}

#[derive(Clone, Copy)]
struct EntityVisualSpec {
    identifier: &'static str,
    image_path: &'static str,
    z: f32,
    display_size: Option<Vec2>,
    ldtk_center_x_from_top_left: bool,
    ldtk_center_y_from_top_left: bool,
    with_collision: bool,
    kind: EntityVisualKind,
}

const TOWN_ENTITY_Z: f32 = 6.0;
const ENTITY_VISUAL_SPECS: &[EntityVisualSpec] = &[
    EntityVisualSpec {
        identifier: "Bank",
        image_path: "bank/bank.png",
        z: TOWN_ENTITY_Z,
        display_size: None,
        ldtk_center_x_from_top_left: false,
        ldtk_center_y_from_top_left: false,
        with_collision: true,
        kind: EntityVisualKind::Bank,
    },
    EntityVisualSpec {
        identifier: "Tavern",
        image_path: "tavern.png",
        z: TOWN_ENTITY_Z,
        display_size: None,
        ldtk_center_x_from_top_left: false,
        ldtk_center_y_from_top_left: false,
        with_collision: true,
        kind: EntityVisualKind::Generic,
    },
    EntityVisualSpec {
        identifier: "Vault",
        image_path: "bank/vault.png",
        z: TOWN_ENTITY_Z,
        display_size: Some(Vec2::splat(64.0)),
        ldtk_center_x_from_top_left: false,
        ldtk_center_y_from_top_left: false,
        with_collision: true,
        kind: EntityVisualKind::Generic,
    },
    EntityVisualSpec {
        identifier: "Bear",
        image_path: "bank/bank_teller_relaxed.png",
        z: TOWN_ENTITY_Z,
        display_size: Some(Vec2::new(48.0, 16.0)),
        ldtk_center_x_from_top_left: true,
        ldtk_center_y_from_top_left: false,
        with_collision: false,
        kind: EntityVisualKind::Generic
    },
    EntityVisualSpec {
        identifier: "Teller",
        image_path: "bank/bank_teller_relaxed.png",
        z: TOWN_ENTITY_Z,
        display_size: Some(Vec2::new(48.0, 16.0)),
        ldtk_center_x_from_top_left: true,
        ldtk_center_y_from_top_left: false,
        with_collision: false,
        kind: EntityVisualKind::Generic
    },
    EntityVisualSpec {
        identifier: "Bank_teller",
        image_path: "bank/bank_teller_relaxed.png",
        z: TOWN_ENTITY_Z,
        display_size: Some(Vec2::new(48.0, 16.0)),
        ldtk_center_x_from_top_left: true,
        ldtk_center_y_from_top_left: false,
        with_collision: false,
        kind: EntityVisualKind::Generic
    },
    EntityVisualSpec {
        identifier: "Newspapers",
        image_path: "newspapers.png",
        z: TOWN_ENTITY_Z,
        display_size: Some(Vec2::splat(TILE_SIZE)),
        ldtk_center_x_from_top_left: false,
        ldtk_center_y_from_top_left: false,
        with_collision: true,
        kind: EntityVisualKind::Generic,
    },
    EntityVisualSpec {
        identifier: "Soup_store",
        image_path: "soup_store.png",
        z: TOWN_ENTITY_Z,
        display_size: Some(Vec2::splat(64.)),
        ldtk_center_x_from_top_left: false,
        ldtk_center_y_from_top_left: false,
        with_collision: true,
        kind: EntityVisualKind::Generic
    },
    EntityVisualSpec {
        identifier: "Bar_guy_1",
        image_path: "bar_guy_1.png",
        z: TOWN_ENTITY_Z,
        display_size: Some(Vec2::splat(32.0)),
        ldtk_center_x_from_top_left: false,
        ldtk_center_y_from_top_left: false,
        with_collision: false,
        kind: EntityVisualKind::Generic,
    },
    EntityVisualSpec {
        identifier: "Bar_guy_2",
        image_path: "bar_guy_2.png",
        z: TOWN_ENTITY_Z,
        display_size: Some(Vec2::splat(32.0)),
        ldtk_center_x_from_top_left: false,
        ldtk_center_y_from_top_left: false,
        with_collision: false,
        kind: EntityVisualKind::Generic,
    },
    EntityVisualSpec {
        identifier: "Bartender",
        image_path: "bartender.png",
        z: TOWN_ENTITY_Z,
        display_size: Some(Vec2::splat(32.0)),
        ldtk_center_x_from_top_left: false,
        ldtk_center_y_from_top_left: false,
        with_collision: false,
        kind: EntityVisualKind::Generic,
    },
    EntityVisualSpec {
        identifier: "Soup",
        image_path: "soup.png",
        z: TOWN_ENTITY_Z,
        display_size: Some(Vec2::splat(64.0)),
        ldtk_center_x_from_top_left: true,
        ldtk_center_y_from_top_left: true,
        with_collision: true,
        kind: EntityVisualKind::Generic
    }
];

fn entity_visual_spec(identifier: &str) -> Option<&'static EntityVisualSpec> {
    ENTITY_VISUAL_SPECS
        .iter()
        .find(|spec| spec.identifier.eq_ignore_ascii_case(identifier))
}

fn project_assets_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("assets")
}

fn project_asset_path(relative: &str) -> PathBuf {
    project_assets_dir().join(relative)
}

fn read_png_dimensions(path: &str) -> Option<(u32, u32)> {
    let bytes = fs::read(path).ok()?;
    if bytes.len() < 24 {
        return None;
    }
    let width = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
    let height = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
    Some((width, height))
}

pub fn materialize_ldtk_entity_sprites(
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut images: ResMut<Assets<Image>>,
    ldtk_projects: Res<Assets<LdtkProject>>,
    ldtk_worlds: Query<&LdtkProjectHandle>,
    entity_instances: Query<(Entity, &EntityInstance, &Transform), Added<EntityInstance>>,
) {
    let bank_dynamic_image_handle = get_bank_img(
        BankImgOpt {
            icon: BankIcon::BlueMoon,
            open: false,
        },
        assets.as_ref(),
        images.as_mut(),
    );
    let images_ref = images.as_ref();

    let ldtk_project = ldtk_worlds
        .single()
        .ok()
        .and_then(|h| ldtk_projects.get(&**h));

    for (entity, instance, transform) in &entity_instances {
        let Some(spec) = entity_visual_spec(&instance.identifier) else {
            if let Some(project) = ldtk_project {
                insert_entity_preview_sprite(
                    &mut commands,
                    entity,
                    instance,
                    transform,
                    project,
                    images_ref,
                );
            }
            continue;
        };

        let size =
            read_png_dimensions(project_asset_path(spec.image_path).to_string_lossy().as_ref())
                .unwrap_or((instance.width as u32, instance.height as u32));

        let image = match spec.kind {
            EntityVisualKind::Bank => bank_dynamic_image_handle.clone(),
            EntityVisualKind::Generic => assets.load::<Image>(spec.image_path),
        };

        let effective_display_size = spec.display_size.unwrap_or(Vec2::new(
            instance.width as f32,
            instance.height as f32,
        ));

        let entity_x = if spec.ldtk_center_x_from_top_left {
            transform.translation.x + (effective_display_size.x * 0.5)
        } else {
            transform.translation.x
        };
        let entity_y = if spec.ldtk_center_y_from_top_left {
            transform.translation.y - (effective_display_size.y * 0.5)
        } else {
            transform.translation.y
        };

        let mut entity_cmd = commands.entity(entity);
        entity_cmd.insert((
            Sprite {
                image,
                custom_size: spec.display_size,
                ..default()
            },
            Transform::from_xyz(entity_x, entity_y, spec.z),
        ));
        if spec.with_collision {
            entity_cmd.insert(BoundingBox {
                width: size.0 as f32,
                height: size.1 as f32,
            });
        }
        if let EntityVisualKind::Bank = spec.kind {
            entity_cmd.insert((BankBuilding, BankSprite));
        }
        if spec.identifier.eq_ignore_ascii_case("Vault") {
            entity_cmd.insert(VaultSprite);
        }
        if matches!(
            spec.identifier,
            "Bear" | "Teller" | "Bank_teller"
        ) {
            entity_cmd.insert(TellerSprite);
        }
    }
}

fn insert_entity_preview_sprite(
    commands: &mut Commands,
    entity: Entity,
    entity_instance: &EntityInstance,
    entity_transform: &Transform,
    project: &LdtkProject,
    images: &Assets<Image>,
) {
    let Some(tile_rect) = &entity_instance.tile else {
        return;
    };

    let Some(tileset_image) = project.tileset_map().get(&tile_rect.tileset_uid) else {
        return;
    };

    let entity_def = project
        .json_data()
        .defs
        .entities
        .iter()
        .find(|def| def.uid == entity_instance.def_uid);
    let is_full_size_uncropped = entity_def
        .map(|def| {
            println!("def.tile_render_mode: {:?}", def.tile_render_mode);
            def.tile_render_mode == TileRenderMode::FullSizeUncropped
        })
        .unwrap_or_else(|| {
            println!("Def thing unfound");
            false
        });

    let mut sprite = Sprite::from_image(tileset_image.clone());
    if !is_full_size_uncropped {
        sprite.rect = Some(Rect::new(
            tile_rect.x as f32,
            tile_rect.y as f32,
            (tile_rect.x + tile_rect.w) as f32,
            (tile_rect.y + tile_rect.h) as f32,
        ));
        // Non-fullsize previews follow the LDtk instance size.
        if entity_instance.width > 0 && entity_instance.height > 0 {
            sprite.custom_size = Some(Vec2::new(
                entity_instance.width as f32,
                entity_instance.height as f32,
            ));
        }
    } else if let Some(def) = entity_def {
        let preview_dims = def
            .ui_tile_rect
            .as_ref()
            .or(def.tile_rect.as_ref())
            .map(|r| (r.w, r.h));

        if let Some(preview_rect) = def.ui_tile_rect.as_ref().or(def.tile_rect.as_ref()) {
            sprite.rect = Some(Rect::new(
                preview_rect.x as f32,
                preview_rect.y as f32,
                (preview_rect.x + preview_rect.w) as f32,
                (preview_rect.y + preview_rect.h) as f32,
            ));
            // FullSizeUncropped should use preview/entity dimensions, not 16x16 instance tile size.
            if def.width > 0 && def.height > 0 {
                sprite.custom_size = Some(Vec2::new(def.width as f32, def.height as f32));
            } else {
                sprite.custom_size = Some(Vec2::new(preview_rect.w as f32, preview_rect.h as f32));
            }
        }

        if sprite.custom_size.is_none() && images.get(tileset_image).is_none() {
            // Keep this debug path explicit.
            if let Some((w, h)) = preview_dims {
                println!(
                    "FullSizeUncropped fallback for {}: image not loaded yet, using preview rect {}x{}",
                    entity_instance.identifier, w, h
                );
            }
        } else if sprite.custom_size.is_none() {
            // Last resort fallback.
            if let Some(image) = images.get(tileset_image) {
                sprite.custom_size = Some(Vec2::new(image.width() as f32, image.height() as f32));
            }
        }
    }

    commands.entity(entity).insert((
        sprite,
        Transform {
            translation: Vec3::new(
                entity_transform.translation.x,
                entity_transform.translation.y,
                TOWN_ENTITY_Z,
            ),
            rotation: entity_transform.rotation,
            scale: Vec3::ONE,
        },
    ));
}

pub fn change_bank_img(
    assets: Res<AssetServer>,
    images: ResMut<Assets<Image>>,
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut bank_icon: ResMut<BankIcon>,
    mut bank_sprites: Query<&mut Sprite, With<BankSprite>>,
) {
    if keyboard_input.just_pressed(KeyCode::Space) {
        *bank_icon = BankIcon::BlueMoon;
    }
    if keyboard_input.just_pressed(KeyCode::Escape) {
        *bank_icon = BankIcon::Blank;
    }

    let bank_image = get_bank_img(
        BankImgOpt {
            icon: *bank_icon,
            open: false,
        },
        assets.as_ref(),
        images.into_inner(),
    );

    for mut sprite in &mut bank_sprites {
        sprite.image = bank_image.clone();
    }
}

fn process_entity_previews(
    mut commands: Commands,
    // Querying for instances that haven't been processed yet
    query: Query<(Entity, &EntityInstance, &Transform), Added<EntityInstance>>,
    images: Res<Assets<Image>>,
    ldtk_projects: Res<Assets<LdtkProject>>,
    ldtk_worlds: Query<&LdtkProjectHandle>,
) {
    let Ok(project_handle) = ldtk_worlds.single() else {
        return;
    };
    let Some(project) = ldtk_projects.get(&**project_handle) else {
        return;
    };

    for (entity, entity_instance, transform) in &query {
        insert_entity_preview_sprite(
            &mut commands,
            entity,
            entity_instance,
            transform,
            project,
            images.as_ref(),
        );
    }
}
