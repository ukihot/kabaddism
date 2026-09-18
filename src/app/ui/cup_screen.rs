//! 世界大会（FR-CUP-*）
//!
//! 提示 → 代表の差し替え → 実行 → 結果。差し替えの規則（人数を変えない・
//! 出場できない人物を選べない）は `sim::cup` が持ち、ここは提示と入力だけを扱う。

use bevy::prelude::*;

use kbism::sim::cup;
use kbism::sim::ids::PersonId;

use super::super::GameRes;
use super::super::state::Phase;
use super::{ACCENT, INK, INK_DIM, ModalRoot, button, label, modal, resume_phase};

/// 控えとして並べる人数。多すぎるとモーダルが伸びる。
const BENCH_SHOWN: usize = 8;

#[derive(Component)]
pub struct Run;
#[derive(Component)]
pub struct Close;

/// 押すと代表に入る控え選手（FR-CUP-02）。
#[derive(Component, Clone, Copy)]
pub struct SwapIn(pub PersonId);

/// 結果表示の入れ替え先。中身を差し替えるだけなので、モーダル自体は残す。
#[derive(Component)]
pub struct CupBody;

pub fn spawn(
    mut commands: Commands,
    root: Single<Entity, With<ModalRoot>>,
    mut game: ResMut<GameRes>,
) {
    game.cup_prepare();
    let heading = game.defs.text.get("ui.cup.heading").to_string();

    commands.entity(*root).with_children(|p| {
        p.spawn((DespawnOnExit(Phase::Cup), modal())).with_children(|c| {
            c.spawn(label(heading, 20.0, INK));
            c.spawn((
                CupBody,
                Node { flex_direction: FlexDirection::Column, row_gap: px(8), ..default() },
            ));
        });
    });
}

/// 提示内容と控えの一覧を組み直す。差し替えのたびにここを通る。
pub fn sync_preview(
    mut commands: Commands,
    body: Option<Single<Entity, With<CupBody>>>,
    game: Res<GameRes>,
    // モーダルは毎年作り直されるので、実体と登録内容の両方で変化を見る。
    mut last: Local<Option<(Entity, Vec<PersonId>)>>,
) {
    let Some(body) = body else { return };
    let Some(preview) = game.cup_preview.clone() else { return };
    if last.as_ref() == Some(&(*body, preview.squad.clone())) {
        return;
    }
    *last = Some((*body, preview.squad.clone()));

    let t = &game.defs.text;
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
    let squad_heading = t.get("ui.cup.squad").to_string();
    let bench_heading = t.get("ui.cup.bench").to_string();
    let run = t.get("ui.cup.run").to_string();

    let member = |id: PersonId| {
        let p = game.world.person(id);
        format!(
            "{}　能力{:.0}　調子{:.0}%",
            p.name,
            p.life.ability.value,
            p.life.condition.factor() * 100.0
        )
    };
    let squad: Vec<String> = preview.squad.iter().map(|id| member(*id)).collect();
    let bench: Vec<(PersonId, String)> = cup::ranked_candidates(&game)
        .into_iter()
        .filter(|id| !preview.squad.contains(id))
        .take(BENCH_SHOWN)
        .map(|id| (id, member(id)))
        .collect();

    commands.entity(*body).despawn_related::<Children>().with_children(|b| {
        b.spawn(label(line, 14.0, INK_DIM));

        b.spawn(label(squad_heading, 11.0, INK_DIM));
        for name in squad {
            b.spawn(label(name, 13.0, INK));
        }

        if !bench.is_empty() {
            b.spawn(label(bench_heading, 11.0, INK_DIM));
            for (id, name) in bench {
                b.spawn(button(SwapIn(id), true)).with_children(|x| {
                    x.spawn(label(name, 12.0, INK_DIM));
                });
            }
        }

        b.spawn(button(Run, true)).with_children(|x| {
            x.spawn(label(run, 15.0, ACCENT));
        });
    });
}

/// 控えを押したら代表に入れる。規則は `sim::cup::swap_in` が持つ。
pub fn click_swap(
    mut game: ResMut<GameRes>,
    q: Query<(&Interaction, &SwapIn), Changed<Interaction>>,
) {
    let Some((_, swap)) = q.iter().find(|(i, _)| **i == Interaction::Pressed) else {
        return;
    };
    let Some(preview) = game.cup_preview.clone() else { return };
    let next = cup::swap_in(&game, &preview.squad, swap.0);
    game.set_squad(next);
    game.cup_prepare();
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
