use bevy::prelude::{Color, IVec2, Vec2};
use bevy_ecs_ldtk::ldtk::{
    AutoLayerRuleDefinition, AutoLayerRuleGroup, Checker, Definitions, EntityDefinition,
    EntityInstance, ImageExportMode, IntGridValueDefinition, LayerDefinition, LayerInstance,
    LdtkJson, Level, LimitBehavior, LimitScope, RenderMode, TileInstance, TileMode,
    TileRenderMode, TilesetDefinition, TilesetRectangle, Type,
};
use serde_json::Value;
use std::error::Error;
use std::fs;
use std::path::Path;

const SOURCE_MAP_PATH: &str = "assets/maps/town.editor.map.json";
const TARGET_LDTK_PATH: &str = "assets/maps/town.ldtk";

const TILESET_ROAD_UID: i32 = 1;
const TILESET_BANK_UID: i32 = 2;
const TILESET_TAVERN_UID: i32 = 3;
const TILESET_PLAYER_UID: i32 = 4;
const TILESET_GRASS_UID: i32 = 5;
const LAYER_ENTITIES_UID: i32 = 10;
const LAYER_ROAD_UID: i32 = 11;
const LAYER_GROUND_UID: i32 = 12;
const ENTITY_BANK_UID: i32 = 100;
const ENTITY_TAVERN_UID: i32 = 101;
const ENTITY_PLAYER_START_UID: i32 = 102;
const TILE_INDEX_MASK: u32 = 0x1FFF_FFFF;
const TILE_FLIP_X: u32 = 0x8000_0000;
const TILE_FLIP_Y: u32 = 0x4000_0000;

