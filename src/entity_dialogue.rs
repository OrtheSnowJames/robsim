use crate::map::{LoadedMap, SceneChangeRequest, loaded_map_path};
use crate::nine_slicing::NineSliceBorder;
use crate::player::{LocalPlayer, Player};
use crate::prompt_key::KeyPrompt;
use bevy::prelude::*;
use bevy::ui::widget::NodeImageMode;
use bevy_ecs_ldtk::EntityInstance;
use std::collections::HashSet;

const INTERACT_RADIUS: f32 = 28.0;
const LARGE_BUILDING_INTERACT_RADIUS: f32 = 120.0;
const DIALOGUE_CHARS_PER_SECOND: f32 = 48.0;

pub struct EntityDialoguePlugin;

impl Plugin for EntityDialoguePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ActiveDialogue>()
            .init_resource::<TextboxMode>()
            .init_resource::<PlayerMovementLock>()
            .init_resource::<DialogueUiState>()
            .init_resource::<DialogueEnabled>()
            .init_resource::<DisabledEntityDialogues>()
            .add_systems(Startup, setup_dialogue_ui)
            .add_systems(
                Update,
                (
                    attach_dialogue_prompts,
                    handle_entity_dialogue,
                    animate_dialogue_text,
                    apply_dialogue_ui_state,
                ),
            );
    }
}

#[derive(Resource)]
pub struct DialogueEnabled {
    pub enabled: bool,
}

impl Default for DialogueEnabled {
    fn default() -> Self {
        Self { enabled: true }
    }
}

#[derive(Resource, Default)]
pub struct DisabledEntityDialogues(HashSet<String>);

impl DisabledEntityDialogues {
    pub fn disable(&mut self, identifier: &str) {
        self.0.insert(identifier.to_string());
    }

    pub fn is_disabled(&self, identifier: &str) -> bool {
        self.0
            .iter()
            .any(|disabled| disabled.eq_ignore_ascii_case(identifier))
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }
}

#[derive(Clone, Copy)]
enum DialogueNode {
    Line {
        text: &'static str,
        next: Option<usize>,
    },
    YesNo {
        prompt: &'static str,
        yes_next: usize,
        no_next: usize,
    },
}

#[derive(Clone, Copy)]
struct EntityDialogueSpec {
    identifier: &'static str,
    start: usize,
    nodes: &'static [DialogueNode],
    interact_radius: f32,
    interact_offset: Vec2,
    on_complete_scene: Option<&'static str>,
}

#[derive(Resource, Default)]
struct ActiveDialogue {
    session: Option<DialogueSession>,
    hold_until_map_change_from: Option<String>,
}

#[derive(Resource, Default)]
pub struct PlayerMovementLock {
    pub active: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TextboxCloseMode {
    OnEnter,
    OnMapChange,
}

#[derive(Resource)]
pub struct TextboxMode(pub TextboxCloseMode);

impl Default for TextboxMode {
    fn default() -> Self {
        Self(TextboxCloseMode::OnEnter)
    }
}

#[derive(Resource)]
struct DialogueUiState {
    visible: bool,
    full_text: String,
    revealed_chars: usize,
    reveal_timer: Timer,
}

impl DialogueUiState {
    fn display_text(&self) -> String {
        self.full_text.chars().take(self.revealed_chars).collect()
    }

    fn is_fully_revealed(&self) -> bool {
        self.revealed_chars >= self.full_text.chars().count()
    }

