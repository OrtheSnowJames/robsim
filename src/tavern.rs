use serde_json::Value;
use rand::RngExt;
use bevy::prelude::*;
use bevy::text::{FontWeight, Justify};
use crate::bank::img_layer::BankIcon;
use crate::enter_interact::EnterInteractCallbackEvent;
use crate::entity_dialogue::PlayerMovementLock;
use crate::map::{loaded_map_path, LdtkEntityByNameQuery, LoadedMap};
use crate::player::Player;
use crate::receipts::{format_receipt_text, Receipt, ReceiptCache};
use crate::text_bubble::TextBubble;
use crate::hex_color;

const TAVERN_MAP_PATH: &str = "maps/tavern.ldtk";
const NEWSPAPER_OFFICE_MAP_PATH: &str = "maps/newspaper.ldtk";
const BANK_SIGN_TRIGGER_LINE: &str = "A slightly larger sign saying 'Please Don't Rob Us.'";
const FORCE_PLEASE_DONT_ROB_US_SIGN: bool = false;
const CONDITIONAL_POOL_PICK_CHANCE: f64 = 0.5;
const RECEIPT_PANEL_WIDTH: f32 = 38.0;
const DEFAULT_PANEL_WIDTH: f32 = 82.0;
const PANEL_HEIGHT: f32 = 84.0;

pub struct TavernPlugin;

impl Plugin for TavernPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<HeistReportMessage>()
            .add_message::<NewspaperUiMessage>()
            .init_resource::<LastHeistReport>()
            .init_resource::<TavernBubbleState>()
            .init_resource::<CachedTavernTalk>()
            .init_resource::<NewspaperUiState>()
            .add_systems(Startup, setup_newspaper_ui)
            .add_systems(
                Update,
                (
                    record_heist_report,
                    load_tavern_talk_from_json,
                    handle_newspaper_ui_messages,
                    handle_newspaper_entity_interact,
                    toggle_newspaper_ui,
                    apply_newspaper_ui_state,
                ),
            );
    }
}

#[derive(Message, Clone)]
pub struct NewspaperUiMessage {
    pub article: String,
    pub open: bool,
}

pub fn show_newspaper_ui(writer: &mut MessageWriter<NewspaperUiMessage>, article: impl Into<String>) {
    writer.write(NewspaperUiMessage {
        article: article.into(),
        open: true,
    });
}

pub fn set_newspaper_ui(
    writer: &mut MessageWriter<NewspaperUiMessage>,
    article: impl Into<String>,
    open: bool,
) {
    writer.write(NewspaperUiMessage {
        article: article.into(),
        open,
    });
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

#[derive(Resource, Default)]
struct NewspaperUiState {
    open: bool,
    article: String,
    receipt_index: Option<usize>,
}

#[derive(Component)]
struct NewspaperOverlayRoot;

#[derive(Component)]
struct NewspaperHeadlineText;

#[derive(Component)]
struct NewspaperBodyText;

#[derive(Component)]
struct NewspaperPaperPanel;

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
    let mut rng = rand::rng();
    rng.random_range(range)
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
        let prefer_conditional = rand::rng().random_bool(CONDITIONAL_POOL_PICK_CHANCE);
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

        // Dialogue pool order:
        // 1) unconditional
        // 2) successful/unsuccessful generic dialogue
        // 3) conditional dialogue
        // 4) random pick from final pool
        let mut dialogue_pool = collect_unconditional_dialogue_pool(&first, &json_val);
        dialogue_pool.extend(collect_dialogue_pool(&first));
        dialogue_pool.extend(collect_matching_conditional_dialogue_pool(
            &first,
            receipt.money,
            receipt.profit,
            receipt.successful_robberies,
            receipt.stopped_at_shaft,
            receipt.time_till_death_secs,
            receipt.heist_duration_secs,
        ));
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

        Self { newspaper, dialogue }
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
    let Some(value) = value_opt else { return String::new(); };

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
            cached.0 = Some(TavernTalk::from_lines_object(
                lines_object,
                receipt,
            ));
        } else {
            cached.0 = None;
        }
        state.applied = false;
    }
}

