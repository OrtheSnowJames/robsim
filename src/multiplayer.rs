use better_bevy_menus::menu;
use bevy::prelude::*;
use bevy_networker_multiplayer::{NetResource, ReplicatedPlugin, netmsg};
use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;

use crate::bank::render::maze::MazeRenderState;
use crate::map::{
    LoadedMap, MainCamera, MultiplayerMazeTransitionBroadcast, MultiplayerMazeTransitionRequest,
    SceneChangeRequest, TOWN_MAP_ASSET_PATH, loaded_map_path,
};
use crate::player::{
    LocalPlayer, PLAYER_Z_LAYER, Player, PlayerIdentity, RemotePlayer, spawn_player_entity,
};
use crate::receipts;

const DEFAULT_ADDRESS: &str = "127.0.0.1:5001";
const DEFAULT_PORT: u16 = 5001;
const HOST_PLAYER_ID: u64 = 1;
const JAIL_MAP_ASSET_PATH: &str = "maps/jail.ldtk";
const MAZE_SCENE_KEY: &str = "maze";
const PLAYER_UPDATE_INTERVAL_SECONDS: f32 = 0.1;
const ROSTER_HEARTBEAT_SECONDS: f32 = 1.0;
const HOST_DISCONNECT_TIMEOUT_SECONDS: f32 = 4.0;
const VAULT_TRANSITION_BURST_COUNT: u8 = 12;
const VAULT_TRANSITION_BURST_INTERVAL_SECONDS: f32 = 0.12;

#[derive(Resource)]
#[menu]
pub struct NetworkMenuSettings {
    #[start_section(name = "Connection")]
    pub name: String,
    pub address: String,
}

