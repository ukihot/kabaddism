//! 情報の3段階の②③（design.md §13.2 / FR-UI-02）
//!
//! ①見出しはニュース欄に常時表示済み。ここはクリック後の②現況と、
//! `stats::aggregate` が残した内訳（③なぜ）をそのまま出す。UI 側では再計算しない（NFR-09）。

use bevy::prelude::*;
use kbism::sim::ids::Subject;
use kbism::sim::world::{BusinessState, FacilityState, PersonStatus};

use super::super::GameRes;
use super::{ACCENT, INK, INK_DIM, ModalRoot, button, label, modal};

#[derive(Component)]
pub struct Close;

#[derive(Component)]
pub struct InspectorPanel;

/// ニュース見出しなど、遷移元のボタンに付ける（FR-NEWS-03）。
#[derive(Component, Clone, Copy)]
pub struct OpenSubject(pub Subject);

/// ②の表示内容。`sim` の現在状態を読んだ結果で、UI が持つ状態ではない。
struct SubjectView {
    heading: String,
    detail: String,
}

pub fn open(
    mut commands: Commands,
    root: Single<Entity, With<ModalRoot>>,
    existing: Query<Entity, With<InspectorPanel>>,
    game: Res<GameRes>,
    q: Query<(&Interaction, &OpenSubject), Changed<Interaction>>,
) {
    let Some((_, target)) = q.iter().find(|(i, _)| **i == Interaction::Pressed) else {
        return;
    };
    let subject = target.0;

    for e in &existing {
        commands.entity(e).despawn();
    }

    let view = describe(&game, subject);
    let why: Vec<String> = game
        .stats
        .today()
        .map(|d| {
            d.breakdown
                .iter()
                .filter(|b| b.subject == Some(subject))
                .map(|b| b.describe(&game.defs.text))
                .collect()
        })
        .unwrap_or_default();
    let why_heading = game.defs.text.get("ui.inspector.why").to_string();
    let close_label = game.defs.text.get("ui.inspector.close").to_string();

    commands.entity(*root).with_children(|p| {
        p.spawn((InspectorPanel, modal())).with_children(|c| {
            c.spawn(label(view.heading, 18.0, INK));
            c.spawn(label(view.detail, 13.0, INK_DIM));
            if !why.is_empty() {
                c.spawn(label(why_heading, 12.0, INK_DIM));
                for line in why {
                    c.spawn(label(format!("・{line}"), 12.0, INK_DIM));
                }
            }
            c.spawn(button(Close, true)).with_children(|x| {
                x.spawn(label(close_label, 14.0, ACCENT));
            });
        });
    });
}

pub fn close(
    mut commands: Commands,
    panel: Query<Entity, With<InspectorPanel>>,
    q: Query<&Interaction, (Changed<Interaction>, With<Close>)>,
) {
    if q.iter().any(|i| *i == Interaction::Pressed) {
        for e in &panel {
            commands.entity(e).despawn();
        }
    }
}

/// ②現況。`sim` の現在状態を読むだけで、独自の集計はしない。
fn describe(game: &GameRes, subject: Subject) -> SubjectView {
    let t = &game.defs.text;
    let (heading, detail) = match subject {
        Subject::District(id) => {
            let d = game.world.district(id);
            let stats =
                game.stats.today().and_then(|s| s.by_district.iter().find(|x| x.district == id.0));
            let detail = match stats {
                Some(s) => t.format(
                    "ui.inspector.district",
                    &[
                        ("population", &s.population.to_string()),
                        ("ability", &format!("{:.1}", s.mean_ability)),
                        ("participation", &format!("{:.0}%", s.participation * 100.0)),
                        ("slack", &format!("{:.0}%", s.life_slack * 100.0)),
                    ],
                ),
                None => t.get("ui.inspector.unknown").to_string(),
            };
            (d.name.clone(), detail)
        }
        Subject::Facility(id) => {
            let f = &game.world.facilities[id.index()];
            let detail = t.format(
                "ui.inspector.facility",
                &[
                    ("enrolled", &format!("{:.0}", f.enrolled)),
                    ("capacity", &format!("{:.0}", f.capacity)),
                    ("staff", &format!("{:.0}", f.staff)),
                    ("required", &format!("{:.0}", f.staff_required)),
                ],
            );
            let suffix = if f.state == FacilityState::Suspended { " ⚠" } else { "" };
            (format!("{}{suffix}", f.name), detail)
        }
        Subject::Person(id) => {
            let p = game.world.person(id);
            let detail = t.format(
                "ui.inspector.person",
                &[
                    ("ability", &format!("{:.1}", p.life.ability.value)),
                    ("health", &format!("{:.0}%", p.life.condition.health * 100.0)),
                    ("fatigue", &format!("{:.0}%", p.life.condition.fatigue * 100.0)),
                ],
            );
            let suffix = if p.status != PersonStatus::Active {
                format!(" ({:?})", p.status)
            } else {
                String::new()
            };
            (format!("{}{suffix}", p.name), detail)
        }
        Subject::Team(id) => {
            let team = game.world.team(id);
            let detail = t.format(
                "ui.inspector.team",
                &[
                    ("strength", &format!("{:.1}", team.strength)),
                    ("cohesion", &format!("{:.0}%", team.cohesion * 100.0)),
                    ("fatigue", &format!("{:.0}%", team.fatigue * 100.0)),
                ],
            );
            (team.name.clone(), detail)
        }
        Subject::Business(id) => {
            let b = &game.world.businesses[id.index()];
            let detail = t.format(
                "ui.inspector.business",
                &[
                    ("state", &format!("{:?}", b.state)),
                    ("balance", &format!("{:.0}", b.cum_balance)),
                ],
            );
            let suffix = if b.state == BusinessState::Closed { " ⚠" } else { "" };
            (format!("{}{suffix}", b.name), detail)
        }
        Subject::Cohort(cid) => {
            let d = game.world.district(cid.district);
            let c = &d.cohorts[cid.index as usize];
            let detail = t.format(
                "ui.inspector.cohort",
                &[
                    ("population", &c.headcount.to_string()),
                    ("ability", &format!("{:.1}", c.life.ability.value)),
                    ("participation", &format!("{:.0}%", c.life.participation * 100.0)),
                ],
            );
            (format!("{} / {:?}", d.name, c.age_band), detail)
        }
        Subject::Project(id) => match game.projects.iter().find(|p| p.id == id) {
            Some(p) => {
                let detail = t.format(
                    "ui.inspector.project",
                    &[("state", &format!("{:?}", p.state)), ("started", &p.started.to_string())],
                );
                (p.policy.clone(), detail)
            }
            None => (t.get("ui.inspector.unknown").to_string(), String::new()),
        },
        Subject::Nation(id) => match game.nations.iter().find(|n| n.id == id) {
            Some(n) => {
                let detail = t.format(
                    "ui.inspector.nation",
                    &[
                        ("strength", &format!("{:.1}", n.strength)),
                        ("growth", &format!("{:.2}", n.growth)),
                    ],
                );
                (n.name.clone(), detail)
            }
            None => (t.get("ui.inspector.unknown").to_string(), String::new()),
        },
    };
    SubjectView { heading, detail }
}
