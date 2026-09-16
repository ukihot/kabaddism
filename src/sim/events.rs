//! ネガティブイベント（design.md §11.1 / FR-NEG-* / FR-SIM-04〜07, 10）
//!
//! 原則:
//! - 抽選は**日次**。政策カードの日数で期待発生回数が歪まない（FR-SIM-04）。
//! - 全国平均では判定しない。参照するのは常に対象そのものの状態（FR-SIM-05）。
//! - 同一 (event_id, target) の重複を、進行中フラグと cooldown で防ぐ（FR-SIM-06）。
//! - `prerequisite` により予兆 → 本番 → 深刻の連鎖を強制する（FR-SIM-07）。
//! - 解決後も `Issue` は履歴に残る（FR-SIM-10）。

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::calendar::Date;
use super::context::{self, DistrictContext};
use super::defs::{Defs, EventDef, Outcome, Scope, StaffRole, Trigger};
use super::ids::*;
use super::news::ArticleKind;
use super::world::{BusinessState, FacilityKind, FacilityState, Injury, PersonStatus};
use super::{Game, news, policy, rng};

/// 発生してから解決するまでの問題。ニュースのスレッドと1対1で対応する。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Issue {
    pub id: IssueId,
    pub event_id: String,
    pub target: Target,
    pub stage: u8,
    pub opened: Date,
    pub closed: Option<Date>,
    pub thread: Option<ThreadId>,
}

#[derive(Clone, Debug)]
pub struct Fired {
    pub event_id: String,
    pub target: Target,
    pub issue: IssueId,
}

/// ③ 状態と条件に応じてイベントを判定する。
pub fn roll(game: &mut Game) -> Vec<Fired> {
    let defs: Arc<Defs> = Arc::clone(&game.defs);
    let ctxs = context::build(&game.world);
    let today = game.date.absolute();
    let mut fired = Vec::new();

    for def in &defs.events {
        if def.base_chance <= 0.0 {
            continue;
        }
        for target in candidates(game, def.scope) {
            // 同じ対象の同じ問題は重複させない
            if game
                .issues
                .iter()
                .any(|i| i.event_id == def.id && i.target == target && i.closed.is_none())
            {
                continue;
            }
            // cooldown
            if game
                .event_cooldowns
                .iter()
                .any(|(id, t, until)| id == &def.id && *t == target && *until > today)
            {
                continue;
            }
            // 予兆が先に出ていること
            if let Some(pre) = &def.prerequisite
                && !game
                    .issues
                    .iter()
                    .any(|i| &i.event_id == pre && i.target == target && i.closed.is_none())
            {
                continue;
            }
            if !triggers_hold(game, &ctxs, def, target) {
                continue;
            }
            if !rng::chance(&mut game.rng.events, def.base_chance) {
                continue;
            }

            let thread = game.news.open_thread(target.as_subject(), def.id.clone(), game.date);
            let id = IssueId(game.issues.len() as u16);
            game.issues.push(Issue {
                id,
                event_id: def.id.clone(),
                target,
                stage: def.stage,
                opened: game.date,
                closed: None,
                thread: Some(thread),
            });
            game.event_cooldowns.push((def.id.clone(), target, today + def.cooldown_days as u32));
            fired.push(Fired { event_id: def.id.clone(), target, issue: id });
        }
    }

    game.event_cooldowns.retain(|(_, _, until)| *until > today);
    fired
}

fn candidates(game: &Game, scope: Scope) -> Vec<Target> {
    match scope {
        Scope::District => game
            .world
            .owned_districts(game.home)
            .map(|(i, _)| Target::District(i))
            .collect(),
        Scope::Facility => game
            .world
            .facilities
            .iter()
            .enumerate()
            .filter(|(_, f)| {
                f.state != FacilityState::Closed && game.world.district(f.district).owner == game.home
            })
            .map(|(i, _)| Target::Facility(FacilityId::from_index(i)))
            .collect(),
        Scope::Person => game
            .world
            .people
            .iter()
            .enumerate()
            .filter(|(_, p)| p.status == PersonStatus::Active)
            .map(|(i, _)| Target::Person(PersonId::from_index(i)))
            .collect(),
        Scope::Cohort => {
            let mut v = Vec::new();
            for (di, d) in game.world.districts.iter().enumerate() {
                if d.owner != game.home {
                    continue;
                }
                for ci in 0..d.cohorts.len() {
                    v.push(Target::Cohort(CohortId::new(DistrictId::from_index(di), ci)));
                }
            }
            v
        }
        Scope::Business => game
            .world
            .businesses
            .iter()
            .enumerate()
            .filter(|(_, b)| b.state != BusinessState::Closed)
            .map(|(i, _)| Target::Business(BusinessId::from_index(i)))
            .collect(),
        Scope::Nation => vec![Target::Nation],
    }
}