fn main() -> Result<(), Box<dyn Error>> {
    let content = fs::read_to_string(SOURCE_MAP_PATH)?;
    let source_json: Value = serde_json::from_str(&content)?;

    let level = source_json
        .get("levels")
        .and_then(Value::as_array)
        .and_then(|levels| levels.first())
        .ok_or("missing levels[0] in source map")?;

    let width = level.get("width").and_then(Value::as_i64).ok_or("missing level width")? as i32;
    let height = level.get("height").and_then(Value::as_i64).ok_or("missing level height")? as i32;
    let grid_size = source_json
        .get("schema")
        .and_then(|s| s.get("project"))
        .and_then(|p| p.get("tile_size"))
        .and_then(Value::as_i64)
        .unwrap_or(16) as i32;

    let road_layer_tiles = level
        .get("layers")
        .and_then(Value::as_array)
        .and_then(|layers| {
            layers.iter().find(|layer| {
                layer
                    .get("name")
                    .and_then(Value::as_str)
                    .map(|name| name == "Road")
                    .unwrap_or(false)
            })
        })
        .and_then(|layer| layer.get("data"))
        .and_then(|data| data.get("Tiles"))
        .and_then(|tiles| tiles.get("tiles"))
        .and_then(Value::as_array)
        .ok_or("missing Road tile data")?;
    let ground_layer_tiles = level
        .get("layers")
        .and_then(Value::as_array)
        .and_then(|layers| {
            layers.iter().find(|layer| {
                layer
                    .get("name")
                    .and_then(Value::as_str)
                    .map(|name| name == "Ground")
                    .unwrap_or(false)
            })
        })
        .and_then(|layer| layer.get("data"))
        .and_then(|data| data.get("Tiles"))
        .and_then(|tiles| tiles.get("tiles"))
        .and_then(Value::as_array)
        .ok_or("missing Ground tile data")?;

    let mut bank_pos = IVec2::new(344, 240);
    let mut tavern_pos = IVec2::new(224, 240);
    if let Some(entities) = level.get("entities").and_then(Value::as_array) {
        for entity in entities {
            let Some(type_name) = entity.get("type_name").and_then(Value::as_str) else {
                continue;
            };
            let Some(pos) = entity.get("position").and_then(Value::as_array) else {
                continue;
            };
            if pos.len() < 2 {
                continue;
            }
            let x = pos[0].as_f64().unwrap_or_default() as i32;
            let y = pos[1].as_f64().unwrap_or_default() as i32;
            match type_name {
                "Bank" => bank_pos = IVec2::new(x, y),
                "Tavern" => tavern_pos = IVec2::new(x, y),
                _ => {}
            }
        }
    }

    let player_start_pos = IVec2::new(216, 128);
    let level_px_wid = width * grid_size;
    let level_px_hei = height * grid_size;
    let bank_size = read_png_dimensions("assets/bank/bank.png").unwrap_or((64, 64));
    let tavern_size = read_png_dimensions("assets/tavern.png").unwrap_or((128, 64));
    let robber_size = read_png_dimensions("assets/robber.png").unwrap_or((64, 64));
    let grass_size = read_png_dimensions("assets/Grass.png").unwrap_or((112, 48));
    let grass_cols = (grass_size.0 / grid_size as u32).max(1);
    let grass_rows = (grass_size.1 / grid_size as u32).max(1);

    let mut int_grid_csv = vec![0; (width * height) as usize];
    let mut auto_layer_tiles = Vec::new();
    for (idx, tile) in road_layer_tiles.iter().enumerate() {
        if tile.is_null() {
            continue;
        }

        int_grid_csv[idx] = 1;

        let x = (idx as i32) % width;
        let y = (idx as i32) / width;
        auto_layer_tiles.push(TileInstance {
            a: 1.0,
            d: vec![0, idx as i32],
            f: 0,
            px: IVec2::new(x * grid_size, y * grid_size),
            src: IVec2::ZERO,
            t: 0,
        });
    }
    let mut ground_grid_tiles = Vec::new();
    for (idx, tile) in ground_layer_tiles.iter().enumerate() {
        let Some(raw) = tile.as_u64() else {
            continue;
        };
        let raw = raw as u32;
        let tile_index = raw & TILE_INDEX_MASK;
        let tile_x = tile_index % grass_cols;
        let tile_y = tile_index / grass_cols;
        let x = (idx as i32) % width;
        let y = (idx as i32) / width;
        let mut flip = 0;
        if raw & TILE_FLIP_X != 0 {
            flip |= 1;
        }
        if raw & TILE_FLIP_Y != 0 {
            flip |= 2;
        }
        ground_grid_tiles.push(TileInstance {
            a: 1.0,
            d: vec![idx as i32],
            f: flip,
            px: IVec2::new(x * grid_size, y * grid_size),
            src: IVec2::new((tile_x * grid_size as u32) as i32, (tile_y * grid_size as u32) as i32),
            t: tile_index as i32,
        });
    }

    let mut ldtk = LdtkJson {
        iid: "f0e3d9c4-3fce-49f6-9f32-62e57711a3d2".to_string(),
        app_build_id: 1.0,
        backup_limit: 0,
        backup_on_save: false,
        bg_color: Color::srgb(0.16, 0.17, 0.20),
        custom_commands: vec![],
        default_entity_height: 16,
        default_entity_width: 16,
        default_grid_size: grid_size,
        default_level_bg_color: Color::srgb(0.16, 0.17, 0.20),
        default_level_height: Some(level_px_hei),
        default_level_width: Some(level_px_wid),
        default_pivot_x: 0.5,
        default_pivot_y: 0.5,
        dummy_world_iid: "f649368a-041e-4498-9555-a7f887bd2a84".to_string(),
        export_level_bg: false,
        export_png: Some(false),
        export_tiled: false,
        external_levels: false,
        flags: vec![],
        identifier_style: Default::default(),
        image_export_mode: ImageExportMode::None,
        json_version: "1.5.3".to_string(),
        level_name_pattern: "Level_%idx".to_string(),
        minify_json: false,
        next_uid: 2000,
        simplified_export: false,
        world_grid_height: Some(level_px_hei),
        world_grid_width: Some(level_px_wid),
        world_layout: Some(Default::default()),
        ..Default::default()
    };

    ldtk.defs = Definitions {
        entities: vec![
            make_entity_def(
                ENTITY_BANK_UID,
                "Bank",
                bank_size.0 as i32,
                bank_size.1 as i32,
                Color::srgb(0.79, 0.71, 0.54),
                Some(TilesetRectangle {
                    h: bank_size.1 as i32,
                    w: bank_size.0 as i32,
                    x: 0,
                    y: 0,
                    tileset_uid: TILESET_BANK_UID,
                }),
            ),
            make_entity_def(
                ENTITY_TAVERN_UID,
                "Tavern",
                tavern_size.0 as i32,
                tavern_size.1 as i32,
                Color::srgb(0.49, 0.42, 0.32),
                Some(TilesetRectangle {
                    h: tavern_size.1 as i32,
                    w: tavern_size.0 as i32,
                    x: 0,
                    y: 0,
                    tileset_uid: TILESET_TAVERN_UID,
                }),
            ),
            make_entity_def(
                ENTITY_PLAYER_START_UID,
                "PlayerStart",
                16,
                16,
                Color::srgb(0.95, 0.95, 0.95),
                Some(TilesetRectangle {
                    h: 16,
                    w: 16,
                    x: 0,
                    y: 0,
                    tileset_uid: TILESET_PLAYER_UID,
                }),
            ),
        ],
        layers: vec![
            make_entities_layer_def(LAYER_ENTITIES_UID, grid_size),
            make_road_layer_def(LAYER_ROAD_UID, TILESET_ROAD_UID, grid_size),
            make_ground_layer_def(LAYER_GROUND_UID, TILESET_GRASS_UID, grid_size),
        ],
        tilesets: vec![
            TilesetDefinition {
                c_hei: 1,
                c_wid: 1,
                identifier: "CobblestoneCenter".to_string(),
                padding: 0,
                px_hei: grid_size,
                px_wid: grid_size,
                rel_path: Some("../cobblestone_center_16.png".to_string()),
                spacing: 0,
                tile_grid_size: grid_size,
                uid: TILESET_ROAD_UID,
                ..Default::default()
            },
            TilesetDefinition {
                c_hei: 1,
                c_wid: 1,
                identifier: "BankEntityVisual".to_string(),
                padding: 0,
                px_hei: bank_size.1 as i32,
                px_wid: bank_size.0 as i32,
                rel_path: Some("../bank/bank.png".to_string()),
                spacing: 0,
                tile_grid_size: bank_size.0.max(bank_size.1) as i32,
                uid: TILESET_BANK_UID,
                ..Default::default()
            },
            TilesetDefinition {
                c_hei: 1,
                c_wid: (tavern_size.0 / tavern_size.1.max(1)) as i32,
                identifier: "TavernEntityVisual".to_string(),
                padding: 0,
                px_hei: tavern_size.1 as i32,
                px_wid: tavern_size.0 as i32,
                rel_path: Some("../tavern.png".to_string()),
                spacing: 0,
                tile_grid_size: tavern_size.1 as i32,
                uid: TILESET_TAVERN_UID,
                ..Default::default()
            },
            TilesetDefinition {
                c_hei: (robber_size.1 / 16) as i32,
                c_wid: (robber_size.0 / 16) as i32,
                identifier: "PlayerEntityVisual".to_string(),
                padding: 0,
                px_hei: robber_size.1 as i32,
                px_wid: robber_size.0 as i32,
                rel_path: Some("../robber.png".to_string()),
                spacing: 0,
                tile_grid_size: 16,
                uid: TILESET_PLAYER_UID,
                ..Default::default()
            },
            TilesetDefinition {
                c_hei: grass_rows as i32,
                c_wid: grass_cols as i32,
                identifier: "Grass".to_string(),
                padding: 0,
                px_hei: grass_size.1 as i32,
                px_wid: grass_size.0 as i32,
                rel_path: Some("../Grass.png".to_string()),
                spacing: 0,
                tile_grid_size: grid_size,
                uid: TILESET_GRASS_UID,
                ..Default::default()
            },
        ],
        ..Default::default()
    };

    let entities_layer = LayerInstance {
        c_hei: height,
        c_wid: width,
        grid_size,
        identifier: "Entities".to_string(),
        opacity: 1.0,
        layer_instance_type: Type::Entities,
        iid: "0f347e4f-8aa5-42ec-8a6c-c03d9e76e6ef".to_string(),
        layer_def_uid: LAYER_ENTITIES_UID,
        level_id: 1,
        visible: true,
        entity_instances: vec![
            make_entity_instance(
                "Bank",
                ENTITY_BANK_UID,
                "ea0b283b-5928-4d8a-9a7b-c5350e5fc0d4",
                bank_pos,
                level_px_hei,
                bank_size.0 as i32,
                bank_size.1 as i32,
            ),
            make_entity_instance(
                "Tavern",
                ENTITY_TAVERN_UID,
                "c60d2e1e-849f-4a2e-ad88-ea0d534b8b76",
                tavern_pos,
                level_px_hei,
                tavern_size.0 as i32,
                tavern_size.1 as i32,
            ),
            make_entity_instance("PlayerStart", ENTITY_PLAYER_START_UID, "2764be0d-8ec0-4f09-9be3-3642f3c8cf57", player_start_pos, level_px_hei, 16, 16),
        ],
        ..Default::default()
    };

    let road_layer = LayerInstance {
        c_hei: height,
        c_wid: width,
        grid_size,
        identifier: "RoadAuto".to_string(),
        opacity: 1.0,
        tileset_def_uid: Some(TILESET_ROAD_UID),
        tileset_rel_path: Some("../cobblestone_center_16.png".to_string()),
        layer_instance_type: Type::IntGrid,
        iid: "4ebf50aa-114b-430f-a9fe-7690dccf5408".to_string(),
        int_grid_csv,
        layer_def_uid: LAYER_ROAD_UID,
        level_id: 1,
        visible: true,
        auto_layer_tiles,
        ..Default::default()
    };
    let ground_layer = LayerInstance {
        c_hei: height,
        c_wid: width,
        grid_size,
        identifier: "Ground".to_string(),
        opacity: 1.0,
        tileset_def_uid: Some(TILESET_GRASS_UID),
        tileset_rel_path: Some("../Grass.png".to_string()),
        layer_instance_type: Type::Tiles,
        iid: "5673fd51-8d62-4c6d-8ec8-29fa72f95e11".to_string(),
        layer_def_uid: LAYER_GROUND_UID,
        level_id: 1,
        visible: true,
        grid_tiles: ground_grid_tiles,
        ..Default::default()
    };

    ldtk.levels = vec![Level {
        identifier: "Town".to_string(),
        iid: "f434d4e6-0ef1-4933-a7c1-f301d346ce55".to_string(),
        uid: 1,
        layer_instances: Some(vec![entities_layer, road_layer, ground_layer]),
        px_hei: level_px_hei,
        px_wid: level_px_wid,
        use_auto_identifier: false,
        world_depth: 0,
        world_x: 0,
        world_y: 0,
        ..Default::default()
    }];

    let target_parent = Path::new(TARGET_LDTK_PATH)
        .parent()
        .ok_or("invalid target path")?;
    fs::create_dir_all(target_parent)?;
    fs::write(TARGET_LDTK_PATH, serde_json::to_string_pretty(&ldtk)?)?;
    println!("wrote {TARGET_LDTK_PATH}");

    Ok(())
}

