//! タイトル画面（design.md §3.2）

use bevy::prelude::*;

use super::GameRes;
use super::state::GameState;
use super::ui::{ACCENT, BG, INK, INK_DIM, button, label};

#[derive(Component)]
pub struct Start;

pub fn spawn(mut commands: Commands, game: Res<GameRes>) {
    let nation = game.defs.scenario.nation_name.clone();
    let start = game.defs.text.get("ui.title.start").to_string();
    let seed = game
        .defs
        .text
        .format("ui.title.seed", &[("seed", &game.rng.seed.to_string())]);

    commands.spawn((
        DespawnOnExit(GameState::Title),
        Node {
            width: percent(100),
            height: percent(100),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            row_gap: px(16),
            ..default()
        },
        BackgroundColor(BG),
    ))
    .with_children(|p| {
        p.spawn(label(nation, 34.0, INK));
        p.spawn(label(seed, 13.0, INK_DIM));
        p.spawn(button(Start, true)).with_children(|c| {
            c.spawn(label(start, 18.0, ACCENT));
        });
    });
}

pub fn click(
    mut next: ResMut<NextState<GameState>>,
    q: Query<&Interaction, (Changed<Interaction>, With<Start>)>,
) {
    if q.iter().any(|i| *i == Interaction::Pressed) {
        next.set(GameState::InGame);
    }
}
