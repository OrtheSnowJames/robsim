use bevy::prelude::*;
use bevy::text::{FontWeight, Justify};

use crate::enter_interact::EnterInteractCallbackEvent;
use crate::entity_dialogue::PlayerMovementLock;
use crate::hex_color;
use crate::map::{LdtkEntityByNameQuery, LoadedMap, loaded_map_path};
use crate::player::{LocalPlayer, Player};
use crate::receipts::{ReceiptCache, format_receipt_text};

const TAVERN_MAP_PATH: &str = "maps/tavern.ldtk";
const DISPLAY_OFFICE_MAP_PATH: &str = "maps/newspaper.ldtk";
const RECEIPT_PANEL_WIDTH: f32 = 38.0;
const DEFAULT_PANEL_WIDTH: f32 = 82.0;
const PANEL_HEIGHT: f32 = 84.0;

pub struct DisplayUiPlugin;

impl Plugin for DisplayUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<DisplayUiMessage>()
            .init_resource::<DisplayUiState>()
            .add_systems(Startup, setup_display_ui)
            .add_systems(
                Update,
                (
                    handle_display_ui_messages,
                    handle_display_interact,
                    toggle_display_ui,
                    apply_display_ui_state,
                ),
            );
    }
}

#[derive(Clone, Default)]
pub enum DisplayContent {
    #[default]
    Empty,
    Article(String),
    Receipt {
        article: String,
        receipt_index: usize,
    },
}

impl DisplayContent {
    pub fn article(&self) -> &str {
        match self {
            Self::Empty => "",
            Self::Article(article) => article,
            Self::Receipt { article, .. } => article,
        }
    }

    pub fn receipt_index(&self) -> Option<usize> {
        match self {
            Self::Receipt { receipt_index, .. } => Some(*receipt_index),
            _ => None,
        }
    }
}

#[derive(Message, Clone, Default)]
pub struct DisplayUiMessage {
    pub content: DisplayContent,
    pub open: bool,
}

pub fn show_display_ui(writer: &mut MessageWriter<DisplayUiMessage>, content: DisplayContent) {
    writer.write(DisplayUiMessage {
        content,
        open: true,
    });
}

pub fn set_display_ui(
    writer: &mut MessageWriter<DisplayUiMessage>,
    content: DisplayContent,
    open: bool,
) {
    writer.write(DisplayUiMessage { content, open });
}

pub fn show_article_ui(writer: &mut MessageWriter<DisplayUiMessage>, article: impl Into<String>) {
    show_display_ui(writer, DisplayContent::Article(article.into()));
}

pub fn set_article_ui(
    writer: &mut MessageWriter<DisplayUiMessage>,
    article: impl Into<String>,
    open: bool,
) {
    set_display_ui(writer, DisplayContent::Article(article.into()), open);
}

pub fn show_receipt_ui(
    writer: &mut MessageWriter<DisplayUiMessage>,
    article: impl Into<String>,
    receipt_index: usize,
) {
    show_display_ui(
        writer,
        DisplayContent::Receipt {
            article: article.into(),
            receipt_index,
        },
    );
}

#[derive(Resource, Default)]
pub struct DisplayUiState {
    pub open: bool,
    pub content: DisplayContent,
    pub article_content: String,
}

impl DisplayUiState {
    pub fn set_article_content(&mut self, article: impl Into<String>) {
        let article = article.into();
        self.article_content = article.clone();
        self.content = DisplayContent::Article(article);
    }

    fn has_article_content(&self) -> bool {
        !self.article_content.trim().is_empty()
    }
}

#[derive(Component)]
struct DisplayOverlayRoot;

#[derive(Component)]
struct DisplayHeadlineText;

#[derive(Component)]
struct DisplayBodyText;

#[derive(Component)]
struct DisplayPanel;

fn handle_display_ui_messages(
    mut messages: MessageReader<DisplayUiMessage>,
    mut ui: ResMut<DisplayUiState>,
    mut lock: ResMut<PlayerMovementLock>,
) {
    for msg in messages.read() {
        if let DisplayContent::Article(article) = &msg.content {
            ui.article_content = article.clone();
        }
        ui.content = msg.content.clone();
        ui.open = msg.open;
        lock.active = msg.open;
    }
}