impl Default for NetworkMenuSettings {
    fn default() -> Self {
        let mut random_name = receipts::random_name();
        random_name = random_name.split(' ').next().unwrap().to_string();
        Self {
            name: random_name,
            address: DEFAULT_ADDRESS.to_string(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PlayerSummary {
    pub id: u64,
    pub name: String,
    pub alive: bool,
    pub is_host: bool,
    pub scene: String,
    pub x: f32,
    pub y: f32,
}

#[derive(Resource, Default)]
pub struct MultiplayerRoster {
    pub players: Vec<PlayerSummary>,
}

impl MultiplayerRoster {
    fn needs_upsert(&self, summary: &PlayerSummary) -> bool {
        self.players
            .iter()
            .find(|player| player.id == summary.id)
            .map(|existing| existing != summary)
            .unwrap_or(true)
    }

    fn upsert(&mut self, summary: PlayerSummary) -> bool {
        if let Some(existing) = self
            .players
            .iter_mut()
            .find(|player| player.id == summary.id)
        {
            if *existing != summary {
                *existing = summary;
                return true;
            }
        } else {
            self.players.push(summary);
            self.players
                .sort_by_key(|player| (!player.is_host, player.id));
            return true;
        }

        false
    }

    fn replace(&mut self, players: Vec<PlayerSummary>) -> bool {
        if self.players == players {
            return false;
        }

        self.players = players;
        self.players
            .sort_by_key(|player| (!player.is_host, player.id));
        true
    }

    pub fn players(&self) -> &[PlayerSummary] {
        &self.players
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum MultiplayerMode {
    #[default]
    Menu,
    SinglePlayer,
    ClientConnecting,
    Client,
    Host,
}

#[derive(Resource, Debug)]
pub struct MultiplayerSession {
    mode: MultiplayerMode,
    pub local_player_id: u64,
    pub local_nonce: String,
    pub local_name: String,
    pub status: String,
}

impl Default for MultiplayerSession {
    fn default() -> Self {
        Self {
            mode: MultiplayerMode::Menu,
            local_player_id: 0,
            local_nonce: String::new(),
            local_name: "Player".to_string(),
            status: "Set name and address.".to_string(),
        }
    }
}

impl MultiplayerSession {
    pub fn is_game_started(&self) -> bool {
        matches!(
            self.mode,
            MultiplayerMode::SinglePlayer | MultiplayerMode::Client | MultiplayerMode::Host
        )
    }

    pub fn is_connected(&self) -> bool {
        matches!(self.mode, MultiplayerMode::Client | MultiplayerMode::Host)
    }

    pub fn local_is_host(&self) -> bool {
        self.mode == MultiplayerMode::Host
    }

    fn is_single_player(&self) -> bool {
        self.mode == MultiplayerMode::SinglePlayer
    }
}

#[derive(Resource)]
struct ServerClientIndex {
    next_player_id: u64,
    nonce_to_id: HashMap<String, u64>,
}

impl Default for ServerClientIndex {
    fn default() -> Self {
        Self {
            next_player_id: HOST_PLAYER_ID + 1,
            nonce_to_id: HashMap::new(),
        }
    }
}

#[derive(Resource, Default)]
struct ConnectionStartLock {
    released: bool,
}

#[derive(Resource, Default)]
struct ClientHostContact {
    seconds_since_contact: f32,
}

#[derive(Resource, Default)]
struct MenuSceneReset {
    pending: bool,
}

#[derive(Resource)]
struct PendingVaultTransitionBroadcast {
    message: Option<VaultMazeTransition>,
    sends_remaining: u8,
    seconds_until_next_send: f32,
}

impl Default for PendingVaultTransitionBroadcast {
    fn default() -> Self {
        Self {
            message: None,
            sends_remaining: 0,
            seconds_until_next_send: 0.0,
        }
    }
}

#[netmsg]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct ClientHello {
    nonce: String,
    name: String,
}

#[netmsg]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct ServerWelcome {
    nonce: String,
    player_id: u64,
}

#[netmsg]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct ClientPlayerUpdate {
    player_id: u64,
    name: String,
    alive: bool,
    scene: String,
    x: f32,
    y: f32,
}

#[netmsg]
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
struct RosterSnapshot {
    players: Vec<PlayerSummary>,
    vault_transition: Option<VaultMazeTransition>,
}

#[netmsg]
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
struct VaultMazeTransition {
    seed: u64,
    center_x: f32,
    center_y: f32,
}

#[derive(Component)]
struct ConnectionActionsRoot;

#[derive(Component)]
struct ConnectionStatusText;

#[derive(Clone, Copy)]
enum ConnectionAction {
    SinglePlayer,
    Host,
    Join,
}

#[derive(Component)]
struct ConnectionActionButton {
    action: ConnectionAction,
}

pub struct MultiplayerPlugin;

impl Plugin for MultiplayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((ReplicatedPlugin, NetworkMenuSettingsMenuPlugin))
            .init_resource::<NetworkMenuSettings>()
            .init_resource::<MultiplayerSession>()
            .init_resource::<MultiplayerRoster>()
            .init_resource::<ServerClientIndex>()
            .init_resource::<ConnectionStartLock>()
            .init_resource::<ClientHostContact>()
            .init_resource::<MenuSceneReset>()
            .init_resource::<PendingVaultTransitionBroadcast>()
            .add_systems(Startup, spawn_connection_actions)
            .add_systems(PostStartup, cleanup_extra_menu_cameras)
            .add_systems(
                Update,
                (
                    handle_connection_buttons,
                    update_connection_actions,
                    send_client_hello,
                    server_handle_client_hello,
                    client_handle_server_welcome,
                    publish_local_player_state,
                    server_handle_client_updates,
                    server_broadcast_roster_snapshots,
                    client_receive_roster_snapshots,
                    apply_roster_to_remote_players,
                    style_player_roles,
                    lock_game_until_connected,
                    broadcast_vault_maze_transitions,
                    receive_vault_maze_transitions,
                ),
            )
            .add_systems(
                Update,
                client_handle_host_timeout
                    .after(send_client_hello)
                    .after(publish_local_player_state)
                    .after(server_handle_client_hello)
                    .after(client_handle_server_welcome)
                    .after(server_handle_client_updates)
                    .after(server_broadcast_roster_snapshots)
                    .after(client_receive_roster_snapshots)
                    .after(broadcast_vault_maze_transitions)
                    .after(receive_vault_maze_transitions),
            )
            .add_systems(
                Update,
                retry_menu_scene_reset
                    .after(client_handle_host_timeout)
                    .before(crate::map::handle_scene_change_request),
            )
            .add_systems(PostUpdate, suppress_generated_menu_when_connected);
    }
}

fn spawn_connection_actions(mut commands: Commands) {
    commands
        .spawn((
            ConnectionActionsRoot,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                bottom: Val::Px(20.0),
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: Val::Px(10.0),
                ..default()
            },
            ZIndex(2000),
        ))
        .with_children(|root| {
            root.spawn((Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(12.0),
                ..default()
            },))
                .with_children(|buttons| {
                    spawn_connection_button(
                        buttons,
                        "Single Player",
                        ConnectionAction::SinglePlayer,
                    );
                    spawn_connection_button(buttons, "Host", ConnectionAction::Host);
                    spawn_connection_button(buttons, "Join", ConnectionAction::Join);
                });

            root.spawn((
                ConnectionStatusText,
                Text::new("Set name and address."),
                TextFont {
                    font_size: 18.0,
                    ..default()
                },
                TextColor(Color::WHITE),
            ));
        });
}

