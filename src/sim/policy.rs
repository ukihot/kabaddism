//! 政策カードと事業（design.md §10 / FR-POL-*）
//!
//! カードは「決定・手続き」を表す。それが起こす**事業 (Project)** は別途の期間をかけて完成し、
//! 次の政策の実行中も並行して進む（FR-POL-04）。
//!
//! `Effect` が触れる先は Infra / Facility / 利用権 / 時間コスト / 受入枠 / 支援額のみ。
//! `Ability` を書ける項目を enum に作らないことで、型で規律を担保する（FR-POL-06）。

use serde::{Deserialize, Serialize};

use super::calendar::Date;
use super::defs::{BudgetField, Effect, PolicyDef, Requirement, StaffRole, TargetScope};
use super::ids::*;
use super::news::ArticleKind;
use super::world::{
    AccessRule, BusinessState, Facility, FacilityKind, FacilityState, HistoryEntry, TeamKind,
};
use super::{Game, news};

/// 実行できない理由（FR-POL-03: 不足している条件を明示する）。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Shortfall {
    Budget { field: BudgetField, need: f32, have: f32 },
    Staff { role: StaffRole, need: f32, have: f32 },
    Facility { kind: FacilityKind },
    PriorPolicy { id: String },
    NoTrackedPerson,
    Construction { need: f32, have: f32 },
    TargetMismatch,
    NotOwned,
    UnknownPolicy,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum ProjectState {
    /// 決定・調整・手続き
    Planning,
    /// 工事
    Construction,
    /// 効果発現待ち
    Lead,
    /// 稼働中
    Operating,
    /// 資源不足で止まっている
    Stalled,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Project {
    pub id: ProjectId,
    pub policy: String,
    pub target: Target,
    pub started: Date,
    pub procedure_days: u16,
    pub construction_days: u16,
    pub lead_days: u16,
    /// 工事1日あたりに必要な建設仕事
    pub buildwork_per_day: f32,
    pub state: ProjectState,
    pub stalled_days: u16,
    /// 抱えている人員（完了後も運営に必要な分は保持し続ける）
    pub staff_held: [f32; 5],
    pub upkeep: f32,
    pub field: BudgetField,
    pub thread: Option<ThreadId>,
}

impl Project {
    pub fn is_done(&self) -> bool {
        self.state == ProjectState::Operating
    }
}

/// 人員の一時派遣（派遣元の余力を使う）。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Dispatch {
    pub role: StaffRole,
    pub amount: f32,
    pub days_remaining: u16,
    pub district: Option<DistrictId>,
}

// ───────────────────────────── 実行可否 ─────────────────────────────

/// 実行可能条件を検証し、不足をすべて列挙する（FR-POL-03）。
pub fn check(game: &Game, def: &PolicyDef, target: Target) -> Vec<Shortfall> {
    let mut miss = Vec::new();

    match (def.target, target) {
        (TargetScope::District, Target::District(_)) => {}
        (TargetScope::Nation, Target::Nation) => {}
        (TargetScope::NationalTeam, Target::Nation) => {}
        _ => miss.push(Shortfall::TargetMismatch),
    }

    if let Some(d) = target.district()
        && game.world.district(d).owner != game.home
    {
        miss.push(Shortfall::NotOwned);
    }

    // 予算枠（枠と実資源は別判定: FR-BUD-05）
    let have = game.treasury.remaining(def.field);
    if have + 1e-4 < def.initial_cost {
        miss.push(Shortfall::Budget { field: def.field, need: def.initial_cost, have });
    }

    for req in &def.requires {
        match req {
            Requirement::Budget { field } => {
                let have = game.treasury.remaining(*field);
                if have <= 0.0 {
                    miss.push(Shortfall::Budget { field: *field, need: def.initial_cost, have });
                }
            }
            Requirement::Staff { role, amount } => {
                let have = game.treasury.real.available(*role);
                if have + 1e-4 < *amount {
                    miss.push(Shortfall::Staff { role: *role, need: *amount, have });
                }
            }
            Requirement::Facility { kind } => {
                let ok = match target.district() {
                    Some(d) => game.world.facilities_in(d, *kind).any(|(_, f)| f.is_running()),
                    None => game.world.facilities.iter().any(|f| f.kind == *kind && f.is_running()),
                };
                if !ok {
                    miss.push(Shortfall::Facility { kind: *kind });
                }
            }
            Requirement::PriorPolicy { id } => {
                let done = game.projects.iter().any(|p| p.policy == *id && p.is_done());
                if !done {
                    miss.push(Shortfall::PriorPolicy { id: id.clone() });
                }
            }
            Requirement::TrackedPerson => {
                let exists = match target.district() {
                    Some(d) => game.world.people.iter().any(|p| p.home == d && p.is_active()),
                    None => game.world.people.iter().any(|p| p.is_active()),
                };
                if !exists {
                    miss.push(Shortfall::NoTrackedPerson);
                }
            }
            Requirement::Construction { amount } => {
                let have = game.world.stock_buildwork + game.treasury.real.materials;
                if have + 1e-4 < *amount {
                    miss.push(Shortfall::Construction { need: *amount, have });
                }
            }
        }
    }
    miss
}