    fn reveal_all(&mut self) {
        self.revealed_chars = self.full_text.chars().count();
        self.reveal_timer.reset();
    }
}

impl Default for DialogueUiState {
    fn default() -> Self {
        Self {
            visible: false,
            full_text: String::new(),
            revealed_chars: 0,
            reveal_timer: Timer::from_seconds(
                1.0 / DIALOGUE_CHARS_PER_SECOND,
                TimerMode::Repeating,
            ),
        }
    }
}

#[derive(Clone, Copy)]
struct DialogueSession {
    spec_idx: usize,
    node_idx: usize,
}

#[derive(Component)]
struct DialoguePromptAttached;

#[derive(Component)]
struct DialogueUiRoot;

#[derive(Component)]
struct DialogueUiText;

const BARTENDER_DIALOGUE: &[DialogueNode] = &[
    DialogueNode::Line {
        text: "You here for a drink or to stare at the wall in silence?",
        next: Some(1),
    },
    DialogueNode::YesNo {
        prompt: "Want the house special?",
        yes_next: 2,
        no_next: 3,
    },
    DialogueNode::Line {
        text: "Good choice. Tastes terrible, builds character.",
        next: None,
    },
    DialogueNode::Line {
        text: "Fair. Water's cheaper.",
        next: None,
    },
];

const BAR_GUY_1_DIALOGUE: &[DialogueNode] = &[
    DialogueNode::Line {
        text: "I counted 43 ceiling planks in here. Might've missed two.",
        next: Some(1),
    },
    DialogueNode::Line {
        text: "Don't ask why I counted. It was a long afternoon.",
        next: None,
    },
];

const BAR_GUY_2_DIALOGUE: &[DialogueNode] = &[
    DialogueNode::Line {
        text: "You ever hear a chair squeak and assume it's haunted?",
        next: Some(1),
    },
    DialogueNode::YesNo {
        prompt: "Do pickles belong on stew?",
        yes_next: 2,
        no_next: 3,
    },
    DialogueNode::Line {
        text: "Correct answer. Finally, a person of culture.",
        next: None,
    },
    DialogueNode::Line {
        text: "Wrong answer, but I'll forgive you this once.",
        next: None,
    },
];

const SOUP_STORE_DIALOGUE: &[DialogueNode] = &[
    DialogueNode::Line {
        text: "It's a soup store.",
        next: Some(1),
    },
    DialogueNode::Line {
        text: "It's locked.",
        next: Some(2),
    },
    DialogueNode::Line {
        text: "I heard you can enter via the vault...",
        next: None,
    },
];

const SOUP_MAN_DIALOGUE: &[DialogueNode] = &[
    DialogueNode::Line {
        text: "Welcome to Martha's soup store- wait. How did you even get in here?!",
        next: Some(1),
    },
    DialogueNode::Line {
        text: "Anyway, please don't drink the soup. It's still in the making.",
        next: None,
    },
];

const SOUP_DIALOGUE: &[DialogueNode] = &[
    DialogueNode::YesNo {
        prompt: "Drink the soup?",
        yes_next: 1,
        no_next: 3,
    },
    DialogueNode::Line {
        text: "You drank the soup. Tasted horrible. You spat it out.",
        next: Some(2),
    },
    DialogueNode::Line {
        text: "Soup guy: HEY!!",
        next: None,
    },
    DialogueNode::Line {
        text: "You decided not to drink the soup. :(",
        next: None,
    },
];

const JAIL_GUARD_DIALOGUE: &[DialogueNode] = &[
    DialogueNode::YesNo {
        prompt: "Do you wanna leave?",
        yes_next: 1,
        no_next: 4,
    },
    DialogueNode::YesNo {
        prompt: "Are you sure you won't rob the bank anymore?",
        yes_next: 2,
        no_next: 3,
    },
    DialogueNode::Line {
        text: "You're free to go!",
        next: None,
    },
    DialogueNode::Line {
        text: "Then you stay in here.",
        next: None,
    },
    DialogueNode::Line {
        text: "Then enjoy your cell.",
        next: None,
    },
];

const TELLER_DIALOGUE: &[DialogueNode] = &[
    DialogueNode::Line {
        text: "Welcome to Blue Moon Bank.",
        next: Some(1),
    },
    DialogueNode::YesNo {
        prompt: "Are you planning to rob the bank today?",
        yes_next: 2,
        no_next: 5,
    },
    DialogueNode::Line {
        text: "I appreciate the honesty.",
        next: Some(3),
    },
    DialogueNode::Line {
        text: "Most people just wait until they're inside.",
        next: Some(4),
    },
    DialogueNode::Line {
        text: "Good luck, I suppose.",
        next: None,
    },
    DialogueNode::Line {
        text: "That's what they all say.",
        next: Some(6),
    },
    DialogueNode::Line {
        text: "Enjoy your visit.",
        next: None,
    },
];

const NEWSPAPERER_DIALOGUE: &[DialogueNode] = &[
    DialogueNode::Line {
        text: "I've always wondered who the robber is.",
        next: Some(1),
    },
    DialogueNode::YesNo {
        prompt: "Would you tell me if you knew?",
        yes_next: 2,
        no_next: 5,
    },
    DialogueNode::Line {
        text: "Interesting.",
        next: Some(3),
    },
    DialogueNode::Line {
        text: "That's exactly what an honest person would say.",
        next: Some(4),
    },
    DialogueNode::Line {
        text: "Probably.",
        next: None,
    },
    DialogueNode::Line {
        text: "Also interesting.",
        next: Some(6),
    },
    DialogueNode::Line {
        text: "That's exactly what a robber would say.",
        next: None,
    },
];

const PRINTER_DIALOGUE: &[DialogueNode] = &[
    DialogueNode::Line {
        text: "The printer is warm.",
        next: Some(1),
    },
    DialogueNode::Line {
        text: "Another robbery must have happened recently.",
        next: None,
    },
];

const GREMLIN_TUTORIAL_DIALOGUE: &[DialogueNode] = &[
    DialogueNode::YesNo {
        prompt: "Want to see a tutorial?",
        yes_next: 1,
        no_next: 7,
    },
    DialogueNode::Line {
        text: "Tutorial time.\n\nEnter Blue Moon Bank and press F to open the vault.",
        next: Some(2),
    },
    DialogueNode::Line {
        text: "Then rob the bank while guards chase you.\nUse the radar in the top-right corner to track them.",
        next: Some(3),
    },
    DialogueNode::Line {
        text: "If you succeed, you keep the money.",
        next: Some(4),
    },
    DialogueNode::Line {
        text: "That also unlocks new tavern dialogue and a newspaper article.",
        next: Some(5),
    },
    DialogueNode::Line {
        text: "You can review your robberies later at the house at the bottom of town.",
        next: Some(6),
    },
    DialogueNode::Line {
        text: "If you get caught, the guards will jail you.\nThen you can ask nicely to be let out.",
        next: Some(8),
    },
    DialogueNode::Line {
        text: "Maybe later. Raw instinct is a valid learning strategy.",
        next: None,
    },
    DialogueNode::Line {
        text: "After that, visit the tavern to read the newspaper and hear what the locals say.",
        next: None,
    },
];

const ENTITY_DIALOGUE_SPECS: &[EntityDialogueSpec] = &[
    EntityDialogueSpec {
        identifier: "Bartender",
        start: 0,
        nodes: BARTENDER_DIALOGUE,
        interact_radius: INTERACT_RADIUS,
        interact_offset: Vec2::ZERO,
        on_complete_scene: None,
    },
    EntityDialogueSpec {
        identifier: "Teller",
        start: 0,
        nodes: TELLER_DIALOGUE,
        interact_radius: INTERACT_RADIUS,
        interact_offset: Vec2::ZERO,
        on_complete_scene: None,
    },
    EntityDialogueSpec {
        identifier: "Bear",
        start: 0,
        nodes: TELLER_DIALOGUE,
        interact_radius: INTERACT_RADIUS,
        interact_offset: Vec2::ZERO,
        on_complete_scene: None,
    },
    EntityDialogueSpec {
        identifier: "Bar_guy_1",
        start: 0,
        nodes: BAR_GUY_1_DIALOGUE,
        interact_radius: INTERACT_RADIUS,
        interact_offset: Vec2::ZERO,
        on_complete_scene: None,
    },
    EntityDialogueSpec {
        identifier: "Bar_guy_2",
        start: 0,
        nodes: BAR_GUY_2_DIALOGUE,
        interact_radius: INTERACT_RADIUS,
        interact_offset: Vec2::ZERO,
        on_complete_scene: None,
    },
    EntityDialogueSpec {
        identifier: "Soup_store",
        start: 0,
        nodes: SOUP_STORE_DIALOGUE,
        interact_radius: LARGE_BUILDING_INTERACT_RADIUS,
        // LDtk entity transforms are top-left aligned in some setups;
        // soup store visual is 64x64, so anchor prompt/dialogue near visual center/front.
        interact_offset: Vec2::new(0.0, -32.0),
        on_complete_scene: None,
    },
    EntityDialogueSpec {
        identifier: "Soup",
        start: 0,
        nodes: SOUP_DIALOGUE,
        interact_radius: INTERACT_RADIUS,
        interact_offset: Vec2::ZERO,
        on_complete_scene: None,
    },
    EntityDialogueSpec {
        identifier: "Soup_guy",
        start: 0,
        nodes: SOUP_MAN_DIALOGUE,
        interact_radius: INTERACT_RADIUS,
        interact_offset: Vec2::ZERO,
        on_complete_scene: None,
    },
    EntityDialogueSpec {
        identifier: "Jail_guard",
        start: 0,
        nodes: JAIL_GUARD_DIALOGUE,
        interact_radius: INTERACT_RADIUS,
        interact_offset: Vec2::ZERO,
        on_complete_scene: Some("maps/town.ldtk"),
    },
    EntityDialogueSpec {
        identifier: "Newspaperer",
        start: 0,
        nodes: NEWSPAPERER_DIALOGUE,
        interact_radius: INTERACT_RADIUS / 1.5,
        interact_offset: Vec2::ZERO,
        on_complete_scene: None,
    },
    EntityDialogueSpec {
        identifier: "Printer",
        start: 0,
        nodes: PRINTER_DIALOGUE,
        interact_radius: INTERACT_RADIUS / 1.5,
        interact_offset: Vec2::ZERO,
        on_complete_scene: None,
    },
    EntityDialogueSpec {
        identifier: "Gremlin",
        start: 0,
        nodes: GREMLIN_TUTORIAL_DIALOGUE,
        interact_radius: INTERACT_RADIUS,
        interact_offset: Vec2::ZERO,
        on_complete_scene: None,
    },
];

fn check_dialogue_spec_duplicates() {
    let mut identifiers = HashSet::new();
    for spec in ENTITY_DIALOGUE_SPECS {
        if identifiers.contains(spec.identifier) {
            panic!("Duplicate dialogue spec identifier: {}", spec.identifier);
        }
        identifiers.insert(spec.identifier);
    }
}

fn find_spec_index(identifier: &str) -> Option<usize> {
    ENTITY_DIALOGUE_SPECS
        .iter()
        .position(|spec| spec.identifier.eq_ignore_ascii_case(identifier))
}

fn node_text(node: DialogueNode) -> String {
    match node {
        DialogueNode::Line { text, .. } => text.to_string(),
        DialogueNode::YesNo { prompt, .. } => format!("{prompt}\n[Y] Yes  [N] No"),
    }
}

fn set_dialogue_text(ui: &mut DialogueUiState, text: String) {
    ui.visible = true;
    ui.full_text = text;
    ui.revealed_chars = 0;
    ui.reveal_timer.reset();
}

fn clear_dialogue_text(ui: &mut DialogueUiState) {
    ui.visible = false;
    ui.full_text.clear();
    ui.revealed_chars = 0;
    ui.reveal_timer.reset();
}

fn animate_dialogue_text(time: Res<Time>, mut ui: ResMut<DialogueUiState>) {
    if !ui.visible || ui.is_fully_revealed() {
        return;
    }

    ui.reveal_timer.tick(time.delta());
    let text_len = ui.full_text.chars().count();
    let revealed_this_tick = ui.reveal_timer.times_finished_this_tick() as usize;
    if revealed_this_tick > 0 {
        ui.revealed_chars = (ui.revealed_chars + revealed_this_tick).min(text_len);
    }
}

fn setup_dialogue_ui(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands
        .spawn((
            DialogueUiRoot,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(24.0),
                right: Val::Px(24.0),
                bottom: Val::Px(20.0),
                min_height: Val::Px(96.0),
                padding: UiRect::all(Val::Px(12.0)),
                border: UiRect::all(Val::Px(0.0)),
                ..default()
            },
            BackgroundColor(Color::NONE),
            BorderColor::all(Color::NONE),
            ImageNode::new(asset_server.load("bubble.png")).with_mode(NodeImageMode::Sliced(
                TextureSlicer {
                    border: BorderRect::all(6.0),
                    center_scale_mode: SliceScaleMode::Stretch,
                    sides_scale_mode: SliceScaleMode::Stretch,
                    max_corner_scale: 1.0,
                },
            )),
            NineSliceBorder {
                border_insets: Vec4::splat(6.0),
            },
            Visibility::Hidden,
            ZIndex(9999),
        ))
        .with_children(|parent| {
            parent.spawn((
                DialogueUiText,
                Text::new(""),
                TextFont {
                    font_size: 32.0,
                    ..default()
                },
                TextColor(Color::WHITE),
            ));
        });
    info!("Dialogue UI uses bubble.png with 6px nine-slice border.");
}