fn spawn_connection_button(
    parent: &mut ChildSpawnerCommands,
    label: &str,
    action: ConnectionAction,
) {
    parent
        .spawn((
            Button,
            Node {
                width: Val::Px(150.0),
                height: Val::Px(42.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(Color::srgb(0.16, 0.24, 0.29)),
            BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.28)),
            ConnectionActionButton { action },
        ))
        .with_children(|button| {
            button.spawn((
                Text::new(label),
                TextFont {
                    font_size: 18.0,
                    ..default()
                },
                TextColor(Color::WHITE),
            ));
        });
}

fn cleanup_extra_menu_cameras(
    mut commands: Commands,
    cameras: Query<(Entity, Option<&MainCamera>), With<Camera2d>>,
) {
    let has_main = cameras.iter().any(|(_, main)| main.is_some());
    if !has_main {
        return;
    }

    for (entity, main) in &cameras {
        if main.is_none() {
            commands.entity(entity).try_despawn();
        }
    }
}

fn handle_connection_buttons(
    mut commands: Commands,
    mut interactions: Query<
        (&Interaction, &ConnectionActionButton),
        (Changed<Interaction>, With<Button>),
    >,
    settings: Res<NetworkMenuSettings>,
    mut session: ResMut<MultiplayerSession>,
    mut roster: ResMut<MultiplayerRoster>,
    mut net: ResMut<NetResource>,
    mut host_contact: ResMut<ClientHostContact>,
    mut menu_scene_reset: ResMut<MenuSceneReset>,
    local_player: Query<Entity, (With<Player>, With<LocalPlayer>)>,
) {
    if session.is_game_started() || session.mode == MultiplayerMode::ClientConnecting {
        return;
    }

    for (interaction, button) in &mut interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }

        match button.action {
            ConnectionAction::SinglePlayer => {
                let name = sanitized_name(&settings.name);
                session.mode = MultiplayerMode::SinglePlayer;
                session.local_player_id = 0;
                session.local_nonce.clear();
                session.local_name = name.clone();
                session.status = "Single player.".to_string();
                host_contact.seconds_since_contact = 0.0;
                menu_scene_reset.pending = false;
                if let Ok(entity) = local_player.single() {
                    commands.entity(entity).insert(PlayerIdentity {
                        id: 0,
                        is_host: false,
                    });
                }
                let summary = PlayerSummary {
                    id: 0,
                    name,
                    alive: true,
                    is_host: false,
                    scene: TOWN_MAP_ASSET_PATH.to_string(),
                    x: 0.0,
                    y: -80.0,
                };
                if roster.needs_upsert(&summary) {
                    roster.upsert(summary);
                }
            }
            ConnectionAction::Host => {
                let name = sanitized_name(&settings.name);
                let port = parse_port(&settings.address);
                net.start_server(port);
                session.mode = MultiplayerMode::Host;
                session.local_player_id = HOST_PLAYER_ID;
                session.local_nonce = "host".to_string();
                session.local_name = name.clone();
                session.status = format!("Hosting on port {port}.");
                host_contact.seconds_since_contact = 0.0;
                menu_scene_reset.pending = false;
                if let Ok(entity) = local_player.single() {
                    commands.entity(entity).insert(PlayerIdentity {
                        id: HOST_PLAYER_ID,
                        is_host: true,
                    });
                }
                let summary = PlayerSummary {
                    id: HOST_PLAYER_ID,
                    name,
                    alive: true,
                    is_host: true,
                    scene: TOWN_MAP_ASSET_PATH.to_string(),
                    x: 0.0,
                    y: -80.0,
                };
                if roster.needs_upsert(&summary) {
                    roster.upsert(summary);
                }
            }
            ConnectionAction::Join => {
                let address = normalized_join_address(&settings.address);
                if address.parse::<SocketAddr>().is_err() {
                    session.status = format!("Invalid address: {address}");
                    continue;
                }
                let name = sanitized_name(&settings.name);
                let nonce = format!(
                    "{}-{}",
                    name,
                    crate::random::random_range_usize(1..1_000_000_000)
                );
                net.join_server(address.clone());
                session.mode = MultiplayerMode::ClientConnecting;
                session.local_player_id = 0;
                session.local_nonce = nonce;
                session.local_name = name;
                session.status = format!("Joining {address}.");
                host_contact.seconds_since_contact = 0.0;
                menu_scene_reset.pending = false;
            }
        }
    }
}