// ───────────────────────────── 実行 ─────────────────────────────

/// 政策カードを実行する。成功すると `pending_days` が設定され、時間が進み始める（FR-TIME-02）。
pub fn execute(
    game: &mut Game,
    policy_id: &str,
    target: Target,
) -> Result<ProjectId, Vec<Shortfall>> {
    let defs = std::sync::Arc::clone(&game.defs);
    let Some(def) = defs.policy(policy_id) else {
        return Err(vec![Shortfall::UnknownPolicy]);
    };

    let miss = check(game, def, target);
    if !miss.is_empty() {
        return Err(miss);
    }

    // 枠から初期費用を引き当てる
    game.treasury.commit(def.field, def.initial_cost);

    // 必要人員を確保（完了後も運営に必要な分は保持し続ける）
    let mut staff_held = [0.0f32; 5];
    for req in &def.requires {
        if let Requirement::Staff { role, amount } = req {
            game.treasury.real.assign(*role, *amount);
            staff_held[role.index()] += *amount;
        }
    }

    let id = ProjectId(game.projects.len() as u16);
    let construction_days = def.construction_days;
    let buildwork_per_day = if construction_days > 0 {
        def.initial_cost / construction_days as f32 * defs.balance.budget.construction_rate
    } else {
        0.0
    };
    let lead_days = {
        let lo = def.lead_time[0];
        let hi = def.lead_time[1].max(lo);
        if hi > lo {
            lo + (super::rng::range(&mut game.rng.daily, 0.0, (hi - lo + 1) as f32) as u16)
                .min(hi - lo)
        } else {
            lo
        }
    };

    let label = def.name.clone();
    let thread = game.news.open_thread(target.as_subject(), label.clone(), game.date);

    game.projects.push(Project {
        id,
        policy: def.id.clone(),
        target,
        started: game.date,
        procedure_days: def.days as u16,
        construction_days,
        lead_days,
        buildwork_per_day,
        state: ProjectState::Planning,
        stalled_days: 0,
        staff_held,
        upkeep: def.upkeep,
        field: def.field,
        thread: Some(thread),
    });

    // 記事: 政策の決定
    let b = news::Builder { text: &defs.text };
    let mut a = b.article(
        game.date,
        ArticleKind::Policy,
        "policy.enacted",
        &[
            ("name", &def.name),
            ("official", &def.official_name),
            ("place", &target_name(game, target)),
        ],
        6,
    );
    a.subjects.push(target.as_subject());
    a.thread = Some(thread);
    a.because.push(defs.text.get(&def.uncertainty_key).to_string());
    game.news.push(a);

    game.pending_days = def.days as u16;
    game.history_note(target, game.date, "history.policy", &def.name);
    Ok(id)
}