fn load_tavern_talk_from_json(
    loaded_map: Res<LoadedMap>,
    mut state: ResMut<TavernBubbleState>,
    mut newspaper_ui: ResMut<NewspaperUiState>,
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

    newspaper_ui.article = talk.newspaper.clone();
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

fn handle_newspaper_ui_messages(
    mut messages: MessageReader<NewspaperUiMessage>,
    mut ui: ResMut<NewspaperUiState>,
    mut lock: ResMut<PlayerMovementLock>,
) {
    for msg in messages.read() {
        ui.article = msg.article.clone();
        ui.open = msg.open;
        ui.receipt_index = None;
        lock.active = msg.open;
    }
}

fn setup_newspaper_ui(mut commands: Commands) {
    commands
        .spawn((
            NewspaperOverlayRoot,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                top: Val::Px(0.0),
                bottom: Val::Px(0.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.72)),
            Visibility::Hidden,
            ZIndex(20_000),
        ))
        .with_children(|parent| {
            parent
                .spawn((
                    Node {
                        width: Val::Percent(DEFAULT_PANEL_WIDTH),
                        height: Val::Percent(PANEL_HEIGHT),
                        flex_direction: FlexDirection::Column,
                        border: UiRect::all(Val::Px(3.0)),
                        padding: UiRect::all(Val::Px(16.0)),
                        row_gap: Val::Px(10.0),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.96, 0.94, 0.88)),
                    BorderColor::all(hex_color!(0x9c7474)),
                    NewspaperPaperPanel,
                ))
                .with_children(|paper| {
                    paper.spawn((
                        NewspaperHeadlineText,
                        Text::new(""),
                        TextFont {
                            font_size: 34.0,
                            weight: FontWeight::BOLD,
                            ..default()
                        },
                        TextLayout::new_with_justify(Justify::Center),
                        TextColor(Color::BLACK),
                    ));
                    paper.spawn((
                        NewspaperBodyText,
                        Text::new(""),
                        TextFont {
                            font_size: 24.0,
                            ..default()
                        },
                        TextLayout::new_with_justify(Justify::Left),
                        TextColor(Color::BLACK),
                    ));
                });
        });
}

fn split_headline_and_body(article: &str) -> (String, String) {
    let lines: Vec<&str> = article.lines().collect();
    if lines.is_empty() {
        return ("".to_string(), "".to_string());
    }
    let first = lines.first().copied().unwrap_or_default();
    let second = lines.get(1).copied().unwrap_or_default();
    let headline = if second.is_empty() {
        first.to_string()
    } else {
        format!("{first}\n{second}")
    };
    let body = if lines.len() > 2 {
        lines[2..].join("\n")
    } else {
        "".to_string()
    };
    (headline, body)
}

fn toggle_newspaper_ui(
    keyboard: Res<ButtonInput<KeyCode>>,
    loaded_map: Res<LoadedMap>,
    _player_q: Query<&Transform, With<Player>>,
    _ldtk_entities: LdtkEntityByNameQuery,
    mut ui: ResMut<NewspaperUiState>,
    receipt_cache: Option<Res<ReceiptCache>>,
    mut lock: ResMut<PlayerMovementLock>,
) {
    let pressed_close = (ui.open && keyboard.just_pressed(KeyCode::Escape))
        || (!ui.open && (keyboard.just_pressed(KeyCode::Enter) || keyboard.just_pressed(KeyCode::NumpadEnter)));

    if keyboard.just_pressed(KeyCode::KeyR) {
        let latest_idx = receipt_cache
            .as_deref()
            .and_then(|cache| cache.all().len().checked_sub(1));
        if let Some(idx) = latest_idx {
            if let Some(cache) = receipt_cache.as_deref() {
                if let Some(entry) = cache.all().get(idx) {
                    ui.article = format_receipt_text(&entry.receipt);
                    ui.receipt_index = Some(idx);
                    ui.open = true;
                    lock.active = true;
                }
            }
        }
        return;
    }

    if ui.open && pressed_close {
        ui.open = false;
        ui.receipt_index = None;
        lock.active = false;
        return;
    }

    if ui.open {
        if let Some(current_idx) = ui.receipt_index {
            if let Some(cache) = receipt_cache.as_deref() {
                let len = cache.all().len();
                if len > 0 && keyboard.just_pressed(KeyCode::ArrowLeft) {
                    let next = current_idx.saturating_sub(1);
                    if let Some(entry) = cache.all().get(next) {
                        ui.receipt_index = Some(next);
                        ui.article = format_receipt_text(&entry.receipt);
                    }
                    return;
                }
                if len > 0 && keyboard.just_pressed(KeyCode::ArrowRight) {
                    let next = (current_idx + 1).min(len.saturating_sub(1));
                    if let Some(entry) = cache.all().get(next) {
                        ui.receipt_index = Some(next);
                        ui.article = format_receipt_text(&entry.receipt);
                    }
                    return;
                }
            }
        }
    }

    let in_tavern = loaded_map_path(&loaded_map) == TAVERN_MAP_PATH;
    let in_allowed_premises = loaded_map_path(&loaded_map) == NEWSPAPER_OFFICE_MAP_PATH || in_tavern;
    if !in_allowed_premises {
        // Keep receipt mode usable outside the tavern/newspaper office.
        if ui.open && ui.receipt_index.is_none() {
            ui.open = false;
            ui.receipt_index = None;
            lock.active = false;
        }
        return;
    }

    let _ = lock;
}