fn update_connection_actions(
    session: Res<MultiplayerSession>,
    mut root_query: Query<&mut Visibility, With<ConnectionActionsRoot>>,
    mut status_query: Query<&mut Text, With<ConnectionStatusText>>,
) {
    for mut visibility in &mut root_query {
        *visibility = if session.is_game_started() {
            Visibility::Hidden
        } else {
            Visibility::Visible
        };
    }

    for mut text in &mut status_query {
        text.0 = session.status.clone();
    }
}

fn suppress_generated_menu_when_connected(
    mut commands: Commands,
    session: Res<MultiplayerSession>,
    roots: Query<Entity, With<NetworkMenuSettingsMenuRootTag>>,
    mut dirty: ResMut<NetworkMenuSettingsMenuDirty>,
    mut focus: ResMut<NetworkMenuSettingsTextFocus>,
) {
    if !session.is_game_started() {
        return;
    }

    focus.0 = None;
    dirty.0 = false;
    for entity in &roots {
        commands.entity(entity).try_despawn();
    }
}

fn lock_game_until_connected(
    session: Res<MultiplayerSession>,
    mut start_lock: ResMut<ConnectionStartLock>,
    mut movement_lock: Option<ResMut<crate::entity_dialogue::PlayerMovementLock>>,
) {
    let Some(lock) = movement_lock.as_deref_mut() else {
        return;
    };

    if !session.is_game_started() {
        lock.active = true;
        start_lock.released = false;
        return;
    }

    if !start_lock.released {
        lock.active = false;
        start_lock.released = true;
    }
}

fn send_client_hello(
    time: Res<Time>,
    mut timer: Local<Option<Timer>>,
    session: Res<MultiplayerSession>,
    mut net: ResMut<NetResource>,
) {
    if session.mode != MultiplayerMode::ClientConnecting {
        return;
    }

    let timer = timer.get_or_insert_with(|| Timer::from_seconds(0.5, TimerMode::Repeating));
    timer.tick(time.delta());
    if !timer.just_finished() && timer.elapsed_secs() > 0.0 {
        return;
    }

    net.queue_message(ClientHello {
        nonce: session.local_nonce.clone(),
        name: session.local_name.clone(),
    });
}

fn server_handle_client_hello(
    mut net: ResMut<NetResource>,
    session: Res<MultiplayerSession>,
    mut index: ResMut<ServerClientIndex>,
    mut roster: ResMut<MultiplayerRoster>,
) {
    if !session.local_is_host() {
        return;
    }

    for hello in net.drain_messages::<ClientHello>() {
        let player_id = match index.nonce_to_id.get(&hello.nonce).copied() {
            Some(player_id) => player_id,
            None => {
                let player_id = index.next_player_id;
                index.next_player_id = index.next_player_id.saturating_add(1);
                index.nonce_to_id.insert(hello.nonce.clone(), player_id);
                player_id
            }
        };

        let summary = PlayerSummary {
            id: player_id,
            name: sanitized_name(&hello.name),
            alive: true,
            is_host: false,
            scene: TOWN_MAP_ASSET_PATH.to_string(),
            x: 0.0,
            y: -80.0,
        };
        if roster.needs_upsert(&summary) {
            roster.upsert(summary);
        }

        net.queue_message(ServerWelcome {
            nonce: hello.nonce,
            player_id,
        });
    }
}

