//! 年次予算編成（FR-BUD-02 / §12.1）
//!
//! 分野ごとに±ボタンで配分を動かす。ドラッグ式のスライダは bevy_ui に無く、
//! design.md §3.1 の方針（自前実装は Node + Interaction のみ）に沿って
//! 段階的なステッパーで代用する。触らずに確定すれば `default_plan()` のままになる。

use bevy::prelude::*;

use kbism::sim::defs::BudgetField;

use super::super::GameRes;
use super::super::state::Phase;
use super::policy_panel::field_key;
use super::{ACCENT, INK, INK_DIM, ModalRoot, button, label, modal, resume_phase};

/// 編集中の配分案。触らなければ `default_plan()` のまま確定する。
/// 負の配分は作れない、という不変条件はこの型の中で守る。
#[derive(Resource, Default)]
pub struct BudgetDraft([f32; 8]);

impl BudgetDraft {
    pub fn reset(&mut self, plan: [f32; 8]) {
        self.0 = plan;
    }

    pub fn of(&self, field: BudgetField) -> f32 {
        self.0[field.index()]
    }

    pub fn adjust(&mut self, field: BudgetField, delta: f32) {
        let i = field.index();
        self.0[i] = (self.0[i] + delta).max(0.0);
    }

    pub fn allocations(&self) -> [f32; 8] {
        self.0
    }
}

#[derive(Component)]
pub struct Confirm;
#[derive(Component)]
pub struct Step {
    field: BudgetField,
    delta: f32,
}

/// スライダ本体。値が変わるたびに中身だけ差し替える。
#[derive(Component)]
pub struct BudgetBody;

pub fn spawn(
    mut commands: Commands,
    root: Single<Entity, With<ModalRoot>>,
    game: Res<GameRes>,
    mut draft: ResMut<BudgetDraft>,
) {
    let brief = game.budget_briefing();
    draft.reset(brief.default_plan());

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

    commands.entity(*root).with_children(|p| {
        p.spawn((DespawnOnExit(Phase::Budget), modal())).with_children(|c| {
            c.spawn(label(heading, 20.0, INK));
            c.spawn(label(summary, 14.0, INK_DIM));
            c.spawn((
                BudgetBody,
                Node { flex_direction: FlexDirection::Column, row_gap: px(4), ..default() },
            ));
        });
    });
}

/// `BudgetDraft` が変わった直後、または初回に呼ぶ。中身だけ差し替える。
pub fn sync(
    mut commands: Commands,
    body: Option<Single<Entity, With<BudgetBody>>>,
    game: Res<GameRes>,
    draft: Res<BudgetDraft>,
) {
    let Some(body) = body else { return };
    if !draft.is_changed() && !game.is_changed() {
        return;
    }
    let t = &game.defs.text;
    let step = (game.budget_briefing().allocatable() * 0.02).max(1.0);
    let confirm = t.get("ui.budget.confirm").to_string();

    commands.entity(*body).despawn_related::<Children>().with_children(|c| {
        for field in BudgetField::ALL {
            let name = t.get(field_key(field)).to_string();
            let value = format!("{:.0}K", draft.of(field));
            c.spawn(Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: px(8),
                ..default()
            })
            .with_children(|row| {
                row.spawn(label(name, 13.0, INK)).insert(Node { width: px(90), ..default() });
                row.spawn(button(Step { field, delta: -step }, draft.of(field) > 0.0))
                    .with_children(|b| {
                        b.spawn(label("−", 14.0, ACCENT));
                    });
                row.spawn(label(value, 13.0, INK_DIM));
                row.spawn(button(Step { field, delta: step }, true)).with_children(|b| {
                    b.spawn(label("＋", 14.0, ACCENT));
                });
            });
        }
        c.spawn(button(Confirm, true)).with_children(|b| {
            b.spawn(label(confirm, 15.0, ACCENT));
        });
    });
}

pub fn click_step(
    mut draft: ResMut<BudgetDraft>,
    q: Query<(&Interaction, &Step), Changed<Interaction>>,
) {
    for (interaction, step) in &q {
        if *interaction == Interaction::Pressed {
            draft.adjust(step.field, step.delta);
        }
    }
}

pub fn click_confirm(
    mut game: ResMut<GameRes>,
    draft: Res<BudgetDraft>,
    mut next: ResMut<NextState<Phase>>,
    q: Query<&Interaction, (Changed<Interaction>, With<Confirm>)>,
) {
    if q.iter().any(|i| *i == Interaction::Pressed) {
        game.apply_budget(draft.allocations());
        next.set(resume_phase(&game));
    }
}