pub fn target_name(game: &Game, target: Target) -> String {
    match target {
        Target::District(d) => game.world.district(d).name.clone(),
        Target::Facility(f) => game.world.facilities[f.index()].name.clone(),
        Target::Person(p) => game.world.people[p.index()].name.clone(),
        Target::Cohort(c) => game.world.district(c.district).name.clone(),
        Target::Business(b) => game.world.businesses[b.index()].name.clone(),
        Target::Nation => game.defs.scenario.nation_name.clone(),
    }
}

// ───────────────────────────── 日次の進捗 ─────────────────────────────

/// ① 政策・契約・工事・育成事業を進める（FR-SIM-01）。
pub fn advance_projects(game: &mut Game) {
    // 派遣の期限切れ
    let mut expired: Vec<(StaffRole, f32)> = Vec::new();
    game.dispatches.retain_mut(|d| {
        d.days_remaining = d.days_remaining.saturating_sub(1);
        if d.days_remaining == 0 {
            expired.push((d.role, d.amount));
            false
        } else {
            true
        }
    });
    for (role, amount) in expired {
        let i = role.index();
        game.treasury.real.staff_total[i] = (game.treasury.real.staff_total[i] - amount).max(0.0);
    }

    let mut completed: Vec<ProjectId> = Vec::new();
    let mut newly_stalled: Vec<ProjectId> = Vec::new();

    for i in 0..game.projects.len() {
        match game.projects[i].state {
            ProjectState::Operating => continue,
            ProjectState::Planning => {
                let p = &mut game.projects[i];
                p.procedure_days = p.procedure_days.saturating_sub(1);
                if p.procedure_days == 0 {
                    p.state = if p.construction_days > 0 {
                        ProjectState::Construction
                    } else {
                        ProjectState::Lead
                    };
                }
            }
            ProjectState::Construction | ProjectState::Stalled => {
                let need = game.projects[i].buildwork_per_day;
                if game.world.stock_buildwork + 1e-4 >= need {
                    game.world.stock_buildwork -= need;
                    game.ledger.used_buildwork += need;
                    let p = &mut game.projects[i];
                    p.state = ProjectState::Construction;
                    p.construction_days = p.construction_days.saturating_sub(1);
                    if p.construction_days == 0 {
                        p.state = ProjectState::Lead;
                    }
                } else {
                    let p = &mut game.projects[i];
                    if p.state != ProjectState::Stalled {
                        newly_stalled.push(p.id);
                    }
                    p.state = ProjectState::Stalled;
                    p.stalled_days += 1;
                }
            }
            ProjectState::Lead => {
                let p = &mut game.projects[i];
                if p.lead_days > 0 {
                    p.lead_days -= 1;
                }
                if p.lead_days == 0 {
                    p.state = ProjectState::Operating;
                    completed.push(p.id);
                }
            }
        }
    }

    for id in newly_stalled {
        stall_news(game, id);
    }
    for id in completed {
        complete(game, id);
    }
}

fn stall_news(game: &mut Game, id: ProjectId) {
    let defs = std::sync::Arc::clone(&game.defs);
    let p = &game.projects[id.index()];
    let (target, thread, policy) = (p.target, p.thread, p.policy.clone());
    let name = defs.policy(&policy).map(|d| d.name.clone()).unwrap_or(policy);
    let b = news::Builder { text: &defs.text };
    let mut a = b.article(
        game.date,
        ArticleKind::Project,
        "project.stalled",
        &[("name", &name), ("place", &target_name(game, target))],
        5,
    );
    a.subjects.push(target.as_subject());
    a.thread = thread;
    a.because.push(defs.text.get("because.buildwork_short").to_string());
    game.news.push(a);
}