fn client_handle_server_welcome(
    mut commands: Commands,
    mut net: ResMut<NetResource>,
    mut session: ResMut<MultiplayerSession>,
    mut host_contact: ResMut<ClientHostContact>,
    local_player: Query<Entity, (With<Player>, With<LocalPlayer>)>,
) {
    if session.mode != MultiplayerMode::ClientConnecting {
        return;
    }

    for welcome in net.drain_messages::<ServerWelcome>() {
        if welcome.nonce != session.local_nonce {
            continue;
        }

        session.mode = MultiplayerMode::Client;
        session.local_player_id = welcome.player_id;
        session.status = format!("Connected as {}.", session.local_name);
        host_contact.seconds_since_contact = 0.0;
        if let Ok(entity) = local_player.single() {
            commands.entity(entity).insert(PlayerIdentity {
                id: welcome.player_id,
                is_host: false,
            });
        }
    }
}

fn publish_local_player_state(
    time: Res<Time>,
    mut timer: Local<Option<Timer>>,
    session: Res<MultiplayerSession>,
    loaded_map: Res<LoadedMap>,
    player_query: Query<&Transform, (With<Player>, With<LocalPlayer>)>,
    mut roster: ResMut<MultiplayerRoster>,
    mut net: ResMut<NetResource>,
) {
    if !session.is_game_started() {
        return;
    }

    let timer = timer.get_or_insert_with(|| {
        Timer::from_seconds(PLAYER_UPDATE_INTERVAL_SECONDS, TimerMode::Repeating)
    });
    timer.tick(time.delta());
    if !timer.just_finished() {
        return;
    }

    let Ok(player_transform) = player_query.single() else {
        return;
    };
    let scene = loaded_map_path(&loaded_map).to_string();
    let alive = scene != JAIL_MAP_ASSET_PATH;
    let update = ClientPlayerUpdate {
        player_id: session.local_player_id,
        name: session.local_name.clone(),
        alive,
        scene,
        x: player_transform.translation.x,
        y: player_transform.translation.y,
    };

    if session.local_is_host() || session.is_single_player() {
        let summary = PlayerSummary {
            id: session.local_player_id,
            name: update.name,
            alive: update.alive,
            is_host: session.local_is_host(),
            scene: update.scene,
            x: update.x,
            y: update.y,
        };
        if roster.needs_upsert(&summary) {
            roster.upsert(summary);
        }
    } else if update.player_id != 0 {
        net.queue_message(update);
    }
}

fn server_handle_client_updates(
    mut net: ResMut<NetResource>,
    session: Res<MultiplayerSession>,
    mut roster: ResMut<MultiplayerRoster>,
) {
    if !session.local_is_host() {
        return;
    }

    for update in net.drain_messages::<ClientPlayerUpdate>() {
        if update.player_id == 0 || update.player_id == HOST_PLAYER_ID {
            continue;
        }

        let summary = PlayerSummary {
            id: update.player_id,
            name: sanitized_name(&update.name),
            alive: update.alive,
            is_host: false,
            scene: update.scene,
            x: update.x,
            y: update.y,
        };
        if roster.needs_upsert(&summary) {
            roster.upsert(summary);
        }
    }
}

fn server_broadcast_roster_snapshots(
    time: Res<Time>,
    mut heartbeat: Local<Option<Timer>>,
    mut last_sent: Local<Option<RosterSnapshot>>,
    session: Res<MultiplayerSession>,
    roster: Res<MultiplayerRoster>,
    loaded_map: Res<LoadedMap>,
    maze_state: Option<Res<MazeRenderState>>,
    mut net: ResMut<NetResource>,
) {
    if !session.local_is_host() {
        return;
    }

    let heartbeat = heartbeat
        .get_or_insert_with(|| Timer::from_seconds(ROSTER_HEARTBEAT_SECONDS, TimerMode::Repeating));
    heartbeat.tick(time.delta());
    let heartbeat_finished = heartbeat.just_finished();

    let vault_transition = if loaded_map_path(&loaded_map) == MAZE_SCENE_KEY {
        maze_state.as_ref().and_then(|maze| {
            maze.seed.map(|seed| VaultMazeTransition {
                seed,
                center_x: maze.world_center.x,
                center_y: maze.world_center.y,
            })
        })
    } else {
        None
    };
    let snapshot = RosterSnapshot {
        players: roster.players.clone(),
        vault_transition,
    };
    let changed = last_sent
        .as_ref()
        .map(|previous| previous != &snapshot)
        .unwrap_or(true);

    if !changed && !heartbeat_finished {
        return;
    }

    net.queue_message(snapshot.clone());
    *last_sent = Some(snapshot);
}

