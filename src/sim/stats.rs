//! 国家ステータスの集計（design.md §13.2 / FR-STAT-*）
//!
//! **国家ステータスへ乱数を直接加算しない**（FR-SIM-03）。すべて現場の状態の集計として出す。
//! 集計時に残す内訳レコード（`Breakdown`）が、UI の③「なぜ」の唯一の出所（NFR-09 / FR-UI-02）。

use serde::{Deserialize, Serialize};

use super::calendar::Date;
use super::defs::MetricKey;
use super::economy;
use super::ids::{DistrictId, Subject};
use super::world::{BusinessState, FacilityKind, FacilityState, PersonStatus, World};
use super::{Game, teams, training};

/// 指標がその値になった理由。UI が独自に再計算しない。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Breakdown {
    /// assets/text/ja.ron のキー
    pub text_key: String,
    pub numbers: Vec<f32>,
    pub subject: Option<Subject>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DistrictStats {
    pub district: u16,
    pub population: u32,
    pub mean_ability: f32,
    pub participation: f32,
    pub life_slack: f32,
    pub fatigue: f32,
    pub necessity_fill: f32,
    pub receipt_value: f32,
    pub obligation: f32,
    pub dojo_capacity: f32,
    pub dojo_enrolled: f32,
    pub coach_seats: f32,
    pub team_access: f32,
}

/// 1日分の国家ステータス。常時表示（FR-STAT-01）と詳細6分野（FR-STAT-02）を両方含む。
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DailyStats {
    pub date: Date,
    pub days_to_cup: u16,

    // ── 常時表示 ──
    pub population: u32,
    pub gdp: f32,
    pub gdp_per_capita: f32,
    pub budget_available: f32,
    pub balance_forecast: f32,
    pub mean_ability: f32,
    pub national_strength: f32,
    pub national_cohesion: f32,

    // ── 経済 ──
    pub obligation_per_household: f32,
    pub genkaba_share: f32,
    pub proxy_share: f32,
    pub businesses_operating: u16,
    pub businesses_closed: u16,
    pub guild_coverage: f32,

    // ── 生活 ──
    pub necessity_fill: f32,
    pub life_slack: f32,
    pub free_minutes: f32,
    pub practice_minutes: f32,
    pub commute_minutes: f32,
    pub care_minutes: f32,
    pub shopping_minutes: f32,
    pub work_minutes: f32,
    pub health: f32,
    pub nutrition: f32,
    pub housing: f32,
    pub fatigue: f32,
    pub credit_limit: f32,
    pub trust: f32,
    pub scouting: f32,
    pub medical_capacity: f32,
    pub nursery_capacity: f32,
    pub guild_visibility: f32,

    // ── 育成 ──
    pub participation: f32,
    pub baseline_rate: f32,
    pub coach_seats: f32,
    pub dojo_capacity: f32,
    pub dojo_enrolled: f32,

    // ── 競技 ──
    pub squad_strength: f32,
    pub squad_depth: f32,
    pub squad_fatigue: f32,
    pub injured: u16,
    pub mean_age: f32,

    // ── 分配 ──
    pub receipt_gini: f32,
    pub access_concentration: f32,
    pub district_gap: f32,
    pub receipt_value: f32,

    // ── 財政 ──
    pub committed_upkeep: f32,
    pub reserve: f32,
    pub spent_this_year: f32,

    pub by_district: Vec<DistrictStats>,
    pub breakdown: Vec<Breakdown>,
}

