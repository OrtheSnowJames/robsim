use crate::bank::img_layer::BankIcon;
use crate::display_ui::{DisplayUiPlugin, DisplayUiState};
use crate::map::{LdtkEntityByNameQuery, LoadedMap, loaded_map_path};
use crate::random::{random_bool, random_range_i32};
use crate::receipts::{Receipt, ReceiptCache};
use crate::text_bubble::TextBubble;
use bevy::prelude::*;
use serde_json::Value;

const TAVERN_MAP_PATH: &str = "maps/tavern.ldtk";
const BANK_SIGN_TRIGGER_LINE: &str = "A slightly larger sign saying 'Please Don't Rob Us.'";
const FORCE_PLEASE_DONT_ROB_US_SIGN: bool = false;
const CONDITIONAL_POOL_PICK_CHANCE: f64 = 0.5;
const CONDITIONAL_DIALOGUE_PICK_CHANCE: f64 = 0.85;

pub struct TavernPlugin;

impl Plugin for TavernPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(DisplayUiPlugin)
            .add_message::<HeistReportMessage>()
            .init_resource::<LastHeistReport>()
            .init_resource::<TavernBubbleState>()
            .init_resource::<CachedTavernTalk>()
            .add_systems(Update, (record_heist_report, load_tavern_talk_from_json));
    }
}

#[derive(Message, Clone, Copy)]
pub struct HeistReportMessage {
    pub successful: bool,
    pub money: i32,
    pub profit: i32,
    pub successful_robberies: u32,
    pub stopped_at_shaft: bool,
    pub time_till_death_secs: Option<f32>,
    pub heist_duration_secs: f32,
}

#[derive(Resource, Default, Clone, Copy)]
struct LastHeistReport(Option<HeistReportMessage>);

#[derive(Resource, Default)]
struct TavernBubbleState {
    applied: bool,
}

#[derive(Resource, Default)]
struct CachedTavernTalk(Option<TavernTalk>);

pub struct TavernDialogue {
    pub guy1: String,
    pub guy2: String,
    pub bartender: String,
}

pub struct TavernTalk {
    pub newspaper: String,
    pub dialogue: TavernDialogue,
}

fn random_number(range: std::ops::Range<i32>) -> i32 {
    random_range_i32(range)
}

impl TavernTalk {
    pub fn from_lines_object(json_val: Value, receipt: Receipt) -> Self {
        let first_index = if receipt.successful {
            "successful"
        } else {
            "unsuccessful"
        };

        let first = json_val.get(first_index).cloned().unwrap_or(Value::Null);
        let conditional_bucket = pick_condition_bucket(
            &first,
            receipt.money,
            receipt.profit,
            receipt.successful_robberies,
            receipt.stopped_at_shaft,
            receipt.time_till_death_secs,
            receipt.heist_duration_secs,
        );
        let prefer_conditional = random_bool(CONDITIONAL_POOL_PICK_CHANCE);
        let base_newspaper_pool = collect_newspaper_pool(&first);
        let conditional_newspaper_pool = conditional_bucket
            .as_ref()
            .map(collect_newspaper_pool)
            .unwrap_or_default();
        let newspaper_pool = choose_pool(
            base_newspaper_pool,
            conditional_newspaper_pool,
            prefer_conditional,
        );
        let newspaper = if newspaper_pool.is_empty() {
            "No newspaper article available.".to_string()
        } else {
            let idx = random_number(0..newspaper_pool.len() as i32) as usize;
            newspaper_pool[idx].clone()
        };

        let base_dialogue_pool = {
            let mut pool = collect_unconditional_dialogue_pool(&first, &json_val);
            pool.extend(collect_dialogue_pool(&first));
            pool
        };
        let conditional_dialogue_pool = collect_matching_conditional_dialogue_pool(
            &first,
            receipt.money,
            receipt.profit,
            receipt.successful_robberies,
            receipt.stopped_at_shaft,
            receipt.time_till_death_secs,
            receipt.heist_duration_secs,
        );
        let prefer_conditional_dialogue = random_bool(CONDITIONAL_DIALOGUE_PICK_CHANCE);
        let dialogue_pool = choose_pool(
            base_dialogue_pool,
            conditional_dialogue_pool,
            prefer_conditional_dialogue,
        );
        let chosen_dialogue = if dialogue_pool.is_empty() {
            Value::Null
        } else {
            let idx = random_number(0..dialogue_pool.len() as i32) as usize;
            dialogue_pool[idx].clone()
        };

        let dialogue = TavernDialogue {
            // Primary schema in lines.jsonc: "1"/"2"/"3"
            // Backward compatible schema: "guy1"/"guy2"/"bartender"
            guy1: pick_dialogue_line(&chosen_dialogue, &["1", "guy1"]),
            guy2: pick_dialogue_line(&chosen_dialogue, &["2", "guy2"]),
            bartender: pick_dialogue_line(&chosen_dialogue, &["3", "bartender"]),
        };

        Self {
            newspaper,
            dialogue,
        }
    }