fn client_receive_roster_snapshots(
    session: Res<MultiplayerSession>,
    loaded_map: Res<LoadedMap>,
    mut net: ResMut<NetResource>,
    mut roster: ResMut<MultiplayerRoster>,
    mut host_contact: ResMut<ClientHostContact>,
    mut maze_requests: MessageWriter<MultiplayerMazeTransitionRequest>,
) {
    if session.local_is_host() || !session.is_connected() {
        return;
    }

    for snapshot in net.drain_messages::<RosterSnapshot>() {
        host_contact.seconds_since_contact = 0.0;
        if let Some(transition) = snapshot.vault_transition {
            if loaded_map_path(&loaded_map) != MAZE_SCENE_KEY {
                maze_requests.write(MultiplayerMazeTransitionRequest {
                    center: Vec2::new(transition.center_x, transition.center_y),
                    seed: transition.seed,
                });
            }
        }
        if roster.players != snapshot.players {
            roster.replace(snapshot.players);
        }
    }
}

fn apply_roster_to_remote_players(
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut texture_atlas_layouts: ResMut<Assets<TextureAtlasLayout>>,
    session: Res<MultiplayerSession>,
    roster: Res<MultiplayerRoster>,
    loaded_map: Res<LoadedMap>,
    mut remote_players: Query<
        (
            Entity,
            &mut PlayerIdentity,
            &mut Transform,
            &mut Visibility,
            &mut Sprite,
        ),
        With<RemotePlayer>,
    >,
) {
    if !session.is_connected() {
        return;
    }
    if !roster.is_changed() && !loaded_map.is_changed() {
        return;
    }

    let roster_ids = roster
        .players
        .iter()
        .filter(|player| player.id != session.local_player_id)
        .map(|player| player.id)
        .collect::<HashSet<_>>();
    let existing = remote_players
        .iter_mut()
        .map(|(entity, identity, _, _, _)| (identity.id, entity))
        .collect::<HashMap<_, _>>();

    for (id, entity) in &existing {
        if !roster_ids.contains(id) {
            commands.entity(*entity).try_despawn();
        }
    }

    let local_scene = loaded_map_path(&loaded_map);
    for player in roster
        .players
        .iter()
        .filter(|player| player.id != session.local_player_id)
    {
        let visible = player.scene == local_scene;
        if let Some(entity) = existing.get(&player.id).copied() {
            if let Ok((_, mut identity, mut transform, mut visibility, mut sprite)) =
                remote_players.get_mut(entity)
            {
                identity.is_host = player.is_host;
                transform.translation.x = player.x;
                transform.translation.y = player.y;
                transform.translation.z = PLAYER_Z_LAYER;
                *visibility = if visible {
                    Visibility::Visible
                } else {
                    Visibility::Hidden
                };
                sprite.color = player_color(player.is_host);
            }
        } else {
            let entity = spawn_player_entity(
                &mut commands,
                assets.as_ref(),
                texture_atlas_layouts.as_mut(),
                Vec3::new(player.x, player.y, PLAYER_Z_LAYER),
                Some((player.id, player.is_host)),
                false,
            );
            commands.entity(entity).insert(if visible {
                Visibility::Visible
            } else {
                Visibility::Hidden
            });
        }
    }
}

fn style_player_roles(
    mut players: Query<(&PlayerIdentity, &mut Sprite), (With<Player>, Changed<PlayerIdentity>)>,
) {
    for (identity, mut sprite) in &mut players {
        sprite.color = player_color(identity.is_host);
    }
}

fn broadcast_vault_maze_transitions(
    time: Res<Time>,
    session: Res<MultiplayerSession>,
    mut net: ResMut<NetResource>,
    mut transitions: MessageReader<MultiplayerMazeTransitionBroadcast>,
    mut pending: ResMut<PendingVaultTransitionBroadcast>,
) {
    if !session.local_is_host() {
        pending.message = None;
        pending.sends_remaining = 0;
        return;
    }

    for transition in transitions.read() {
        pending.message = Some(VaultMazeTransition {
            seed: transition.seed,
            center_x: transition.center.x,
            center_y: transition.center.y,
        });
        pending.sends_remaining = VAULT_TRANSITION_BURST_COUNT;
        pending.seconds_until_next_send = 0.0;
    }

    let Some(message) = pending.message.clone() else {
        return;
    };

    pending.seconds_until_next_send -= time.delta_secs();
    if pending.seconds_until_next_send > 0.0 {
        return;
    }

    net.queue_message(message);
    pending.sends_remaining = pending.sends_remaining.saturating_sub(1);
    if pending.sends_remaining == 0 {
        pending.message = None;
        pending.seconds_until_next_send = 0.0;
    } else {
        pending.seconds_until_next_send = VAULT_TRANSITION_BURST_INTERVAL_SECONDS;
    }
}