fn apply_dialogue_ui_state(
    ui: Res<DialogueUiState>,
    mut root_q: Query<&mut Visibility, With<DialogueUiRoot>>,
    mut text_q: Query<&mut Text, With<DialogueUiText>>,
) {
    let Ok(mut root_vis) = root_q.single_mut() else {
        return;
    };
    let Ok(mut text) = text_q.single_mut() else {
        return;
    };

    *root_vis = if ui.visible {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
    text.0 = ui.display_text();
}

fn attach_dialogue_prompts(
    mut commands: Commands,
    query: Query<
        (Entity, &EntityInstance),
        (Added<EntityInstance>, Without<DialoguePromptAttached>),
    >,
    has_query: Query<Entity, With<DialoguePromptAttached>>,
    dialogue_enabled: Res<DialogueEnabled>,
    disabled_dialogues: Res<DisabledEntityDialogues>,
) {
    check_dialogue_spec_duplicates();

    if !dialogue_enabled.enabled {
        for entity in has_query.iter() {
            commands
                .entity(entity)
                .remove::<KeyPrompt>()
                .remove::<DialoguePromptAttached>();
        }
    }

    for (entity, instance) in &query {
        if disabled_dialogues.is_disabled(&instance.identifier) {
            continue;
        }
        if let Some(spec_idx) = find_spec_index(&instance.identifier) {
            let spec = ENTITY_DIALOGUE_SPECS[spec_idx];
            let half_extents = entity_half_extents(instance);
            commands.entity(entity).insert((
                KeyPrompt {
                    key: "ENTER".to_string(),
                    radius: spec.interact_radius,
                    world_offset: spec.interact_offset,
                    half_extents,
                },
                DialoguePromptAttached,
            ));
        }
    }
}

fn handle_entity_dialogue(
    keyboard: Res<ButtonInput<KeyCode>>,
    loaded_map: Res<LoadedMap>,
    textbox_mode: Res<TextboxMode>,
    player_q: Query<&Transform, (With<Player>, With<LocalPlayer>)>,
    entities: Query<(Entity, &EntityInstance, &Transform)>,
    dialogue_enabled: Res<DialogueEnabled>,
    disabled_dialogues: Res<DisabledEntityDialogues>,
    mut active: ResMut<ActiveDialogue>,
    mut lock: ResMut<PlayerMovementLock>,
    mut ui: ResMut<DialogueUiState>,
    mut scene_change: MessageWriter<SceneChangeRequest>,
) {
    if !dialogue_enabled.enabled {
        return;
    }

    if let Some(source_map) = &active.hold_until_map_change_from {
        lock.active = true;
        if loaded_map_path(&loaded_map) != source_map {
            active.hold_until_map_change_from = None;
            clear_dialogue_text(ui.as_mut());
            lock.active = false;
        }
        return;
    }

    let Ok(player_tf) = player_q.single() else {
        return;
    };
    let player_pos = player_tf.translation.truncate();

    if let Some(mut session) = active.session {
        lock.active = true;
        let spec = ENTITY_DIALOGUE_SPECS[session.spec_idx];
        let Some(node) = spec.nodes.get(session.node_idx).copied() else {
            clear_dialogue_text(ui.as_mut());
            lock.active = false;
            active.session = None;
            return;
        };

        let should_advance =
            keyboard.just_pressed(KeyCode::Enter) || keyboard.just_pressed(KeyCode::NumpadEnter);

        match node {
            DialogueNode::Line { next, .. } => {
                if should_advance {
                    if !ui.is_fully_revealed() {
                        ui.reveal_all();
                        active.session = Some(session);
                        return;
                    }
                    if let Some(next_idx) = next {
                        session.node_idx = next_idx;
                        if let Some(next_node) = spec.nodes.get(session.node_idx).copied() {
                            set_dialogue_text(ui.as_mut(), node_text(next_node));
                            active.session = Some(session);
                        } else {
                            clear_dialogue_text(ui.as_mut());
                            lock.active = false;
                            active.session = None;
                        }
                    } else {
                        active.session = None;
                        if let Some(scene) = spec.on_complete_scene {
                            scene_change.write(SceneChangeRequest {
                                asset_path: scene.to_string(),
                            });
                        }
                        if textbox_mode.0 == TextboxCloseMode::OnMapChange {
                            active.hold_until_map_change_from =
                                Some(loaded_map_path(&loaded_map).to_string());
                            lock.active = true;
                        } else {
                            clear_dialogue_text(ui.as_mut());
                            lock.active = false;
                        }
                    }
                } else {
                    active.session = Some(session);
                }
            }
            DialogueNode::YesNo {
                yes_next, no_next, ..
            } => {
                if should_advance && !ui.is_fully_revealed() {
                    ui.reveal_all();
                    active.session = Some(session);
                    return;
                }
                if !ui.is_fully_revealed() {
                    active.session = Some(session);
                    return;
                }
                let mut next_idx = None;
                if keyboard.just_pressed(KeyCode::KeyY) {
                    next_idx = Some(yes_next);
                } else if keyboard.just_pressed(KeyCode::KeyN) {
                    next_idx = Some(no_next);
                }

                if let Some(idx) = next_idx {
                    session.node_idx = idx;
                    if let Some(next_node) = spec.nodes.get(session.node_idx).copied() {
                        set_dialogue_text(ui.as_mut(), node_text(next_node));
                        active.session = Some(session);
                    } else {
                        clear_dialogue_text(ui.as_mut());
                        lock.active = false;
                        active.session = None;
                    }
                } else {
                    active.session = Some(session);
                }
            }
        }
        return;
    }

    let pressed_enter =
        keyboard.just_pressed(KeyCode::Enter) || keyboard.just_pressed(KeyCode::NumpadEnter);
    if !pressed_enter {
        return;
    }

    let mut best: Option<(Entity, usize, f32)> = None;
    for (entity, instance, tf) in &entities {
        if disabled_dialogues.is_disabled(&instance.identifier) {
            continue;
        }
        let Some(spec_idx) = find_spec_index(&instance.identifier) else {
            continue;
        };
        let spec = ENTITY_DIALOGUE_SPECS[spec_idx];
        let anchor = tf.translation.truncate() + spec.interact_offset;
        let d = distance_to_entity_interact_bounds(player_pos, anchor, instance);
        if d > spec.interact_radius {
            continue;
        }

        match best {
            Some((_, _, best_d)) if d >= best_d => {}
            _ => best = Some((entity, spec_idx, d)),
        }
    }

    let Some((_entity, spec_idx, _)) = best else {
        return;
    };
    let spec = ENTITY_DIALOGUE_SPECS[spec_idx];
    let start_node = spec.start;
    let Some(node) = spec.nodes.get(start_node).copied() else {
        return;
    };

    set_dialogue_text(ui.as_mut(), node_text(node));
    lock.active = true;
    active.session = Some(DialogueSession {
        spec_idx,
        node_idx: start_node,
    });
}

fn entity_half_extents(instance: &EntityInstance) -> Vec2 {
    Vec2::new(
        (instance.width.max(1) as f32) * 0.5,
        (instance.height.max(1) as f32) * 0.5,
    )
}

fn distance_to_entity_interact_bounds(point: Vec2, center: Vec2, instance: &EntityInstance) -> f32 {
    let half_extents = entity_half_extents(instance);
    let dx = (point.x - center.x).abs() - half_extents.x;
    let dy = (point.y - center.y).abs() - half_extents.y;
    Vec2::new(dx.max(0.0), dy.max(0.0)).length()
}