    pub fn from_intro_lines_object(json_val: Value) -> Option<Self> {
        let intro = json_val.get("intro")?;
        let dialogue_node = intro.get("dialogue")?;
        let chosen_dialogue = if let Some(options) = dialogue_node.as_array() {
            if options.is_empty() {
                Value::Null
            } else {
                let idx = random_number(0..options.len() as i32) as usize;
                options[idx].clone()
            }
        } else {
            dialogue_node.clone()
        };

        let mut newspaper_pool = Vec::new();
        if let Some(single) = intro.get("newspaper").and_then(Value::as_str) {
            newspaper_pool.push(single.to_string());
        }
        newspaper_pool.extend(collect_newspaper_pool(intro));
        let newspaper = if newspaper_pool.is_empty() {
            "No newspaper article available.".to_string()
        } else {
            let idx = random_number(0..newspaper_pool.len() as i32) as usize;
            newspaper_pool[idx].clone()
        };

        Some(Self {
            newspaper,
            dialogue: TavernDialogue {
                guy1: pick_dialogue_line(&chosen_dialogue, &["1", "guy1"]),
                guy2: pick_dialogue_line(&chosen_dialogue, &["2", "guy2"]),
                bartender: pick_dialogue_line(&chosen_dialogue, &["3", "bartender"]),
            },
        })
    }
}

fn choose_pool<T>(base: Vec<T>, conditional: Vec<T>, prefer_conditional: bool) -> Vec<T> {
    if prefer_conditional {
        if !conditional.is_empty() {
            return conditional;
        }
        return base;
    }

    if !base.is_empty() {
        return base;
    }
    conditional
}

fn pick_condition_bucket(
    root: &Value,
    player_money: i32,
    profit: i32,
    successful_robberies: u32,
    stopped_at_shaft: bool,
    time_till_death_secs: Option<f32>,
    heist_duration_secs: f32,
) -> Option<Value> {
    let conditions = root.get("conditions")?.as_array()?;
    let mut matches: Vec<&Value> = Vec::new();

    for cond in conditions {
        if condition_matches(
            cond,
            player_money,
            profit,
            successful_robberies,
            stopped_at_shaft,
            time_till_death_secs,
            heist_duration_secs,
        ) {
            matches.push(cond);
        }
    }

    if matches.is_empty() {
        None
    } else {
        let idx = random_number(0..matches.len() as i32) as usize;
        Some(matches[idx].clone())
    }
}