fn make_entity_def(
    uid: i32,
    identifier: &str,
    width: i32,
    height: i32,
    color: Color,
    tile_rect: Option<TilesetRectangle>,
) -> EntityDefinition {
    let tileset_id = tile_rect.as_ref().map(|rect| rect.tileset_uid);
    let render_mode = if tile_rect.is_some() {
        RenderMode::Tile
    } else {
        RenderMode::Rectangle
    };

    EntityDefinition {
        uid,
        identifier: identifier.to_string(),
        width,
        height,
        color,
        pivot_x: 0.5,
        pivot_y: 0.5,
        render_mode,
        tile_rect,
        tile_render_mode: TileRenderMode::FitInside,
        tileset_id,
        tile_id: None,
        tile_opacity: 1.0,
        fill_opacity: 1.0,
        line_opacity: 1.0,
        limit_behavior: LimitBehavior::MoveLastOne,
        limit_scope: LimitScope::PerLevel,
        max_count: 0,
        ..Default::default()
    }
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

fn make_entities_layer_def(uid: i32, grid_size: i32) -> LayerDefinition {
    LayerDefinition {
        uid,
        identifier: "Entities".to_string(),
        layer_definition_type: "Entities".to_string(),
        purple_type: Type::Entities,
        grid_size,
        display_opacity: 1.0,
        tile_pivot_x: 0.0,
        tile_pivot_y: 0.0,
        ..Default::default()
    }
}

fn make_road_layer_def(uid: i32, tileset_uid: i32, grid_size: i32) -> LayerDefinition {
    let rule = AutoLayerRuleDefinition {
        active: true,
        alpha: 1.0,
        break_on_match: true,
        chance: 1.0,
        checker: Checker::None,
        flip_x: false,
        flip_y: false,
        invalidated: false,
        out_of_bounds_value: Some(0),
        pattern: vec![1],
        perlin_active: false,
        perlin_octaves: 1.0,
        perlin_scale: 1.0,
        perlin_seed: 0.0,
        pivot_x: 0.0,
        pivot_y: 0.0,
        size: 1,
        tile_ids: None,
        tile_mode: TileMode::Single,
        tile_random_x_max: 0,
        tile_random_x_min: 0,
        tile_random_y_max: 0,
        tile_random_y_min: 0,
        tile_rects_ids: vec![vec![0, 0, grid_size, grid_size]],
        tile_x_offset: 0,
        tile_y_offset: 0,
        uid: 11001,
        x_modulo: 1,
        x_offset: 0,
        y_modulo: 1,
        y_offset: 0,
    };
    let rule_group = AutoLayerRuleGroup {
        active: true,
        biome_requirement_mode: 0,
        collapsed: Some(false),
        color: None,
        icon: None,
        is_optional: false,
        name: "Road".to_string(),
        required_biome_values: vec![],
        rules: vec![rule],
        uid: 11000,
        uses_wizard: false,
    };

    LayerDefinition {
        uid,
        identifier: "RoadAuto".to_string(),
        layer_definition_type: "IntGrid".to_string(),
        purple_type: Type::IntGrid,
        grid_size,
        display_opacity: 1.0,
        tile_pivot_x: 0.0,
        tile_pivot_y: 0.0,
        tileset_def_uid: Some(tileset_uid),
        auto_rule_groups: vec![rule_group],
        int_grid_values: vec![IntGridValueDefinition {
            value: 1,
            identifier: Some("Road".to_string()),
            color: Color::srgb(0.50, 0.50, 0.50),
            ..Default::default()
        }],
        ..Default::default()
    }
}

fn make_ground_layer_def(uid: i32, tileset_uid: i32, grid_size: i32) -> LayerDefinition {
    LayerDefinition {
        uid,
        identifier: "Ground".to_string(),
        layer_definition_type: "Tiles".to_string(),
        purple_type: Type::Tiles,
        grid_size,
        display_opacity: 1.0,
        tile_pivot_x: 0.0,
        tile_pivot_y: 0.0,
        tileset_def_uid: Some(tileset_uid),
        ..Default::default()
    }
}

fn make_entity_instance(
    identifier: &str,
    def_uid: i32,
    iid: &str,
    world_center: IVec2,
    level_height: i32,
    width: i32,
    height: i32,
) -> EntityInstance {
    EntityInstance {
        identifier: identifier.to_string(),
        def_uid,
        iid: iid.to_string(),
        grid: IVec2::new(world_center.x / 16, world_center.y / 16),
        pivot: Vec2::splat(0.5),
        smart_color: Color::WHITE,
        px: IVec2::new(world_center.x, level_height - world_center.y),
        width,
        height,
        ..Default::default()
    }
}
