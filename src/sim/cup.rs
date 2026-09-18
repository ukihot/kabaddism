//! 世界大会と領地（design.md §12.2 / FR-CUP-*）
//!
//! 侵攻や戦争はない。国境は大会の結果によって変わる。
//! **最大損失は大会前に確定表示し、UI に渡した値をそのまま使う**（FR-CUP-03 / AC-11）。

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::defs::Defs;
use super::ids::*;
use super::news::ArticleKind;
use super::world::{PersonStatus, TeamKind};
use super::{Game, news, rng};

/// 他国（簡易モデル: FR-CUP-09）。内政はシミュレートしない。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Nation {
    pub id: NationId,
    pub name: String,
    pub strength: f32,
    pub growth: f32,
    /// 自国かどうか
    pub is_home: bool,
}

/// 大会前に確定し、そのまま結果処理に使う提示内容（FR-CUP-02, 03）。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CupPreview {
    pub year: u16,
    /// 代表として登録された人物
    pub squad: Vec<PersonId>,
    pub squad_strength: f32,
    /// 自国が賭ける領地
    pub staked: Vec<DistrictId>,
    /// 最大損失（領地の評価額 K）。これを超える損失は発生しない。
    pub max_loss: f32,
    /// 各国が賭ける領地（相手から獲得し得るもの）
    pub rival_stakes: Vec<(NationId, DistrictId, f32)>,
    pub opponents: Vec<NationId>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MatchResult {
    pub a: NationId,
    pub b: NationId,
    pub score_a: u8,
    pub score_b: u8,
    /// 見せ場になった自国選手
    pub highlight: Option<PersonId>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CupResult {
    pub year: u16,
    pub matches: Vec<MatchResult>,
    /// 順位（上位から NationId）
    pub ranking: Vec<NationId>,
    pub home_rank: usize,
    /// 獲得した領地
    pub gained: Vec<DistrictId>,
    /// 失った領地
    pub lost: Vec<DistrictId>,
    /// 実際の損失額。max_loss を超えないことをテストで保証する（AC-11）。
    pub realized_loss: f32,
}

/// 領地の評価額（K）。人口・施設・生産基盤から求める。
pub fn district_value(game: &Game, id: DistrictId) -> f32 {
    let d = game.world.district(id);
    let pop = d.population() as f32;
    let facilities: f32 = game
        .world
        .facilities
        .iter()
        .filter(|f| f.district == id && f.state != super::world::FacilityState::Closed)
        .map(|f| f.capacity * 0.5 + f.upkeep)
        .sum();
    let infra = (d.infra.transit + d.infra.housing + d.infra.medical + d.infra.food) * 25.0;
    pop * 0.5 + facilities + infra
}

/// 代表としての評価値。選考も差し替えも同じ物差しを使う。
pub fn rating(game: &Game, id: PersonId) -> f32 {
    let p = game.world.person(id);
    p.life.ability.value * p.life.condition.factor()
}

/// 代表になれる人物を、評価の高い順に並べる。
/// 同値のときは添字順。決定論のため（NFR-01）。
pub fn ranked_candidates(game: &Game) -> Vec<PersonId> {
    let mut cands: Vec<(usize, f32)> = game
        .world
        .people
        .iter()
        .enumerate()
        .filter(|(_, p)| {
            p.status == PersonStatus::Active
                && p.injury.is_none()
                && game.world.district(p.home).owner == game.home
        })
        .map(|(i, p)| (i, p.life.ability.value * p.life.condition.factor()))
        .collect();
    cands.sort_by(|a, b| {
        b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal).then(a.0.cmp(&b.0))
    });
    cands.into_iter().map(|(i, _)| PersonId::from_index(i)).collect()
}

/// 代表選考: `ability × condition` の上位を自動提示する。プレイヤーは差し替えられる。
pub fn propose_squad(game: &Game) -> Vec<PersonId> {
    ranked_candidates(game).into_iter().take(game.defs.balance.cup.squad_size).collect()
}

/// 代表の差し替え（FR-CUP-02）。登録人数は変えず、最も評価の低い登録者と入れ替える。
/// 既に登録済みの人物を渡したときは何も変えない。
pub fn swap_in(game: &Game, squad: &[PersonId], incoming: PersonId) -> Vec<PersonId> {
    let mut next = squad.to_vec();
    if next.contains(&incoming) {
        return next;
    }
    let weakest = next
        .iter()
        .enumerate()
        .min_by(|a, b| {
            rating(game, *a.1)
                .partial_cmp(&rating(game, *b.1))
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(b.0.cmp(&a.0))
        })
        .map(|(i, _)| i);
    match weakest {
        Some(i) => next[i] = incoming,
        None => next.push(incoming),
    }
    next
}