fn triggers_hold(game: &Game, ctxs: &[DistrictContext], def: &EventDef, target: Target) -> bool {
    def.trigger.iter().all(|t| trigger_holds(game, ctxs, *t, target))
}

fn trigger_holds(game: &Game, ctxs: &[DistrictContext], t: Trigger, target: Target) -> bool {
    let w = &game.world;
    let dctx = target.district().map(|d| &ctxs[d.index()]);

    // 対象そのものの生活状態
    let life = match target {
        Target::Person(p) => Some(&w.people[p.index()].life),
        Target::Cohort(c) => Some(&w.districts[c.district.index()].cohorts[c.index as usize].life),
        _ => None,
    };

    // 地区の平均（地区が対象のとき）
    let district_mean = |f: fn(&super::world::Life) -> f32| -> f32 {
        match target.district() {
            Some(d) => {
                let dd = w.district(d);
                let mut num = 0.0;
                let mut den = 0.0;
                for c in &dd.cohorts {
                    num += c.headcount as f32 * f(&c.life);
                    den += c.headcount as f32;
                }
                if den > 0.0 { num / den } else { 0.0 }
            }
            None => 0.0,
        }
    };

    match t {
        Trigger::LifeSlackBelow(v) => match life {
            Some(l) => l.life_slack < v,
            None => district_mean(|l| l.life_slack) < v,
        },
        Trigger::FatigueAbove(v) => match life {
            Some(l) => l.condition.fatigue > v,
            None => district_mean(|l| l.condition.fatigue) > v,
        },
        Trigger::HealthBelow(v) => match life {
            Some(l) => l.condition.health < v,
            None => district_mean(|l| l.condition.health) < v,
        },
        Trigger::NutritionBelow(v) => match life {
            Some(l) => l.condition.nutrition < v,
            None => district_mean(|l| l.condition.nutrition) < v,
        },
        Trigger::ParticipationBelow(v) => match life {
            Some(l) => l.participation < v,
            None => district_mean(|l| l.participation) < v,
        },
        Trigger::ObligationRatioAbove(v) => match life {
            Some(l) => l.obligation / l.credit_limit.max(0.1) > v,
            None => district_mean(|l| l.obligation / l.credit_limit.max(0.1)) > v,
        },
        Trigger::MedicalAccessBelow(v) => dctx.is_some_and(|c| c.infra.medical < v),
        Trigger::FoodAccessBelow(v) => dctx.is_some_and(|c| c.infra.food < v),
        Trigger::HousingBelow(v) => dctx.is_some_and(|c| c.infra.housing < v),
        Trigger::TransitBelow(v) => dctx.is_some_and(|c| c.infra.transit < v),
        Trigger::TrustBelow(v) => dctx.is_some_and(|c| c.infra.trust < v),
        Trigger::ScoutingBelow(v) => dctx.is_some_and(|c| c.infra.scouting < v),
        Trigger::PaymentVenueBelow(v) => dctx.is_some_and(|c| c.infra.payment_venue < v),
        Trigger::OverEnrolled(v) => match target {
            Target::Facility(f) => {
                let f = &w.facilities[f.index()];
                let cap = (f.capacity * f.staffing().min(1.0)).max(0.1);
                f.enrolled / cap > v
            }
            _ => false,
        },
        Trigger::FacilityKindIs(k) => match target {
            Target::Facility(f) => w.facilities[f.index()].kind == k,
            _ => false,
        },
        Trigger::StaffingBelow(v) => match target {
            Target::Facility(f) => w.facilities[f.index()].staffing() < v,
            _ => false,
        },
        Trigger::QualityBelow(v) => match target {
            Target::Facility(f) => w.facilities[f.index()].quality < v,
            _ => false,
        },
        Trigger::BusinessBalanceBelow(v) => match target {
            Target::Business(b) => w.businesses[b.index()].cum_balance < v,
            _ => false,
        },
        Trigger::GuildCoverageBelow(v) => w.guild.coverage < v,
        Trigger::ReserveBelow(v) => {
            game.treasury.remaining(super::defs::BudgetField::Reserve) < v
        }
        Trigger::ProjectStalled => game
            .projects
            .iter()
            .any(|p| p.state == policy::ProjectState::Stalled && p.target == target),
        Trigger::NotInjured => match target {
            Target::Person(p) => w.people[p.index()].injury.is_none(),
            _ => true,
        },
        Trigger::AbilityAbove(v) => match target {
            Target::Person(p) => w.people[p.index()].life.ability.value > v,
            _ => false,
        },
    }
}