fn complete(game: &mut Game, id: ProjectId) {
    let defs = std::sync::Arc::clone(&game.defs);
    let (policy, target, thread) = {
        let p = &game.projects[id.index()];
        (p.policy.clone(), p.target, p.thread)
    };
    let Some(def) = defs.policy(&policy) else { return };

    for e in &def.effects {
        apply_effect(game, e, target, id);
    }

    let b = news::Builder { text: &defs.text };
    let mut a = b.article(
        game.date,
        ArticleKind::Project,
        "project.completed",
        &[("name", &def.name), ("place", &target_name(game, target))],
        6,
    );
    a.subjects.push(target.as_subject());
    a.thread = thread;
    game.news.push(a);
    game.history_note(target, game.date, "history.project_done", &def.name);
}

// ───────────────────────────── 効果の適用 ─────────────────────────────

pub fn apply_effect(game: &mut Game, effect: &Effect, target: Target, project: ProjectId) {
    let districts: Vec<DistrictId> = match target {
        Target::District(d) => vec![d],
        Target::Cohort(c) => vec![c.district],
        Target::Nation => game.world.owned_districts(game.home).map(|(i, _)| i).collect(),
        _ => Vec::new(),
    };

    match effect {
        Effect::Infra { field, delta } => {
            for d in districts {
                game.world.district_mut(d).infra.add(*field, *delta);
            }
        }

        Effect::BuildFacility {
            kind,
            name_key,
            capacity,
            staff_required,
            upkeep,
            access,
            night_open,
        } => {
            let defs = std::sync::Arc::clone(&game.defs);
            for d in districts {
                let name = format!("{}{}", game.world.district(d).name, defs.text.get(name_key));
                game.world.facilities.push(Facility {
                    name,
                    kind: *kind,
                    district: d,
                    capacity: *capacity,
                    staff_required: *staff_required,
                    staff: 0.0,
                    upkeep: *upkeep,
                    state: FacilityState::Operating,
                    access: *access,
                    night_open: *night_open,
                    quality: 1.0,
                    enrolled: 0.0,
                    history: vec![HistoryEntry {
                        date: game.date,
                        text_key: "history.facility_built".into(),
                        detail: String::new(),
                    }],
                });
            }
            // 事業が抱えていた人員を新設施設へ配置する
            let held = game.projects[project.index()].staff_held;
            let n = game.world.facilities.len();
            if n > 0 {
                let total: f32 = held.iter().sum();
                if total > 0.0 {
                    game.world.facilities[n - 1].staff += total;
                }
            }
        }

        Effect::ExpandFacility { kind, capacity_delta, quality_delta } => {
            for d in districts {
                let ids: Vec<usize> = game
                    .world
                    .facilities
                    .iter()
                    .enumerate()
                    .filter(|(_, f)| f.district == d && f.kind == *kind)
                    .map(|(i, _)| i)
                    .collect();
                if ids.is_empty() {
                    continue;
                }
                let per = *capacity_delta / ids.len() as f32;
                for i in ids {
                    let f = &mut game.world.facilities[i];
                    f.capacity = (f.capacity + per).max(0.0);
                    f.quality = (f.quality + *quality_delta).clamp(0.0, 1.0);
                    if f.state == FacilityState::Suspended && f.staffing() >= 1.0 {
                        f.state = FacilityState::Operating;
                    }
                }
            }
        }

        Effect::FacilityAccess { kind, access } => {
            for d in districts {
                for f in game.world.facilities.iter_mut() {
                    if f.district == d && f.kind == *kind {
                        f.access = *access;
                    }
                }
            }
        }

        Effect::FacilityHours { kind, night_open } => {
            for d in districts {
                for f in game.world.facilities.iter_mut() {
                    if f.district == d && f.kind == *kind {
                        f.night_open = *night_open;
                    }
                }
            }
        }

        Effect::TrainStaff { role, amount } => {
            game.treasury.real.staff_total[role.index()] += *amount;
            // 新しい指導者は、対象地区の道場へ配置される
            for d in &districts {
                let ids: Vec<usize> = game
                    .world
                    .facilities
                    .iter()
                    .enumerate()
                    .filter(|(_, f)| f.district == *d && staff_kind(*role) == f.kind)
                    .map(|(i, _)| i)
                    .collect();
                if ids.is_empty() {
                    continue;
                }
                let per = *amount / (districts.len() * ids.len()) as f32;
                for i in ids {
                    game.world.facilities[i].staff += per;
                    if game.world.facilities[i].state == FacilityState::Suspended
                        && game.world.facilities[i].staffing() >= 1.0
                    {
                        game.world.facilities[i].state = FacilityState::Operating;
                    }
                }
                game.treasury.real.assign(*role, *amount / districts.len() as f32);
            }
        }

        Effect::DispatchStaff { role, amount, days } => {
            game.treasury.real.staff_total[role.index()] += *amount;
            game.dispatches.push(Dispatch {
                role: *role,
                amount: *amount,
                days_remaining: *days,
                district: districts.first().copied(),
            });
            for d in &districts {
                let ids: Vec<usize> = game
                    .world
                    .facilities
                    .iter()
                    .enumerate()
                    .filter(|(_, f)| f.district == *d && staff_kind(*role) == f.kind)
                    .map(|(i, _)| i)
                    .collect();
                if ids.is_empty() {
                    continue;
                }
                let per = *amount / (districts.len() * ids.len()) as f32;
                for i in ids {
                    game.world.facilities[i].staff += per;
                    if game.world.facilities[i].state == FacilityState::Suspended
                        && game.world.facilities[i].staffing() >= 1.0
                    {
                        game.world.facilities[i].state = FacilityState::Operating;
                    }
                }
            }
        }

        Effect::CreditBase { delta } => {
            game.world.guild.credit_base = (game.world.guild.credit_base + *delta).max(0.5);
        }

        Effect::PublicTeamCapacity { delta } => {
            game.world.public_team_base += *delta;
        }

        Effect::CommunityTeamGrant { strength, access_share } => {
            for d in districts {
                for t in game.world.teams.iter_mut() {
                    if t.district == d && t.kind == TeamKind::Community {
                        // 戦力そのものは住民の能力から日次で導出される。
                        // 助成が動かすのは「何回出られるか」と「誰が使えるか」。
                        t.slots += *strength;
                        t.access_share = (t.access_share + *access_share).clamp(0.0, 1.0);
                    }
                }
            }
        }

        Effect::AgencyCapacity { delta } => {
            game.world.agency_base += *delta;
        }

        Effect::SponsorProgram { strength } => {
            // 民間の支援。制度への信頼と景気に左右され、確実には効かない。
            for d in districts {
                let trust = game.world.district(d).infra.trust;
                let gain = *strength * trust;
                for b in game.world.businesses.iter_mut() {
                    if b.district == d && b.state != BusinessState::Closed {
                        b.scale = (b.scale + gain * 0.1).min(2.0);
                    }
                }
                for f in game.world.facilities.iter_mut() {
                    if f.district == d && f.kind == FacilityKind::Dojo {
                        f.quality = (f.quality + gain * 0.05).clamp(0.0, 1.0);
                    }
                }
            }
        }

        Effect::NationalCamp { cohesion, fatigue } => {
            let nt = game.world.national_team;
            let members = game.world.teams[nt.index()].members.clone();
            {
                let t = &mut game.world.teams[nt.index()];
                t.cohesion = (t.cohesion + *cohesion).clamp(0.3, 1.3);
                t.fatigue = (t.fatigue + *fatigue).clamp(0.0, 1.5);
            }
            // 所属先チームの稼働に影響する（FR-TEAM-03）
            for pid in &members {
                let p = &mut game.world.people[pid.index()];
                p.life.condition.fatigue = (p.life.condition.fatigue + *fatigue).clamp(0.0, 1.5);
                if let Some(club) = p.life.team {
                    let t = &mut game.world.teams[club.index()];
                    t.on_national_duty += 1.0;
                }
            }
        }

        Effect::Audit => {
            game.world.guild.audited = true;
        }

        Effect::MedicalAgreement { capacity } => {
            // 診療能力の範囲内で配分する。医療人員の余力を超えては効かない。
            let avail = game.treasury.real.available(StaffRole::Medic);
            let effective = capacity.min(avail.max(0.0));
            for d in districts {
                game.world
                    .district_mut(d)
                    .infra
                    .add(super::world::InfraField::Medical, effective * 0.05);
            }
        }
    }
}