fn setup_display_ui(mut commands: Commands) {
    commands
        .spawn((
            DisplayOverlayRoot,
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
                    DisplayPanel,
                ))
                .with_children(|panel| {
                    panel.spawn((
                        DisplayHeadlineText,
                        Text::new(""),
                        TextFont {
                            font_size: 34.0,
                            weight: FontWeight::BOLD,
                            ..default()
                        },
                        TextLayout::new_with_justify(Justify::Center),
                        TextColor(Color::BLACK),
                    ));
                    panel.spawn((
                        DisplayBodyText,
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

fn set_latest_receipt(ui: &mut DisplayUiState, receipt_cache: Option<&ReceiptCache>) -> bool {
    let Some(cache) = receipt_cache else {
        return false;
    };
    let Some(idx) = cache.all().len().checked_sub(1) else {
        return false;
    };
    let Some(entry) = cache.all().get(idx) else {
        return false;
    };
    ui.content = DisplayContent::Receipt {
        article: format_receipt_text(&entry.receipt),
        receipt_index: idx,
    };
    ui.open = true;
    true
}

fn toggle_display_ui(
    keyboard: Res<ButtonInput<KeyCode>>,
    loaded_map: Res<LoadedMap>,
    _player_q: Query<&Transform, (With<Player>, With<LocalPlayer>)>,
    _ldtk_entities: LdtkEntityByNameQuery,
    mut ui: ResMut<DisplayUiState>,
    receipt_cache: Option<Res<ReceiptCache>>,
    mut lock: ResMut<PlayerMovementLock>,
) {
    let pressed_close = (ui.open && keyboard.just_pressed(KeyCode::Escape))
        || (!ui.open
            && (keyboard.just_pressed(KeyCode::Enter)
                || keyboard.just_pressed(KeyCode::NumpadEnter)));

    if keyboard.just_pressed(KeyCode::KeyR) {
        if set_latest_receipt(&mut ui, receipt_cache.as_deref()) {
            lock.active = true;
        }
        return;
    }

    if ui.open && pressed_close {
        ui.open = false;
        lock.active = false;
        return;
    }

    if ui.open {
        if let Some(current_idx) = ui.content.receipt_index() {
            if let Some(cache) = receipt_cache.as_deref() {
                let len = cache.all().len();
                if len > 0 && keyboard.just_pressed(KeyCode::ArrowLeft) {
                    let next = current_idx.saturating_sub(1);
                    if let Some(entry) = cache.all().get(next) {
                        ui.content = DisplayContent::Receipt {
                            article: format_receipt_text(&entry.receipt),
                            receipt_index: next,
                        };
                    }
                    return;
                }
                if len > 0 && keyboard.just_pressed(KeyCode::ArrowRight) {
                    let next = (current_idx + 1).min(len.saturating_sub(1));
                    if let Some(entry) = cache.all().get(next) {
                        ui.content = DisplayContent::Receipt {
                            article: format_receipt_text(&entry.receipt),
                            receipt_index: next,
                        };
                    }
                    return;
                }
            }
        }
    }

    let in_tavern = loaded_map_path(&loaded_map) == TAVERN_MAP_PATH;
    let in_allowed_premises = loaded_map_path(&loaded_map) == DISPLAY_OFFICE_MAP_PATH || in_tavern;
    if !in_allowed_premises {
        if ui.open && ui.content.receipt_index().is_none() {
            ui.open = false;
            lock.active = false;
        }
        return;
    }

    let _ = lock;
}

fn handle_display_interact(
    mut events: MessageReader<EnterInteractCallbackEvent>,
    mut ui: ResMut<DisplayUiState>,
    receipt_cache: Option<Res<ReceiptCache>>,
    mut lock: ResMut<PlayerMovementLock>,
) {
    for ev in events.read() {
        match *ev {
            EnterInteractCallbackEvent::OpenDisplay(entity) => {
                let _ = entity;
                if !ui.has_article_content() {
                    continue;
                }
                ui.content = DisplayContent::Article(ui.article_content.clone());
                if ui.open && ui.content.receipt_index().is_none() {
                    ui.open = false;
                    lock.active = false;
                } else {
                    ui.open = true;
                    lock.active = true;
                }
            }
            EnterInteractCallbackEvent::OpenReceipts(entity) => {
                let _ = entity;
                if set_latest_receipt(&mut ui, receipt_cache.as_deref()) {
                    lock.active = true;
                }
            }
        }
    }
}

fn apply_display_ui_state(
    ui: Res<DisplayUiState>,
    mut lock: ResMut<PlayerMovementLock>,
    mut root_q: Query<&mut Visibility, With<DisplayOverlayRoot>>,
    mut panel_q: Query<&mut Node, With<DisplayPanel>>,
    mut text_qs: ParamSet<(
        Query<&mut Text, With<DisplayHeadlineText>>,
        Query<&mut Text, With<DisplayBodyText>>,
    )>,
) {
    let Ok(mut vis) = root_q.single_mut() else {
        return;
    };

    if ui.open {
        if let Ok(mut panel) = panel_q.single_mut() {
            panel.width = if ui.content.receipt_index().is_some() {
                Val::Percent(RECEIPT_PANEL_WIDTH)
            } else {
                Val::Percent(DEFAULT_PANEL_WIDTH)
            };
            panel.height = Val::Percent(PANEL_HEIGHT);
        }
        let (headline, body) = split_headline_and_body(ui.content.article());
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
