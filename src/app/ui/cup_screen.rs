//! 世界大会（FR-CUP-*）
//!
//! フェーズ7 では提示 → 実行 → 結果の3画面だけ。代表の差し替えはフェーズ8。

use bevy::prelude::*;

use super::super::GameRes;
use super::super::state::Phase;
use super::{ACCENT, INK, INK_DIM, ModalRoot, button, label, modal, resume_phase};

#[derive(Component)]
pub struct Run;
#[derive(Component)]
pub struct Close;

/// 結果表示の入れ替え先。中身を差し替えるだけなので、モーダル自体は残す。
#[derive(Component)]
pub struct CupBody;

pub fn spawn(
    mut commands: Commands,
    root: Single<Entity, With<ModalRoot>>,
    mut game: ResMut<GameRes>,
) {
    let preview = game.cup_prepare();
    let t = &game.defs.text;
    let heading = t.get("ui.cup.heading").to_string();
    let staked: Vec<String> =
        preview.staked.iter().map(|d| game.world.district(*d).name.clone()).collect();
    let line = t.format(
        "ui.cup.preview",
        &[
            ("squad", &preview.squad.len().to_string()),
            ("strength", &format!("{:.1}", preview.squad_strength)),
            ("staked", &staked.join("・")),
            ("loss", &format!("{:.0}", preview.max_loss)),
        ],
    );
    let run = t.get("ui.cup.run").to_string();

    commands.entity(*root).with_children(|p| {
        p.spawn((DespawnOnExit(Phase::Cup), modal())).with_children(|c| {
            c.spawn(label(heading, 20.0, INK));
            c.spawn((
                CupBody,
                Node { flex_direction: FlexDirection::Column, row_gap: px(8), ..default() },
            ))
            .with_children(|b| {
                b.spawn(label(line, 14.0, INK_DIM));
                b.spawn(button(Run, true)).with_children(|x| {
                    x.spawn(label(run, 15.0, ACCENT));
                });
            });
        });
    });
}

pub fn click_run(
    mut commands: Commands,
    mut game: ResMut<GameRes>,
    body: Single<Entity, With<CupBody>>,
    q: Query<&Interaction, (Changed<Interaction>, With<Run>)>,
) {
    if !q.iter().any(|i| *i == Interaction::Pressed) {
        return;
    }
    let result = game.cup_run();
    let t = &game.defs.text;
    let line = t.format(
        "ui.cup.result",
        &[
            ("rank", &(result.home_rank + 1).to_string()),
            ("gained", &result.gained.len().to_string()),
            ("lost", &result.lost.len().to_string()),
            ("realized", &format!("{:.0}", result.realized_loss)),
        ],
    );
    let close = t.get("ui.cup.close").to_string();

    commands.entity(*body).despawn_related::<Children>().with_children(|b| {
        b.spawn(label(line, 15.0, INK));
        b.spawn(button(Close, true)).with_children(|x| {
            x.spawn(label(close, 15.0, ACCENT));
        });
    });
}

pub fn click_close(
    game: Res<GameRes>,
    mut next: ResMut<NextState<Phase>>,
    q: Query<&Interaction, (Changed<Interaction>, With<Close>)>,
) {
    if q.iter().any(|i| *i == Interaction::Pressed) {
        next.set(resume_phase(&game));
    }
}