fn receive_vault_maze_transitions(
    session: Res<MultiplayerSession>,
    mut net: ResMut<NetResource>,
    mut requests: MessageWriter<MultiplayerMazeTransitionRequest>,
    mut host_contact: ResMut<ClientHostContact>,
) {
    if session.local_is_host() {
        return;
    }

    for transition in net.drain_messages::<VaultMazeTransition>() {
        host_contact.seconds_since_contact = 0.0;
        requests.write(MultiplayerMazeTransitionRequest {
            center: Vec2::new(transition.center_x, transition.center_y),
            seed: transition.seed,
        });
    }
}

fn client_handle_host_timeout(
    time: Res<Time>,
    mut commands: Commands,
    mut session: ResMut<MultiplayerSession>,
    mut roster: ResMut<MultiplayerRoster>,
    mut net: ResMut<NetResource>,
    mut host_contact: ResMut<ClientHostContact>,
    mut menu_scene_reset: ResMut<MenuSceneReset>,
    loaded_map: Res<LoadedMap>,
    remote_players: Query<Entity, With<RemotePlayer>>,
    local_player: Query<Entity, (With<Player>, With<LocalPlayer>)>,
    mut dirty: ResMut<NetworkMenuSettingsMenuDirty>,
    mut focus: ResMut<NetworkMenuSettingsTextFocus>,
) {
    if !matches!(
        session.mode,
        MultiplayerMode::Client | MultiplayerMode::ClientConnecting
    ) {
        host_contact.seconds_since_contact = 0.0;
        return;
    }

    host_contact.seconds_since_contact += time.delta_secs();
    if host_contact.seconds_since_contact < HOST_DISCONNECT_TIMEOUT_SECONDS {
        return;
    }

    let status = if session.mode == MultiplayerMode::ClientConnecting {
        "Could not connect to host.".to_string()
    } else {
        "Host disconnected.".to_string()
    };

    session.mode = MultiplayerMode::Menu;
    session.local_player_id = 0;
    session.local_nonce.clear();
    session.status = status;
    roster.players.clear();
    host_contact.seconds_since_contact = 0.0;
    *net = NetResource::default();

    for entity in &remote_players {
        commands.entity(entity).try_despawn();
    }

    if let Ok(entity) = local_player.single() {
        commands.entity(entity).insert(PlayerIdentity {
            id: 0,
            is_host: false,
        });
    }

    dirty.0 = true;
    focus.0 = None;
    menu_scene_reset.pending = loaded_map_path(&loaded_map) != TOWN_MAP_ASSET_PATH;
}

fn retry_menu_scene_reset(
    loaded_map: Res<LoadedMap>,
    mut menu_scene_reset: ResMut<MenuSceneReset>,
    mut scene_changes: MessageWriter<SceneChangeRequest>,
) {
    if !menu_scene_reset.pending {
        return;
    }

    if loaded_map_path(&loaded_map) == TOWN_MAP_ASSET_PATH {
        menu_scene_reset.pending = false;
        return;
    }

    scene_changes.write(SceneChangeRequest {
        asset_path: TOWN_MAP_ASSET_PATH.to_string(),
    });
}

fn player_color(is_host: bool) -> Color {
    if is_host {
        Color::srgb(1.0, 0.76, 0.18)
    } else {
        Color::WHITE
    }
}

fn sanitized_name(name: &str) -> String {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        "Player".to_string()
    } else {
        trimmed.chars().take(24).collect()
    }
}

fn normalized_join_address(address: &str) -> String {
    let trimmed = address.trim();
    if trimmed.is_empty() {
        return DEFAULT_ADDRESS.to_string();
    }
    if trimmed.parse::<SocketAddr>().is_ok() {
        return trimmed.to_string();
    }
    format!("{trimmed}:{DEFAULT_PORT}")
}

fn parse_port(address: &str) -> u16 {
    address
        .trim()
        .rsplit_once(':')
        .and_then(|(_, port)| port.parse::<u16>().ok())
        .unwrap_or(DEFAULT_PORT)
}