impl DailyStats {
    /// 回帰テスト（AC-03）と UI が参照する単一の入口。
    pub fn metric(&self, key: MetricKey) -> f32 {
        match key {
            MetricKey::PracticeMinutes => self.practice_minutes,
            MetricKey::Participation => self.participation,
            MetricKey::AverageAbility => self.mean_ability,
            MetricKey::LifeSlack => self.life_slack,
            MetricKey::Gdp => self.gdp,
            MetricKey::Fatigue => self.fatigue,
            MetricKey::CoachCapacity => self.coach_seats,
            MetricKey::MedicalCapacity => self.medical_capacity,
            MetricKey::NurseryCapacity => self.nursery_capacity,
            MetricKey::DojoCapacity => self.dojo_capacity,
            MetricKey::CreditLimit => self.credit_limit,
            MetricKey::Receipt => self.receipt_value,
            MetricKey::NationalStrength => self.national_strength,
            MetricKey::NationalCohesion => self.national_cohesion,
            MetricKey::GuildVisibility => self.guild_visibility,
            MetricKey::Scouting => self.scouting,
            MetricKey::Trust => self.trust,
            MetricKey::CommuteMinutes => self.commute_minutes,
            MetricKey::CareMinutes => self.care_minutes,
            MetricKey::ShoppingMinutes => self.shopping_minutes,
            MetricKey::WorkMinutes => self.work_minutes,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct StatsHistory {
    pub days: Vec<DailyStats>,
}

impl StatsHistory {
    pub fn push(&mut self, s: DailyStats) {
        self.days.push(s);
    }
    pub fn today(&self) -> Option<&DailyStats> {
        self.days.last()
    }
    /// 推移グラフ（FR-STAT-03）。
    pub fn series(&self, key: MetricKey) -> Vec<(u32, f32)> {
        self.days.iter().map(|d| (d.date.absolute(), d.metric(key))).collect()
    }
    /// n 日前との差。政策の効果を読むのに使う。
    pub fn delta(&self, key: MetricKey, days_back: usize) -> f32 {
        let n = self.days.len();
        if n == 0 {
            return 0.0;
        }
        let now = self.days[n - 1].metric(key);
        let then = self.days[n.saturating_sub(days_back + 1)].metric(key);
        now - then
    }
}

/// ⑤ 国家ステータスを集計する。
pub fn aggregate(game: &Game) -> DailyStats {
    let w = &game.world;
    let defs = &game.defs;
    let mut s = DailyStats { date: game.date, ..Default::default() };

    s.days_to_cup = game.date.days_until(defs.balance.calendar.cup_doy);

    let mut pop = 0.0f32;
    let mut acc = Acc::default();
    let mut by_district = Vec::new();

    for (id, d) in w.owned_districts(game.home) {
        let mut ds = DistrictStats { district: id.0, ..Default::default() };
        let mut dpop = 0.0f32;
        let mut dacc = Acc::default();

        for c in &d.cohorts {
            let n = c.headcount as f32;
            dpop += n;
            dacc.add(n, &c.life);
        }
        for p in w.people.iter().filter(|p| p.home == id && p.status != PersonStatus::Emigrated) {
            dacc.add(1.0, &p.life);
        }

        ds.population = dpop as u32;
        ds.mean_ability = dacc.mean(dacc.ability);
        ds.participation = dacc.mean(dacc.participation);
        ds.life_slack = dacc.mean(dacc.life_slack);
        ds.fatigue = dacc.mean(dacc.fatigue);
        ds.necessity_fill = dacc.mean(dacc.fill) / defs.balance.economy.need_necessity.max(0.01);
        ds.receipt_value = dacc.receipt_value;
        ds.obligation = economy::obligation_by_district(w, id);
        ds.dojo_capacity = w.capacity_of(id, FacilityKind::Dojo);
        ds.dojo_enrolled = w.facilities_in(id, FacilityKind::Dojo).map(|(_, f)| f.enrolled).sum();
        ds.coach_seats = w
            .facilities_in(id, FacilityKind::Dojo)
            .filter(|(_, f)| f.is_running())
            .map(|(_, f)| f.staff * defs.balance.training.coach_ratio)
            .sum();
        ds.team_access = teams::team_access_in(w, id);

        pop += dpop;
        acc.merge(&dacc);
        by_district.push(ds);
    }

    s.population = pop as u32;
    s.gdp = game.ledger.gdp;
    s.gdp_per_capita = if pop > 0.0 { s.gdp / pop } else { 0.0 };
    s.budget_available = (game.treasury.total_allocated() - game.treasury.total_spent()).max(0.0);
    s.balance_forecast = game.treasury.balance_forecast();
    s.mean_ability = training::mean_ability(w, game.home);

    let nt = teams::national(game);
    s.national_strength = nt.effective_strength();
    s.national_cohesion = nt.cohesion;
    s.squad_strength = nt.strength;
    s.squad_fatigue = nt.fatigue;
    s.squad_depth = nt.roster;

    s.obligation_per_household = acc.mean(acc.obligation);
    s.genkaba_share = economy::genkaba_share(w);
    s.proxy_share = economy::proxy_share(w);
    s.businesses_operating =
        w.businesses.iter().filter(|b| b.state != BusinessState::Closed).count() as u16;
    s.businesses_closed =
        w.businesses.iter().filter(|b| b.state == BusinessState::Closed).count() as u16;
    s.guild_coverage = w.guild.coverage;

    s.necessity_fill = acc.mean(acc.fill) / defs.balance.economy.need_necessity.max(0.01);
    s.life_slack = acc.mean(acc.life_slack);
    s.free_minutes = acc.mean(acc.leisure);
    s.practice_minutes = acc.mean(acc.practice);
    s.commute_minutes = acc.mean(acc.commute);
    s.care_minutes = acc.mean(acc.care);
    s.shopping_minutes = acc.mean(acc.shopping);
    s.work_minutes = acc.mean(acc.work);
    s.health = acc.mean(acc.health);
    s.nutrition = acc.mean(acc.nutrition);
    s.housing = mean_infra(w, game.home, |i| i.housing);
    s.fatigue = acc.mean(acc.fatigue);
    s.credit_limit = acc.mean(acc.credit_limit);
    s.trust = mean_infra(w, game.home, |i| i.trust);
    s.scouting = mean_infra(w, game.home, |i| i.scouting);
    s.nursery_capacity =
        w.owned_districts(game.home).map(|(id, _)| w.capacity_of(id, FacilityKind::Nursery)).sum();
    s.medical_capacity = w
        .owned_districts(game.home)
        .map(|(id, _)| w.capacity_of(id, FacilityKind::Hospital))
        .sum::<f32>()
        + mean_infra(w, game.home, |i| i.medical) * 100.0;
    s.guild_visibility = if w.guild.audited { 1.0 } else { 0.0 };

    s.participation = acc.mean(acc.participation);
    s.baseline_rate = training::baseline_rate(w, game.home, 30.0);
    s.coach_seats = by_district.iter().map(|d| d.coach_seats).sum();
    s.dojo_capacity = by_district.iter().map(|d| d.dojo_capacity).sum();
    s.dojo_enrolled = by_district.iter().map(|d| d.dojo_enrolled).sum();

    s.injured = w.people.iter().filter(|p| p.injury.is_some()).count() as u16;
    s.mean_age = {
        let active: Vec<&super::world::Person> =
            w.people.iter().filter(|p| p.status == PersonStatus::Active).collect();
        if active.is_empty() {
            0.0
        } else {
            active.iter().map(|p| p.age as f32).sum::<f32>() / active.len() as f32
        }
    };

    s.receipt_gini = economy::receipt_concentration(w);
    s.access_concentration = teams::access_concentration(w, game.home);
    s.receipt_value = game.ledger.receipt_value;
    s.district_gap = {
        if by_district.len() < 2 {
            0.0
        } else {
            let hi = by_district.iter().map(|d| d.mean_ability).fold(f32::MIN, f32::max);
            let lo = by_district.iter().map(|d| d.mean_ability).fold(f32::MAX, f32::min);
            if hi > 0.0 { (hi - lo) / hi } else { 0.0 }
        }
    };

    s.committed_upkeep = game.treasury.committed_upkeep;
    s.reserve = game.treasury.remaining(super::defs::BudgetField::Reserve);
    s.spent_this_year = game.treasury.total_spent();

    s.breakdown = build_breakdown(game, &by_district);
    s.by_district = by_district;
    s
}

#[derive(Default)]
struct Acc {
    w: f32,
    ability: f32,
    participation: f32,
    life_slack: f32,
    fatigue: f32,
    health: f32,
    nutrition: f32,
    leisure: f32,
    fill: f32,
    obligation: f32,
    receipt_value: f32,
    practice: f32,
    commute: f32,
    care: f32,
    shopping: f32,
    work: f32,
    credit_limit: f32,
}

impl Acc {
    fn add(&mut self, n: f32, l: &super::world::Life) {
        self.w += n;
        self.ability += n * l.ability.value;
        self.participation += n * l.participation;
        self.life_slack += n * l.life_slack;
        self.fatigue += n * l.condition.fatigue;
        self.health += n * l.condition.health;
        self.nutrition += n * l.condition.nutrition;
        self.leisure += n * (l.time.leisure + l.time.practice);
        self.fill += n * l.receipt.necessity;
        self.obligation += n * l.obligation;
        self.receipt_value += n * (l.receipt.necessity + l.receipt.service);
        self.practice += n * l.time.practice;
        self.commute += n * l.time.commute;
        self.care += n * l.time.care;
        self.shopping += n * l.time.shopping;
        self.work += n * (l.time.work + l.time.extra_labor);
        self.credit_limit += n * l.credit_limit;
    }
    fn merge(&mut self, o: &Acc) {
        self.w += o.w;
        self.ability += o.ability;
        self.participation += o.participation;
        self.life_slack += o.life_slack;
        self.fatigue += o.fatigue;
        self.health += o.health;
        self.nutrition += o.nutrition;
        self.leisure += o.leisure;
        self.fill += o.fill;
        self.obligation += o.obligation;
        self.receipt_value += o.receipt_value;
        self.practice += o.practice;
        self.commute += o.commute;
        self.care += o.care;
        self.shopping += o.shopping;
        self.work += o.work;
        self.credit_limit += o.credit_limit;
    }
    fn mean(&self, v: f32) -> f32 {
        if self.w > 0.0 { v / self.w } else { 0.0 }
    }
}

fn mean_infra(w: &World, home: super::ids::NationId, f: fn(&super::world::Infra) -> f32) -> f32 {
    let mut num = 0.0;
    let mut den = 0.0;
    for (_, d) in w.owned_districts(home) {
        let n = d.population() as f32;
        num += n * f(&d.infra);
        den += n;
    }
    if den > 0.0 { num / den } else { 0.0 }
}

/// 「なぜその値になったか」を残す（NFR-09）。
/// 例: 指導枠 = 指導者3名 × 定員12 = 36 < 在籍44。
fn build_breakdown(game: &Game, ds: &[DistrictStats]) -> Vec<Breakdown> {
    let mut v = Vec::new();
    for d in ds {
        let id = DistrictId(d.district);
        if d.dojo_enrolled > d.coach_seats && d.coach_seats > 0.0 {
            v.push(Breakdown {
                text_key: "because.coach_seats".into(),
                numbers: vec![
                    d.coach_seats / game.defs.balance.training.coach_ratio,
                    game.defs.balance.training.coach_ratio,
                    d.coach_seats,
                    d.dojo_enrolled,
                ],
                subject: Some(Subject::District(id)),
            });
        }
        if d.necessity_fill < 0.95 {
            v.push(Breakdown {
                text_key: "because.necessity_short".into(),
                numbers: vec![d.necessity_fill, d.receipt_value],
                subject: Some(Subject::District(id)),
            });
        }
        if d.life_slack < 0.35 {
            v.push(Breakdown {
                text_key: "because.life_slack".into(),
                numbers: vec![d.life_slack, d.participation],
                subject: Some(Subject::District(id)),
            });
        }
    }
    for (i, f) in game.world.facilities.iter().enumerate() {
        if f.state == FacilityState::Suspended {
            v.push(Breakdown {
                text_key: "because.facility_suspended".into(),
                numbers: vec![f.staff, f.staff_required],
                subject: Some(Subject::Facility(super::ids::FacilityId::from_index(i))),
            });
        }
    }
    v
}