/// ④ イベントの結果を、人物・施設・地域へ反映する。
pub fn apply(game: &mut Game, fired: &[Fired]) {
    let defs: Arc<Defs> = Arc::clone(&game.defs);
    for f in fired {
        let Some(def) = defs.event(&f.event_id) else { continue };
        for outcome in &def.outcomes {
            apply_outcome(game, outcome, f.target);
        }
        emit_news(game, def, f);
        let label = defs.text.get(&format!("{}.headline", def.news_key)).to_string();
        game.history_note(f.target, game.date, "history.issue", &label);
    }
}

fn apply_outcome(game: &mut Game, outcome: &Outcome, target: Target) {
    match outcome {
        Outcome::Effect(e) => {
            // 事業を介さない即時の環境変化（悪化方向）
            policy::apply_effect(game, e, target, ProjectId(u16::MAX));
        }
        Outcome::MissPractice { ratio } => {
            with_lives(game, target, |l| {
                let lost = l.time.practice * *ratio;
                l.time.practice -= lost;
                l.time.leisure += lost;
                l.participation *= 1.0 - *ratio;
            });
        }
        Outcome::Injure { severity } => {
            if let Target::Person(p) = target {
                let base = game.defs.balance.events.injury_days_base;
                let person = &mut game.world.people[p.index()];
                person.injury = Some(Injury {
                    days_remaining: (base * severity).round().max(1.0) as u16,
                    severity: *severity,
                });
                // 離脱中は練習できない。能力はここで下がり始める（design.md §7）。
                person.life.time.practice = 0.0;
                person.life.participation = 0.0;
                person.life.ability.value *= 1.0 - 0.02 * severity;
            }
        }
        Outcome::Pause { grace_days } => {
            if let Target::Person(p) = target {
                let person = &mut game.world.people[p.index()];
                if *grace_days == 0 {
                    person.status = PersonStatus::Paused;
                    person.life.time.practice = 0.0;
                } else {
                    person.life.motivation *= 0.7;
                }
            }
        }
        Outcome::Emigrate => {
            if let Target::Person(p) = target {
                game.world.people[p.index()].status = PersonStatus::Emigrated;
            }
        }
        Outcome::SuspendFacility => {
            if let Target::Facility(f) = target {
                game.world.facilities[f.index()].state = FacilityState::Suspended;
            }
        }
        Outcome::DamageFacility { quality } => {
            if let Target::Facility(f) = target {
                let fac = &mut game.world.facilities[f.index()];
                fac.quality = (fac.quality - *quality).clamp(0.0, 1.0);
            }
        }
        Outcome::StaffLeave { role, amount, days } => {
            let i = role.index();
            game.treasury.real.staff_total[i] = (game.treasury.real.staff_total[i] - amount).max(0.0);
            game.dispatches.push(policy::Dispatch {
                role: *role,
                amount: -*amount,
                days_remaining: *days,
                district: target.district(),
            });
            if let Target::Facility(f) = target {
                let fac = &mut game.world.facilities[f.index()];
                fac.staff = (fac.staff - amount).max(0.0);
                if fac.staffing() < 0.5 {
                    fac.state = FacilityState::Suspended;
                }
            }
        }
        Outcome::StallProject => {
            for p in game.projects.iter_mut() {
                if p.target == target && !p.is_done() {
                    p.state = policy::ProjectState::Stalled;
                }
            }
        }
        Outcome::ShrinkCredit { ratio } => {
            for d in game.world.districts.iter_mut() {
                for c in d.cohorts.iter_mut() {
                    c.life.credit_limit *= ratio;
                }
            }
            for p in game.world.people.iter_mut() {
                p.life.credit_limit *= ratio;
            }
            game.world.guild.credit_base *= ratio;
        }
        Outcome::HoldInvestment { trust } => {
            if let Some(d) = target.district() {
                game.world
                    .district_mut(d)
                    .infra
                    .add(super::world::InfraField::Trust, -*trust);
                for b in game.world.businesses.iter_mut() {
                    if b.district == d && b.state != BusinessState::Closed {
                        b.scale = (b.scale * 0.95).max(0.1);
                    }
                }
            }
        }
        Outcome::MissSelection => {
            if let Some(d) = target.district() {
                game.world
                    .district_mut(d)
                    .infra
                    .add(super::world::InfraField::Scouting, -0.1);
            }
        }
        Outcome::CongestPayment { minutes } => {
            with_lives(game, target, |l| {
                l.pending_congestion += *minutes;
            });
        }
        Outcome::ShrinkBusiness { ratio } => {
            if let Target::Business(b) = target {
                let biz = &mut game.world.businesses[b.index()];
                biz.scale = (biz.scale * ratio).max(0.05);
                biz.state = BusinessState::Shrinking;
            }
        }
    }
}

