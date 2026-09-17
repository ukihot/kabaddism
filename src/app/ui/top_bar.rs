//! 上部バー: カレンダー + 国家ステータス（FR-UI-01）
//!
//! セルは起動時に一度だけ並べ、以降は値の `Text` だけ書き換える（§13.3）。

use bevy::prelude::*;

use super::super::GameRes;
use super::{INK, INK_DIM, TopBar, font, label};

/// 何番目のセルの値かを覚えておくだけの目印。
#[derive(Component)]
pub struct StatCell(usize);

const KEYS: [&str; 7] = [
    "ui.stat.date",
    "ui.stat.cup",
    "ui.stat.population",
    "ui.stat.gdp",
    "ui.stat.budget",
    "ui.stat.ability",
    "ui.stat.squad",
];

pub fn spawn(mut commands: Commands, bar: Single<Entity, With<TopBar>>, game: Res<GameRes>) {
    let labels: Vec<String> = KEYS.iter().map(|k| game.defs.text.get(k).to_string()).collect();
    commands.entity(*bar).with_children(|p| {
        for (i, name) in labels.into_iter().enumerate() {
            p.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(2), ..default() })
                .with_children(|c| {
                    c.spawn(label(name, 11.0, INK_DIM));
                    c.spawn((StatCell(i), Text::new("-"), font(17.0), TextColor(INK)));
                });
        }
    });
}

pub fn sync(game: Res<GameRes>, mut cells: Query<(&StatCell, &mut Text)>) {
    if !game.is_changed() {
        return;
    }
    let values = values(&game);
    for (cell, mut text) in &mut cells {
        if let Some(v) = values.get(cell.0)
            && text.as_str() != v
        {
            **text = v.clone();
        }
    }
}

fn values(game: &GameRes) -> Vec<String> {
    let t = game.stats.today();
    let days = |n: u16| game.defs.text.format("ui.stat.days", &[("days", &n.to_string())]);
    vec![
        game.date.to_string(),
        days(t.map(|t| t.days_to_cup).unwrap_or(0)),
        t.map(|t| t.population.to_string()).unwrap_or_default(),
        t.map(|t| format!("{:.0}K", t.gdp)).unwrap_or_default(),
        t.map(|t| format!("{:.0}K", t.budget_available)).unwrap_or_default(),
        t.map(|t| format!("{:.1}", t.mean_ability)).unwrap_or_default(),
        t.map(|t| format!("{:.1}", t.squad_strength)).unwrap_or_default(),
    ]
}