fn staff_kind(role: StaffRole) -> FacilityKind {
    match role {
        StaffRole::Coach => FacilityKind::Dojo,
        StaffRole::Medic => FacilityKind::Hospital,
        StaffRole::Nursery => FacilityKind::Nursery,
        StaffRole::Official => FacilityKind::PaymentVenue,
        StaffRole::Builder => FacilityKind::Housing,
    }
}

// ───────────────────────────── 推奨枠 ─────────────────────────────

/// 現状の問題に関係するカードを推奨する（FR-POL-05）。
/// 進行中の問題（Issue）に紐づく対応策を優先し、足りない分は指標の悪い分野から補う。
pub fn recommended(game: &Game, limit: usize) -> Vec<(String, Target, String)> {
    let mut out: Vec<(String, Target, String)> = Vec::new();
    for issue in game.issues.iter().filter(|i| i.closed.is_none()) {
        let Some(def) = game.defs.event(&issue.event_id) else { continue };
        for remedy in &def.remedies {
            let Some(p) = game.defs.policy(remedy) else { continue };
            let target = match p.target {
                TargetScope::District => match issue.target.district() {
                    Some(d) => Target::District(d),
                    None => continue,
                },
                _ => Target::Nation,
            };
            if out.iter().any(|(id, t, _)| id == remedy && *t == target) {
                continue;
            }
            if !check(game, p, target).is_empty() {
                continue;
            }
            out.push((remedy.clone(), target, issue.event_id.clone()));
            if out.len() >= limit {
                return out;
            }
        }
    }
    // 問題が出ていないときは、将来への投資を並べる
    for p in &game.defs.policies {
        if out.len() >= limit {
            break;
        }
        let target = match p.target {
            TargetScope::District => match game.world.owned_districts(game.home).next() {
                Some((d, _)) => Target::District(d),
                None => continue,
            },
            _ => Target::Nation,
        };
        if out.iter().any(|(id, _, _)| *id == p.id) {
            continue;
        }
        if check(game, p, target).is_empty() {
            out.push((p.id.clone(), target, String::new()));
        }
    }
    out
}

/// 年度の継続費用を再計算する（FR-BUD-02）。
pub fn committed_upkeep(game: &Game) -> f32 {
    let projects: f32 = game.projects.iter().filter(|p| p.is_done()).map(|p| p.upkeep).sum();
    let facilities: f32 = game
        .world
        .facilities
        .iter()
        .filter(|f| f.state != FacilityState::Closed && f.district_owned(game))
        .map(|f| f.upkeep)
        .sum();
    let teams: f32 = game
        .world
        .teams
        .iter()
        .filter(|t| matches!(t.kind, TeamKind::Public))
        .map(|t| t.upkeep)
        .sum();
    projects + facilities + teams
}

impl super::world::Facility {
    fn district_owned(&self, game: &Game) -> bool {
        game.world.district(self.district).owner == game.home
    }
}

/// 利用権が制限されている施設の一覧（FR-TOWN-04 の可視化）。
pub fn restricted_facilities(game: &Game) -> Vec<FacilityId> {
    game.world
        .facilities
        .iter()
        .enumerate()
        .filter(|(_, f)| f.access != AccessRule::Public && f.is_running())
        .map(|(i, _)| FacilityId::from_index(i))
        .collect()
}