/// 大会前の確定処理。ここで計算した値を UI に渡し、結果処理でも同じ値を使う。
pub fn prepare(game: &mut Game) -> CupPreview {
    let squad = if game.pending_squad.is_empty() {
        propose_squad(game)
    } else {
        game.pending_squad.clone()
    };

    // 代表チームへ登録する
    let nt = game.world.national_team;
    game.world.teams[nt.index()].members = squad.clone();
    for p in &squad {
        game.world.people[p.index()].selected_for_cup = true;
    }
    super::teams::refresh(game);
    let squad_strength = game.world.teams[nt.index()].effective_strength();

    // 賭ける領地: 本拠地は対象外（FR-CUP-04）。評価額の低い順から供出する。
    let n_stake = game.defs.balance.cup.stake_districts;
    let mut own: Vec<(DistrictId, f32)> = game
        .world
        .owned_districts(game.home)
        .filter(|(_, d)| !d.is_home)
        .map(|(i, _)| (i, district_value(game, i)))
        .collect();
    own.sort_by(|a, b| {
        a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal).then(a.0.0.cmp(&b.0.0))
    });
    let staked: Vec<DistrictId> = own.iter().take(n_stake).map(|(i, _)| *i).collect();
    let max_loss: f32 = own.iter().take(n_stake).map(|(_, v)| *v).sum();

    // 相手国の供出領地
    let mut rival_stakes = Vec::new();
    for nation in game.nations.iter().filter(|n| !n.is_home) {
        let mut theirs: Vec<(DistrictId, f32)> = game
            .world
            .owned_districts(nation.id)
            .filter(|(_, d)| !d.is_home)
            .map(|(i, _)| (i, district_value(game, i)))
            .collect();
        theirs.sort_by(|a, b| {
            a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal).then(a.0.0.cmp(&b.0.0))
        });
        if let Some((id, v)) = theirs.first() {
            rival_stakes.push((nation.id, *id, *v));
        }
    }

    let opponents: Vec<NationId> =
        game.nations.iter().filter(|n| !n.is_home).map(|n| n.id).collect();

    CupPreview {
        year: game.date.year,
        squad,
        squad_strength,
        staked,
        max_loss,
        rival_stakes,
        opponents,
    }
}

/// 試合は自動進行する。プレイヤーは結果を見届ける（FR-CUP-05）。
pub fn run(game: &mut Game, preview: &CupPreview) -> CupResult {
    let defs: Arc<Defs> = Arc::clone(&game.defs);
    let slope = defs.balance.cup.logistic_slope;

    // 自国の戦力は代表チームから、他国は簡易モデルから
    let mut strength: Vec<(NationId, f32)> = Vec::new();
    for n in &game.nations {
        let s = if n.is_home { preview.squad_strength } else { n.strength };
        strength.push((n.id, s));
    }

    let mut matches = Vec::new();
    let mut wins: Vec<(NationId, u32, f32)> = strength.iter().map(|(id, s)| (*id, 0, *s)).collect();

    for i in 0..strength.len() {
        for j in (i + 1)..strength.len() {
            let (ia, sa) = strength[i];
            let (ib, sb) = strength[j];
            let p = 1.0 / (1.0 + (-slope * (sa - sb)).exp());
            let a_wins = rng::chance(&mut game.rng.cup, p);
            let margin = rng::range(&mut game.rng.cup, 1.0, 12.0);
            let base = rng::range(&mut game.rng.cup, 20.0, 34.0);
            let (score_a, score_b) = if a_wins {
                ((base + margin) as u8, base as u8)
            } else {
                (base as u8, (base + margin) as u8)
            };
            let highlight = if ia == game.home || ib == game.home {
                pick_highlight(game, &preview.squad)
            } else {
                None
            };
            matches.push(MatchResult { a: ia, b: ib, score_a, score_b, highlight });
            if a_wins {
                wins[i].1 += 1;
            } else {
                wins[j].1 += 1;
            }
        }
    }

    // 順位: 勝数 → 戦力 → ID（決定論）
    wins.sort_by(|a, b| {
        b.1.cmp(&a.1)
            .then(b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal))
            .then(a.0.0.cmp(&b.0.0))
    });
    let ranking: Vec<NationId> = wins.iter().map(|w| w.0).collect();
    let home_rank = ranking.iter().position(|n| *n == game.home).unwrap_or(0);

    // 領地の移動: 優勝国が最下位国の供出領地を受け取る
    let champion = ranking[0];
    let bottom = *ranking.last().unwrap();
    let mut gained = Vec::new();
    let mut lost = Vec::new();
    let mut realized_loss = 0.0;

    if champion != bottom {
        let staked_of_bottom: Option<DistrictId> = if bottom == game.home {
            preview.staked.first().copied()
        } else {
            preview.rival_stakes.iter().find(|(n, _, _)| *n == bottom).map(|(_, d, _)| *d)
        };
        if let Some(d) = staked_of_bottom {
            let value = district_value(game, d);
            game.world.district_mut(d).owner = champion;
            if bottom == game.home {
                lost.push(d);
                // 事前提示を超えないことを保証する（AC-11）
                realized_loss = value.min(preview.max_loss);
            }
            if champion == game.home {
                gained.push(d);
            }
            game.history_note(
                Target::District(d),
                game.date,
                "history.territory",
                &game.nations[champion.index()].name.clone(),
            );
        }
    }

    // 他国の年次成長
    let noise = defs.balance.cup.nation_growth_noise;
    for i in 0..game.nations.len() {
        if game.nations[i].is_home {
            continue;
        }
        let g = game.nations[i].growth * rng::noise(&mut game.rng.cup, noise);
        game.nations[i].strength = (game.nations[i].strength * g).clamp(10.0, 100.0);
    }

    // 代表の疲労
    let fatigue = defs.balance.cup.cup_fatigue;
    for p in &preview.squad {
        let person = &mut game.world.people[p.index()];
        person.life.condition.fatigue = (person.life.condition.fatigue + fatigue).clamp(0.0, 1.5);
        person.caps += 1;
        person.selected_for_cup = false;
    }

    let result = CupResult {
        year: game.date.year,
        matches,
        ranking,
        home_rank,
        gained,
        lost,
        realized_loss,
    };
    report(game, &result);
    game.pending_squad.clear();
    result
}

