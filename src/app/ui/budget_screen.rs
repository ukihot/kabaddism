//! 年次予算編成（FR-BUD-02）
//!
//! フェーズ7 では提示と確定だけ。分野ごとにスライダで配分を動かすのはフェーズ8。

use bevy::prelude::*;

use super::super::GameRes;
use super::super::state::Phase;
use super::{ACCENT, INK, INK_DIM, ModalRoot, button, label, modal, resume_phase};

#[derive(Component)]
pub struct Confirm;

pub fn spawn(mut commands: Commands, root: Single<Entity, With<ModalRoot>>, game: Res<GameRes>) {
    let brief = game.budget_briefing();
    let t = &game.defs.text;
    let heading = t.format("ui.budget.heading", &[("year", &brief.year.to_string())]);
    let summary = t.format(
        "ui.budget.summary",
        &[
            ("revenue", &format!("{:.0}", brief.revenue_estimate)),
            ("upkeep", &format!("{:.0}", brief.committed_upkeep)),
            ("pot", &format!("{:.0}", brief.allocatable())),
        ],
    );
    let confirm = t.get("ui.budget.confirm").to_string();

    commands.entity(*root).with_children(|p| {
        p.spawn((DespawnOnExit(Phase::Budget), modal())).with_children(|c| {
            c.spawn(label(heading, 20.0, INK));
            c.spawn(label(summary, 14.0, INK_DIM));
            c.spawn(button(Confirm, true)).with_children(|b| {
                b.spawn(label(confirm, 15.0, ACCENT));
            });
        });
    });
}

pub fn click(
    mut game: ResMut<GameRes>,
    mut next: ResMut<NextState<Phase>>,
    q: Query<&Interaction, (Changed<Interaction>, With<Confirm>)>,
) {
    if q.iter().any(|i| *i == Interaction::Pressed) {
        let plan = game.budget_briefing().default_plan();
        game.apply_budget(plan);
        next.set(resume_phase(&game));
    }
}