fn handle_newspaper_entity_interact(
    mut events: MessageReader<EnterInteractCallbackEvent>,
    mut ui: ResMut<NewspaperUiState>,
    receipt_cache: Option<Res<ReceiptCache>>,
    mut lock: ResMut<PlayerMovementLock>,
) {
    for ev in events.read() {
        match *ev {
            EnterInteractCallbackEvent::OpenNewspaper(entity) => {
                let _ = entity;
                if ui.article.trim().is_empty() {
                    continue;
                }
                if ui.open && ui.receipt_index.is_none() {
                    ui.open = false;
                    lock.active = false;
                } else {
                    ui.open = true;
                    ui.receipt_index = None;
                    lock.active = true;
                }
            }
            EnterInteractCallbackEvent::OpenReceipts(entity) => {
                let _ = entity;
                let latest_idx = receipt_cache
                    .as_deref()
                    .and_then(|cache| cache.all().len().checked_sub(1));
                let Some(idx) = latest_idx else {
                    continue;
                };
                let Some(cache) = receipt_cache.as_deref() else {
                    continue;
                };
                let Some(entry) = cache.all().get(idx) else {
                    continue;
                };
                ui.article = format_receipt_text(&entry.receipt);
                ui.receipt_index = Some(idx);
                ui.open = true;
                lock.active = true;
            }
        }
    }
}

fn apply_newspaper_ui_state(
    ui: Res<NewspaperUiState>,
    mut lock: ResMut<PlayerMovementLock>,
    mut root_q: Query<&mut Visibility, With<NewspaperOverlayRoot>>,
    mut panel_q: Query<&mut Node, With<NewspaperPaperPanel>>,
    mut text_qs: ParamSet<(
        Query<&mut Text, With<NewspaperHeadlineText>>,
        Query<&mut Text, With<NewspaperBodyText>>,
    )>,
) {
    let Ok(mut vis) = root_q.single_mut() else {
        return;
    };

    if ui.open {
        if let Ok(mut panel) = panel_q.single_mut() {
            panel.width = if ui.receipt_index.is_some() {
                Val::Percent(RECEIPT_PANEL_WIDTH)
            } else {
                Val::Percent(DEFAULT_PANEL_WIDTH)
            };
            panel.height = Val::Percent(PANEL_HEIGHT);
        }
        let (headline, body) = split_headline_and_body(&ui.article);
        {
            let mut headline_q = text_qs.p0();
            let Ok(mut headline_text) = headline_q.single_mut() else {
                return;
            };
            headline_text.0 = headline;
        }
        {
            let mut body_q = text_qs.p1();
            let Ok(mut body_text) = body_q.single_mut() else {
                return;
            };
            body_text.0 = body;
        }
        *vis = Visibility::Visible;
        lock.active = true;
    } else {
        *vis = Visibility::Hidden;
    }
}