fn pick_highlight(game: &mut Game, squad: &[PersonId]) -> Option<PersonId> {
    if squad.is_empty() {
        return None;
    }
    let i = rng::range(&mut game.rng.cup, 0.0, squad.len() as f32) as usize;
    Some(squad[i.min(squad.len() - 1)])
}

/// 試合演出は出場選手の背景（出身地区・経歴）を引用する（FR-CUP-06）。
fn report(game: &mut Game, result: &CupResult) {
    let defs: Arc<Defs> = Arc::clone(&game.defs);
    let b = news::Builder { text: &defs.text };

    let rank = (result.home_rank + 1).to_string();
    let mut a = b.article(
        game.date,
        ArticleKind::Cup,
        "cup.result",
        &[("rank", &rank), ("nation", &defs.scenario.nation_name)],
        10,
    );
    a.subjects.push(Subject::Nation(game.home));
    a.pinned = true;
    game.news.push(a);

    for m in &result.matches {
        if m.a != game.home && m.b != game.home {
            continue;
        }
        let Some(pid) = m.highlight else { continue };
        let person = &game.world.people[pid.index()];
        let place = game.world.district(person.home).name.clone();
        let name = person.name.clone();
        let story = person
            .history
            .iter()
            .rev()
            .find(|h| {
                h.text_key == "history.issue_resolved" || h.text_key == "history.project_done"
            })
            .map(|h| h.detail.clone())
            .unwrap_or_default();
        let opponent = if m.a == game.home {
            game.nations[m.b.index()].name.clone()
        } else {
            game.nations[m.a.index()].name.clone()
        };
        let mut a = b.article(
            game.date,
            ArticleKind::Cup,
            "cup.highlight",
            &[("name", &name), ("place", &place), ("story", &story), ("opponent", &opponent)],
            8,
        );
        a.subjects.push(Subject::Person(pid));
        game.news.push(a);
        game.history_note(Target::Person(pid), game.date, "history.cap", &opponent);
    }

    for d in &result.lost {
        let name = game.world.district(*d).name.clone();
        let mut a = b.article(game.date, ArticleKind::Cup, "cup.lost", &[("place", &name)], 9);
        a.subjects.push(Subject::District(*d));
        a.pinned = true;
        game.news.push(a);
    }
    for d in &result.gained {
        let name = game.world.district(*d).name.clone();
        let mut a = b.article(game.date, ArticleKind::Cup, "cup.gained", &[("place", &name)], 9);
        a.subjects.push(Subject::District(*d));
        a.pinned = true;
        game.news.push(a);
    }
}

/// 代表活動が所属先チームの稼働に与える影響（FR-TEAM-03）。
pub fn apply_national_duty(game: &mut Game) {
    for t in game.world.teams.iter_mut() {
        t.on_national_duty = 0.0;
    }
    let squad: Vec<PersonId> = game
        .world
        .people
        .iter()
        .enumerate()
        .filter(|(_, p)| p.selected_for_cup)
        .map(|(i, _)| PersonId::from_index(i))
        .collect();
    for pid in squad {
        if let Some(club) = game.world.people[pid.index()].life.team {
            let t = &mut game.world.teams[club.index()];
            if t.kind != TeamKind::Public {
                t.on_national_duty += 1.0;
            }
        }
    }
}