fn condition_matches(
    condition: &Value,
    player_money: i32,
    profit: i32,
    successful_robberies: u32,
    stopped_at_shaft: bool,
    time_till_death_secs: Option<f32>,
    heist_duration_secs: f32,
) -> bool {
    let when = condition.get("when").unwrap_or(&Value::Null);
    let mut has_bound = false;

    if let Some(min) = when.get("profit_min").and_then(Value::as_i64) {
        has_bound = true;
        if profit < min as i32 {
            return false;
        }
    }
    if let Some(max) = when.get("profit_max").and_then(Value::as_i64) {
        has_bound = true;
        if profit > max as i32 {
            return false;
        }
    }
    if let Some(min) = when.get("money_min").and_then(Value::as_i64) {
        has_bound = true;
        if player_money < min as i32 {
            return false;
        }
    }
    if let Some(max) = when.get("money_max").and_then(Value::as_i64) {
        has_bound = true;
        if player_money > max as i32 {
            return false;
        }
    }
    if let Some(min) = when.get("successful_robberies_min").and_then(Value::as_u64) {
        has_bound = true;
        if u64::from(successful_robberies) < min {
            return false;
        }
    }
    if let Some(max) = when.get("successful_robberies_max").and_then(Value::as_u64) {
        has_bound = true;
        if u64::from(successful_robberies) > max {
            return false;
        }
    }
    if let Some(required) = when.get("stopped_at_shaft").and_then(Value::as_bool) {
        has_bound = true;
        if stopped_at_shaft != required {
            return false;
        }
    }
    if let Some(min) = when.get("time_till_death_min").and_then(Value::as_f64) {
        has_bound = true;
        let Some(actual) = time_till_death_secs else {
            return false;
        };
        if (actual as f64) < min {
            return false;
        }
    }
    if let Some(max) = when.get("time_till_death_max").and_then(Value::as_f64) {
        has_bound = true;
        let Some(actual) = time_till_death_secs else {
            return false;
        };
        if (actual as f64) > max {
            return false;
        }
    }
    if let Some(min) = when.get("heist_duration_min").and_then(Value::as_f64) {
        has_bound = true;
        if (heist_duration_secs as f64) < min {
            return false;
        }
    }
    if let Some(max) = when.get("heist_duration_max").and_then(Value::as_f64) {
        has_bound = true;
        if (heist_duration_secs as f64) > max {
            return false;
        }
    }

    has_bound
}

