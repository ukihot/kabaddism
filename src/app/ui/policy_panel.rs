//! 政策カード選択（FR-UI-01 / FR-UI-03 / FR-POL-05）
//!
//! `Planning` に入った時だけ組み立て、抜けるときに `DespawnOnExit` で消える。
//! 費用・期間・分野・対象・実行できない理由を、すべて出す（FR-UI-03）。

use bevy::prelude::*;

use kbism::sim::defs::BudgetField;
use kbism::sim::ids::Target;
use kbism::sim::policy::{self, Shortfall};

use super::super::GameRes;
use super::super::autosave::{self, AutosaveCounter};
use super::super::state::Phase;
use super::{ACCENT, INK, INK_DIM, PolicyPanel, WARN, button, label};

/// 推奨枠に出す最大件数。
const SHOWN: usize = 8;

#[derive(Component)]
pub struct PolicyChoice {
    id: String,
    target: Target,
}

/// 政策を打たずに1日だけ進める。詰まっても前に進めるようにしておく。
#[derive(Component)]
pub struct WaitChoice;

pub fn spawn(mut commands: Commands, panel: Single<Entity, With<PolicyPanel>>, game: Res<GameRes>) {
    let text = &game.defs.text;
    let heading = text.get("ui.policy.heading").to_string();
    let recommended = text.get("ui.policy.recommended").to_string();
    let wait = text.get("ui.policy.wait").to_string();
    let none = text.get("ui.policy.none").to_string();

    // 表示に必要なものを、借用を持ち越さない形で先に組み立てる。
    struct Row {
        id: String,
        target: Target,
        name: String,
        detail: String,
        place: String,
        reason: Option<String>,
    }
    let rows: Vec<Row> = game
        .recommended_policies(SHOWN)
        .into_iter()
        .filter_map(|(id, target, _issue)| {
            let def = game.defs.policy(&id)?;
            let miss = policy::check(&game, def, target);
            Some(Row {
                name: def.name.clone(),
                detail: text.format(
                    "ui.policy.cost",
                    &[
                        ("days", &def.days.to_string()),
                        ("cost", &format!("{:.0}", def.initial_cost)),
                        ("field", text.get(field_key(def.field))),
                    ],
                ),
                place: policy::target_name(&game, target),
                reason: miss.first().map(|s| describe(&game, s)),
                id,
                target,
            })
        })
        .collect();

    commands.entity(*panel).with_children(|parent| {
        parent
            .spawn((
                DespawnOnExit(Phase::Planning),
                Node { flex_direction: FlexDirection::Column, ..default() },
            ))
            .with_children(|p| {
                p.spawn(label(heading, 11.0, INK_DIM));
                p.spawn(label(recommended, 13.0, INK));
                if rows.is_empty() {
                    p.spawn(label(none, 12.0, INK_DIM));
                }
                for row in rows {
                    let ok = row.reason.is_none();
                    p.spawn(button(PolicyChoice { id: row.id, target: row.target }, ok))
                        .with_children(|c| {
                            c.spawn(label(row.name, 15.0, if ok { INK } else { INK_DIM }));
                            c.spawn(label(format!("{}　{}", row.place, row.detail), 11.0, INK_DIM));
                            if let Some(reason) = row.reason {
                                c.spawn(label(reason, 11.0, WARN));
                            }
                        });
                }
                p.spawn(button(WaitChoice, true)).with_children(|c| {
                    c.spawn(label(wait, 14.0, ACCENT));
                });
            });
    });
}

/// 押されたら時間が動き出す。`sim` を書き換える数少ない入口のひとつ。
pub fn click(
    mut game: ResMut<GameRes>,
    mut autosave_counter: ResMut<AutosaveCounter>,
    mut next: ResMut<NextState<Phase>>,
    policies: Query<(&Interaction, &PolicyChoice), Changed<Interaction>>,
    waits: Query<&Interaction, (Changed<Interaction>, With<WaitChoice>)>,
) {
    for (interaction, choice) in &policies {
        if *interaction != Interaction::Pressed {
            continue;
        }
        autosave::run(&game, &mut autosave_counter);
        if game.execute_policy(&choice.id, choice.target).is_ok() {
            next.set(Phase::Advancing);
            return;
        }
    }
    for interaction in &waits {
        if *interaction == Interaction::Pressed {
            game.pending_days = 1;
            next.set(Phase::Advancing);
            return;
        }
    }
}

/// 進行中は残日数だけ出す。カードは消えている。
pub fn spawn_advancing(
    mut commands: Commands,
    panel: Single<Entity, With<PolicyPanel>>,
    game: Res<GameRes>,
) {
    let text =
        game.defs.text.format("ui.policy.advancing", &[("days", &game.pending_days.to_string())]);
    commands.entity(*panel).with_children(|p| {
        p.spawn((DespawnOnExit(Phase::Advancing), label(text, 14.0, INK_DIM)));
    });
}

pub(super) fn field_key(f: BudgetField) -> &'static str {
    match f {
        BudgetField::Education => "ui.field.education",
        BudgetField::Medical => "ui.field.medical",
        BudgetField::Housing => "ui.field.housing",
        BudgetField::Transit => "ui.field.transit",
        BudgetField::Facilities => "ui.field.facilities",
        BudgetField::PublicTeams => "ui.field.public_teams",
        BudgetField::National => "ui.field.national",
        BudgetField::Reserve => "ui.field.reserve",
    }
}

fn describe(game: &GameRes, s: &Shortfall) -> String {
    let t = &game.defs.text;
    let n = |v: f32| format!("{v:.0}");
    match s {
        Shortfall::Budget { field, need, have } => t.format(
            "ui.short.budget",
            &[("field", t.get(field_key(*field))), ("need", &n(*need)), ("have", &n(*have))],
        ),
        Shortfall::Staff { need, have, .. } => {
            t.format("ui.short.staff", &[("need", &n(*need)), ("have", &n(*have))])
        }
        Shortfall::Construction { need, have } => {
            t.format("ui.short.construction", &[("need", &n(*need)), ("have", &n(*have))])
        }
        Shortfall::Facility { .. } => t.get("ui.short.facility").to_string(),
        Shortfall::PriorPolicy { .. } => t.get("ui.short.prior").to_string(),
        Shortfall::NoTrackedPerson => t.get("ui.short.person").to_string(),
        Shortfall::TargetMismatch => t.get("ui.short.target").to_string(),
        Shortfall::NotOwned => t.get("ui.short.owner").to_string(),
        Shortfall::UnknownPolicy => t.get("ui.short.unknown").to_string(),
    }
}