fn with_lives(game: &mut Game, target: Target, mut f: impl FnMut(&mut super::world::Life)) {
    match target {
        Target::Person(p) => f(&mut game.world.people[p.index()].life),
        Target::Cohort(c) => {
            f(&mut game.world.districts[c.district.index()].cohorts[c.index as usize].life)
        }
        Target::District(d) => {
            for c in game.world.districts[d.index()].cohorts.iter_mut() {
                f(&mut c.life);
            }
            let ids: Vec<usize> = game
                .world
                .people
                .iter()
                .enumerate()
                .filter(|(_, p)| p.home == d)
                .map(|(i, _)| i)
                .collect();
            for i in ids {
                f(&mut game.world.people[i].life);
            }
        }
        Target::Facility(fid) => {
            let d = game.world.facilities[fid.index()].district;
            with_lives(game, Target::District(d), f);
        }
        _ => {}
    }
}

fn emit_news(game: &mut Game, def: &EventDef, fired: &Fired) {
    let defs: Arc<Defs> = Arc::clone(&game.defs);
    let place = policy::target_name(game, fired.target);
    let b = news::Builder { text: &defs.text };
    let mut a = b.article(
        game.date,
        def.kind,
        &def.news_key,
        &[("place", &place), ("name", &place)],
        def.weight,
    );
    a.subjects.push(fired.target.as_subject());
    a.thread = game.issues[fired.issue.index()].thread;
    // ③「なぜ」と、対応策への直リンク（FR-UI-02 / FR-NEG-02）
    a.because.push(defs.text.get(&format!("{}.because", def.news_key)).to_string());
    a.remedies = def.remedies.clone();
    game.news.push(a);
}

/// 条件が解消された問題を閉じる。閉じた `Issue` は履歴として残る（FR-SIM-10）。
pub fn resolve(game: &mut Game) {
    let defs: Arc<Defs> = Arc::clone(&game.defs);
    let ctxs = context::build(&game.world);
    let mut closed: Vec<(usize, Target, String)> = Vec::new();

    for (i, issue) in game.issues.iter().enumerate() {
        if issue.closed.is_some() {
            continue;
        }
        let Some(def) = defs.event(&issue.event_id) else { continue };
        if !triggers_hold(game, &ctxs, def, issue.target) {
            closed.push((i, issue.target, issue.event_id.clone()));
        }
    }

    for (i, target, event_id) in closed {
        game.issues[i].closed = Some(game.date);
        let thread = game.issues[i].thread;
        let Some(def) = defs.event(&event_id) else { continue };
        let place = policy::target_name(game, target);
        let b = news::Builder { text: &defs.text };
        let mut a = b.article(
            game.date,
            def.kind,
            &format!("{}.resolved", def.news_key),
            &[("place", &place), ("name", &place)],
            def.weight.saturating_sub(1).max(1),
        );
        a.subjects.push(target.as_subject());
        a.thread = thread;
        game.news.push(a);
        if let Some(t) = thread {
            game.news.close_thread(t, game.date);
        }
        game.history_note(target, game.date, "history.issue_resolved", &place);
        // 施設の受け入れ停止は、人員が戻れば再開する
        if let Target::Facility(f) = target {
            let fac = &mut game.world.facilities[f.index()];
            if fac.state == FacilityState::Suspended && fac.staffing() >= 0.9 {
                fac.state = FacilityState::Operating;
            }
        }
    }
}

/// 進行中の問題（UI の「何が起きているか」と推奨カードの素）。
pub fn open_issues(game: &Game) -> Vec<&Issue> {
    game.issues.iter().filter(|i| i.closed.is_none()).collect()
}

/// 施設の状態から問題を読み取る（FR-NEWS-08: 記事を読まなくても把握できる）。
pub fn facility_alerts(game: &Game) -> Vec<(FacilityId, &'static str)> {
    let mut v = Vec::new();
    for (i, f) in game.world.facilities.iter().enumerate() {
        let id = FacilityId::from_index(i);
        if f.state == FacilityState::Suspended {
            v.push((id, "suspended"));
        } else if f.kind == FacilityKind::Dojo && f.enrolled > f.capacity * f.staffing().min(1.0) {
            v.push((id, "over_enrolled"));
        } else if f.staffing() < 0.7 {
            v.push((id, "understaffed"));
        } else if f.quality < 0.4 {
            v.push((id, "damaged"));
        }
    }
    v
}

#[allow(dead_code)]
fn unused(_: StaffRole, _: ArticleKind) {}
