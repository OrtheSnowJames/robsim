use crate::PlayerMoney;
use crate::multiplayer::MultiplayerRoster;
use bevy::prelude::*;

#[derive(Component)]
struct TopNode;

#[derive(Component)]
struct MoneyText;

#[derive(Component)]
struct PlayerListText;

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_hud);
        app.add_systems(Update, (change_money, change_player_list));
    }
}

fn setup_hud(mut commands: Commands, player_money: Option<Res<PlayerMoney>>) {
    let player_money = player_money.as_deref();
    let amount = player_money.map_or(0, |m| m.amount);

    commands
        .spawn((
            TopNode,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::FlexStart,
                align_items: AlignItems::FlexStart,
                row_gap: Val::Px(8.0),
                padding: UiRect::all(Val::Px(16.0)),
                ..default()
            },
        ))
        .with_children(|parent| {
            parent
                .spawn((
                    Node {
                        padding: UiRect::axes(Val::Px(12.0), Val::Px(8.0)),
                        border: UiRect::all(Val::Px(2.0)),
                        border_radius: BorderRadius::all(Val::Px(8.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.05, 0.06, 0.08, 0.88)),
                    BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.25)),
                ))
                .with_children(|panel| {
                    panel.spawn((
                        MoneyText,
                        Text::new(format!("Money: {}", amount)),
                        TextFont {
                            font_size: 28.0,
                            ..default()
                        },
                        TextColor(Color::srgba(0.98, 0.98, 0.96, 1.0)),
                    ));
                });

            parent
                .spawn((
                    Node {
                        width: Val::Px(220.0),
                        padding: UiRect::axes(Val::Px(12.0), Val::Px(10.0)),
                        border: UiRect::all(Val::Px(2.0)),
                        border_radius: BorderRadius::all(Val::Px(8.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.05, 0.06, 0.08, 0.88)),
                    BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.25)),
                ))
                .with_children(|panel| {
                    panel.spawn((
                        PlayerListText,
                        Text::new("Players\nWaiting"),
                        TextFont {
                            font_size: 17.0,
                            ..default()
                        },
                        TextColor(Color::srgba(0.98, 0.98, 0.96, 1.0)),
                    ));
                });
        });
}

fn change_money(
    player_money: Option<Res<PlayerMoney>>,
    mut text_q: Query<&mut Text, With<MoneyText>>,
) {
    let Ok(mut text) = text_q.single_mut() else {
        return;
    };
    let amount = player_money.as_deref().map_or(0, |m| m.amount);
    let new_text = format!("Money: {}", amount);
    text.0 = new_text;
}

fn change_player_list(
    roster: Option<Res<MultiplayerRoster>>,
    mut text_q: Query<&mut Text, With<PlayerListText>>,
) {
    if roster
        .as_ref()
        .map(|roster| !roster.is_changed())
        .unwrap_or(false)
    {
        return;
    }

    let Ok(mut text) = text_q.single_mut() else {
        return;
    };
    let Some(roster) = roster else {
        text.0 = "Players\nWaiting".to_string();
        return;
    };

    if roster.players().is_empty() {
        text.0 = "Players\nWaiting".to_string();
        return;
    }

    let mut lines = vec!["Players".to_string()];
    for player in roster.players() {
        let role = if player.is_host { "HOST" } else { "PLAYER" };
        let status = if player.alive { "Alive" } else { "Down" };
        lines.push(format!("{role} {} - {status}", player.name));
    }
    text.0 = lines.join("\n");
}