fn collect_newspaper_pool(node: &Value) -> Vec<String> {
    node.get("newspapers")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn collect_dialogue_pool(node: &Value) -> Vec<Value> {
    let Some(dialogue_node) = node.get("dialogue") else {
        return Vec::new();
    };
    if let Some(options) = dialogue_node.as_array() {
        options.clone()
    } else {
        vec![dialogue_node.clone()]
    }
}

fn collect_unconditional_dialogue_pool(scene_node: &Value, root: &Value) -> Vec<Value> {
    if let Some(unconditional) = scene_node.get("unconditional") {
        return collect_dialogue_pool(unconditional);
    }
    if let Some(unconditional) = root.get("unconditional") {
        return collect_dialogue_pool(unconditional);
    }
    Vec::new()
}

fn collect_matching_conditional_dialogue_pool(
    root: &Value,
    player_money: i32,
    profit: i32,
    successful_robberies: u32,
    stopped_at_shaft: bool,
    time_till_death_secs: Option<f32>,
    heist_duration_secs: f32,
) -> Vec<Value> {
    let Some(conditions) = root.get("conditions").and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for cond in conditions {
        if condition_matches(
            cond,
            player_money,
            profit,
            successful_robberies,
            stopped_at_shaft,
            time_till_death_secs,
            heist_duration_secs,
        ) {
            out.extend(collect_dialogue_pool(cond));
        }
    }
    out
}

fn pick_dialogue_line(dialogue_node: &Value, keys: &[&str]) -> String {
    let mut value_opt = None;
    for key in keys {
        if let Some(v) = dialogue_node.get(*key) {
            value_opt = Some(v);
            break;
        }
    }
    let Some(value) = value_opt else {
        return String::new();
    };

    if let Some(single) = value.as_str() {
        return single.to_string();
    }

    if let Some(arr) = value.as_array() {
        let options: Vec<&str> = arr.iter().filter_map(Value::as_str).collect();
        if options.is_empty() {
            return String::new();
        }
        let idx = random_number(0..options.len() as i32) as usize;
        return options[idx].to_string();
    }

    String::new()
}

fn record_heist_report(
    mut messages: MessageReader<HeistReportMessage>,
    mut report: ResMut<LastHeistReport>,
    mut state: ResMut<TavernBubbleState>,
    mut cached: ResMut<CachedTavernTalk>,
    mut receipt_cache: Option<ResMut<ReceiptCache>>,
) {
    for msg in messages.read() {
        report.0 = Some(*msg);
        let receipt = Receipt {
            successful: msg.successful,
            money: msg.money,
            profit: msg.profit,
            successful_robberies: msg.successful_robberies,
            failed_robberies: 0,
            stopped_at_shaft: msg.stopped_at_shaft,
            time_till_death_secs: msg.time_till_death_secs,
            heist_duration_secs: msg.heist_duration_secs,
        };
        let lines_object = if let Some(cache) = receipt_cache.as_deref_mut() {
            cache.get_or_build_lines_object(receipt)
        } else {
            receipt.lines_object()
        };
        if let Some(lines_object) = lines_object {
            cached.0 = Some(TavernTalk::from_lines_object(lines_object, receipt));
        } else {
            cached.0 = None;
        }
        state.applied = false;
    }
}

fn load_tavern_talk_from_json(
    loaded_map: Res<LoadedMap>,
    mut state: ResMut<TavernBubbleState>,
    mut display_ui: ResMut<DisplayUiState>,
    report: Res<LastHeistReport>,
    mut cached: ResMut<CachedTavernTalk>,
    mut receipt_cache: Option<ResMut<ReceiptCache>>,
    mut bank_icon: ResMut<BankIcon>,
    ldtk_entities: LdtkEntityByNameQuery,
    mut commands: Commands,
) {
    if loaded_map_path(&loaded_map) != TAVERN_MAP_PATH {
        state.applied = false;
        return;
    }
    if state.applied {
        return;
    }
    if report.0.is_none() && !state.applied {
        let receipt = Receipt::default();
        let lines_object = if let Some(cache) = receipt_cache.as_deref_mut() {
            cache.get_or_build_lines_object(receipt)
        } else {
            receipt.lines_object()
        };
        cached.0 = lines_object.and_then(TavernTalk::from_intro_lines_object);
    }
    let Some(talk) = &cached.0 else {
        return;
    };

    display_ui.set_article_content(talk.newspaper.clone());
    let guy1_line = if talk.dialogue.guy1.trim().is_empty() {
        "..."
    } else {
        talk.dialogue.guy1.as_str()
    };
    let guy2_line = if talk.dialogue.guy2.trim().is_empty() {
        "..."
    } else {
        talk.dialogue.guy2.as_str()
    };
    let bartender_line = if talk.dialogue.bartender.trim().is_empty() {
        "..."
    } else {
        talk.dialogue.bartender.as_str()
    };

    let should_use_please_dont_rob_us_sign = FORCE_PLEASE_DONT_ROB_US_SIGN
        || bartender_line.contains("Please Don't Rob Us")
        || bartender_line == BANK_SIGN_TRIGGER_LINE;
    *bank_icon = if should_use_please_dont_rob_us_sign {
        BankIcon::PleaseDontRobUs
    } else {
        BankIcon::BlueMoon
    };

    let mut applied_any = false;

    // NOTE:
    // Bar_guy_1 and Bar_guy_2 are reversed.
    // Do not "fix" this.
    // The town has adapted.

    if let Some((entity, _, _)) = ldtk_entities.first_named("Bar_guy_2") {
        commands.entity(entity).insert(TextBubble {
            message: guy1_line.to_string(),
            offset: Vec2::new(0.0, 22.0),
            visible: true,
        });
        applied_any = true;
    }
    if let Some((entity, _, _)) = ldtk_entities.first_named("Bar_guy_1") {
        commands.entity(entity).insert(TextBubble {
            message: guy2_line.to_string(),
            offset: Vec2::new(0.0, 22.0),
            visible: true,
        });
        applied_any = true;
    }

    let bartender_entity = ldtk_entities
        .first_named("Bartender")
        .or_else(|| ldtk_entities.first_named("Bear"))
        .or_else(|| ldtk_entities.first_named("Teller"))
        .or_else(|| ldtk_entities.first_named("Bank_teller"));
    if let Some((entity, _, _)) = bartender_entity {
        commands.entity(entity).insert(TextBubble {
            message: bartender_line.to_string(),
            offset: Vec2::new(0.0, 22.0),
            visible: true,
        });
        applied_any = true;
    }

    // Only mark done if at least one target entity existed; otherwise retry next frame.
    state.applied = applied_any;
}
