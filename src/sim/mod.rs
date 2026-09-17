//! シミュレーション本体（bevy 非依存: design.md §1.1）
//!
//! 逐次・決定論・少数エンティティ。`app` 層はここを**読むだけ**で、
//! 書き換えるのは [`Game::step_day`] とプレイヤー操作を表す少数のコマンド関数のみ。

pub mod budget;
pub mod calendar;
pub mod condition;
pub mod context;
pub mod cup;
pub mod defs;
pub mod economy;
pub mod environment;
pub mod events;
pub mod ids;
pub mod news;
pub mod policy;
pub mod rng;
pub mod save;
pub mod stats;
pub mod teams;
pub mod time_budget;
pub mod training;
pub mod world;

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use budget::{BudgetBriefing, Treasury};
use calendar::{Date, StopReason};
use cup::{CupPreview, CupResult, Nation};
use defs::{BudgetField, Defs};
use economy::accounts::Ledger;
use events::Issue;
use ids::*;
use news::NewsLog;
use policy::{Dispatch, Project, Shortfall};
use rng::RngSet;
use stats::StatsHistory;
use world::*;

pub const GAME_VERSION: u32 = 1;

/// 最上位の世界状態（design.md §4.1）。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Game {
    pub version: u32,
    pub date: Date,
    /// 残りの進行日数。年次イベントで停止しても保持する（FR-TIME-06）。
    pub pending_days: u16,
    pub rng: RngSet,
    pub world: World,
    pub treasury: Treasury,
    pub projects: Vec<Project>,
    pub dispatches: Vec<Dispatch>,
    pub issues: Vec<Issue>,
    /// (event_id, target, この通算日までは再発生させない)
    pub event_cooldowns: Vec<(String, Target, u32)>,
    pub news: NewsLog,
    pub stats: StatsHistory,
    pub nations: Vec<Nation>,
    pub home: NationId,
    pub ledger: Ledger,
    /// プレイヤーが差し替えた代表登録
    pub pending_squad: Vec<PersonId>,
    /// 大会前に確定した提示内容。結果処理でも同じ値を使う（AC-11）。
    pub cup_preview: Option<CupPreview>,
    pub last_cup: Option<CupResult>,
    /// 直近の政策を開始した日（その期間の主要記事を残すため: FR-NEWS-06）
    pub advance_start: Date,
    /// 直近の停止理由
    pub stop: Option<StopReason>,
    /// 外部データ。セーブに含めない（design.md §4.1）
    #[serde(skip)]
    pub defs: Arc<Defs>,
}

impl Game {
    /// 初期シナリオから世界を作る。
    pub fn new(seed: u64, defs: Arc<Defs>) -> Self {
        let mut rngset = RngSet::new(seed);
        let (world, nations, treasury) = build_world(&defs, &mut rngset);
        let mut game = Game {
            version: GAME_VERSION,
            date: Date::new(1, 1),
            pending_days: 0,
            rng: rngset,
            world,
            treasury,
            projects: Vec::new(),
            dispatches: Vec::new(),
            issues: Vec::new(),
            event_cooldowns: Vec::new(),
            news: NewsLog::default(),
            stats: StatsHistory::default(),
            nations,
            home: NationId(0),
            ledger: Ledger::default(),
            pending_squad: Vec::new(),
            cup_preview: None,
            last_cup: None,
            advance_start: Date::new(1, 1),
            stop: None,
            defs,
        };
        game.treasury.committed_upkeep = policy::committed_upkeep(&game);
        game.treasury.revenue_estimate = game.defs.scenario.initial_allocations.iter().sum::<f32>()
            + game.treasury.committed_upkeep;
        // 初日の集計を1件入れておく（UI と回帰テストの基準点）
        let today = stats::aggregate(&game);
        game.stats.push(today);
        game
    }

    // ───────────────────────── 日次パイプライン ─────────────────────────

    /// 1日進める。FR-SIM-01 の順序をそのまま関数列にしたもの。
    /// **この20行が最も重要な設計物**であり、呼び出し順が仕様そのものである。
    pub fn step_day(&mut self) -> Option<StopReason> {
        self.date = self.date.next();
        if self.date.doy == 1 {
            training::age_everyone(self);
        }
        economy::open_books(self);

        // ① 政策・契約・工事・育成事業を進める
        policy::advance_projects(self);

        // ② 生産、収支、仕事、生活、練習、回復を更新する
        let env = environment::update(self);
        economy::refill_capacity(self);
        teams::refresh(self);
        cup::apply_national_duty(self);
        time_budget::allocate(self);
        economy::produce(self, env);
        economy::settle_daily(self);
        training::run(self);
        condition::recover(self);

        if self.date.is_weekend() {
            economy::weekly_settlement(self);
        }

        // ③④ イベント
        let fired = events::roll(self);
        events::apply(self, &fired);
        events::resolve(self);
        training::lifecycle(self);

        economy::close_books(self);

        // ⑤ 集計
        let today = stats::aggregate(self);
        self.treasury.revenue_accrued += today.gdp * self.defs.balance.budget.revenue_rate;
        self.stats.push(today.clone());

        // ⑥ ニュース
        news::generate(self, &fired, &today);

        debug_assert!(
            economy::accounts::identities_hold(&self.ledger).is_ok(),
            "会計の恒等式が破れた: {:?} / {:?}",
            self.date,
            economy::accounts::identities_hold(&self.ledger)
        );

        self.stop = self.calendar_stop_reason();
        self.stop
    }

    fn calendar_stop_reason(&self) -> Option<StopReason> {
        self.defs.balance.calendar.stop_reason(self.date)
    }

    /// 残日数を1日消化する。年次イベントに到達した日は `Some` を返して停止する（FR-TIME-05）。
    pub fn advance_one(&mut self) -> Option<StopReason> {
        if self.pending_days == 0 {
            return None;
        }
        let stop = self.step_day();
        self.pending_days -= 1;
        stop
    }

    /// 残日数を最後まで、または年次イベントに当たるまで進める。
    pub fn advance_all(&mut self) -> Option<StopReason> {
        while self.pending_days > 0 {
            if let Some(stop) = self.advance_one() {
                return Some(stop);
            }
        }
        None
    }

    // ───────────────────────── プレイヤー操作 ─────────────────────────

    /// 政策カードを実行する（時間が進む唯一の入口: FR-TIME-02）。
    pub fn execute_policy(
        &mut self,
        policy_id: &str,
        target: Target,
    ) -> Result<ProjectId, Vec<Shortfall>> {
        self.advance_start = self.date;
        policy::execute(self, policy_id, target)
    }

    /// 情報の閲覧は日数を消費しない（FR-TIME-03）。参照系はすべて `&self`。
    pub fn recommended_policies(&self, limit: usize) -> Vec<(String, Target, String)> {
        policy::recommended(self, limit)
    }

    /// 直近の進行期間に残す主要記事（FR-NEWS-06）。
    pub fn period_highlights(&self) -> Vec<&news::Article> {
        self.news.highlights(self.advance_start, self.date, 5)
    }

    // ───────────────────────── 年次イベント ─────────────────────────

    /// 予算編成の提示内容（FR-BUD-02）。
    pub fn budget_briefing(&self) -> BudgetBriefing {
        let upkeep = policy::committed_upkeep(self);
        // 次年度の収入見込みは、直近の実績から推計する
        let recent: f32 = {
            let n = self.stats.days.len();
            let take = n.min(90);
            if take == 0 {
                0.0
            } else {
                self.stats.days[n - take..].iter().map(|d| d.gdp).sum::<f32>() / take as f32
            }
        };
        let revenue = recent * self.defs.balance.budget.revenue_rate * 360.0;
        BudgetBriefing {
            year: self.date.year + 1,
            revenue_estimate: revenue,
            committed_upkeep: upkeep,
            carried_deficit: self.treasury.carried_deficit,
            last_revenue: self.treasury.revenue_accrued,
            last_spent: self.treasury.total_spent(),
            resources: self.treasury.real,
            previous: self.treasury.allocations,
        }
    }

    /// 配分を確定する。収入を超える配分も許可する（FR-BUD-06）。
    pub fn apply_budget(&mut self, allocations: [f32; 8]) {
        let brief = self.budget_briefing();
        // 前年度の精算: 収入を超えた分は翌年度へ持ち越す
        let overspend = (self.treasury.total_spent() + self.treasury.upkeep_paid
            - self.treasury.revenue_accrued)
            .max(0.0);
        self.treasury.carried_deficit = overspend * self.defs.balance.budget.deficit_carry;

        self.treasury.allocations = allocations;
        self.treasury.spent = [0.0; 8];
        self.treasury.committed_upkeep = brief.committed_upkeep;
        self.treasury.revenue_estimate = brief.revenue_estimate;
        self.treasury.revenue_accrued = 0.0;
        // 継続費用は年度の頭に確保する（枠から先に引く）
        self.treasury.upkeep_paid = brief.committed_upkeep;

        let defs = Arc::clone(&self.defs);
        let b = news::Builder { text: &defs.text };
        let total = format!("{:.0}", allocations.iter().sum::<f32>());
        let mut a = b.article(
            self.date,
            news::ArticleKind::Budget,
            "budget.enacted",
            &[("total", &total), ("year", &brief.year.to_string())],
            8,
        );
        a.pinned = true;
        self.news.push(a);
    }

    /// 大会前の確定処理。ここで返した値が UI に渡り、結果処理でも同じものを使う。
    pub fn cup_prepare(&mut self) -> CupPreview {
        let p = cup::prepare(self);
        self.cup_preview = Some(p.clone());
        p
    }

    /// 代表の差し替え（FR-CUP-02）。
    pub fn set_squad(&mut self, squad: Vec<PersonId>) {
        self.pending_squad = squad;
    }

    /// 大会を実行する。
    pub fn cup_run(&mut self) -> CupResult {
        let preview = match self.cup_preview.clone() {
            Some(p) => p,
            None => self.cup_prepare(),
        };
        let r = cup::run(self, &preview);
        self.last_cup = Some(r.clone());
        self.cup_preview = None;
        r
    }

    // ───────────────────────── 履歴とハッシュ ─────────────────────────

    /// 人物・施設・地区に履歴を残す（FR-SIM-10 / FR-POP-07）。
    pub fn history_note(&mut self, target: Target, date: Date, key: &str, detail: &str) {
        let entry = HistoryEntry { date, text_key: key.to_string(), detail: detail.to_string() };
        match target {
            Target::District(d) => self.world.districts[d.index()].history.push(entry),
            Target::Cohort(c) => self.world.districts[c.district.index()].history.push(entry),
            Target::Facility(f) => self.world.facilities[f.index()].history.push(entry),
            Target::Person(p) => self.world.people[p.index()].history.push(entry),
            Target::Business(b) => self.world.businesses[b.index()].history.push(entry),
            Target::Nation => {
                if let Some(d) = self.world.districts.iter_mut().find(|d| d.is_home) {
                    d.history.push(entry);
                }
            }
        }
    }

    /// 全状態のハッシュ。決定論テスト（AC-06）とセーブ往復テスト（AC-10）で使う。
    pub fn state_hash(&self) -> u64 {
        let s = ron::ser::to_string(self).unwrap_or_default();
        fnv1a(s.as_bytes())
    }

    /// ロード後の検証。定義が消えている事業は中止扱いにする（design.md §4.1）。
    pub fn revalidate_after_load(&mut self) {
        let defs = Arc::clone(&self.defs);
        let mut dropped = Vec::new();
        for p in self.projects.iter_mut() {
            if defs.policy(&p.policy).is_none() && !p.is_done() {
                p.state = policy::ProjectState::Stalled;
                dropped.push(p.policy.clone());
            }
        }
        self.issues.retain(|i| defs.event(&i.event_id).is_some());
    }

    /// 予算の残額（FR-STAT-01）。
    pub fn budget_remaining(&self, field: BudgetField) -> f32 {
        self.treasury.remaining(field)
    }
}

pub fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x1000_0000_01b3);
    }
    h
}

// ───────────────────────────── 初期世界の構築 ─────────────────────────────

fn build_world(defs: &Defs, rngset: &mut RngSet) -> (World, Vec<Nation>, Treasury) {
    let sc = &defs.scenario;
    let home = NationId(0);

    let mut nations = vec![Nation {
        id: home,
        name: sc.nation_name.clone(),
        strength: 50.0,
        growth: 1.0,
        is_home: true,
    }];
    for (i, r) in sc.rivals.iter().enumerate() {
        nations.push(Nation {
            id: NationId(i as u16 + 1),
            name: r.name.clone(),
            strength: r.strength,
            growth: r.growth,
            is_home: false,
        });
    }

    let mut world = World {
        districts: Vec::new(),
        facilities: Vec::new(),
        teams: Vec::new(),
        people: Vec::new(),
        businesses: Vec::new(),
        guild: CardGuild {
            credit_base: 14.0,
            guaranteed: 0.0,
            reserve: 0.0,
            coverage: 1.0,
            audited: false,
            outstanding: 0.0,
        },
        environment: Environment::default(),
        stock_necessity: 0.0,
        stock_service: 0.0,
        stock_buildwork: 0.0,
        public_team_capacity: 0.0,
        agency_capacity: 0.0,
        public_team_base: 8.0,
        agency_base: 4.0,
        national_team: TeamId(0),
    };

    // ── 自国の地区 ──
    for seed in &sc.districts {
        let id = DistrictId::from_index(world.districts.len());
        let mut district = District {
            name: seed.name.clone(),
            owner: home,
            cohorts: Vec::new(),
            infra: seed.infra,
            distance_to: seed.distance_to.clone(),
            is_home: seed.is_home,
            history: Vec::new(),
        };

        for cs in &seed.cohorts {
            let mut life = Life::new(cs.occupation);
            life.ability = Ability { value: cs.ability, spread: cs.spread };
            life.motivation = cs.motivation;
            life.talent = cs.talent;
            life.household = Household {
                size: cs.household_size,
                children: cs.children,
                dependents: cs.dependents,
            };
            district.cohorts.push(Cohort { age_band: cs.age_band, headcount: cs.headcount, life });
        }
        world.districts.push(district);

        for fs in &seed.facilities {
            world.facilities.push(Facility {
                name: format!("{}{}", seed.name, fs.name),
                kind: fs.kind,
                district: id,
                capacity: fs.capacity,
                staff_required: fs.staff_required,
                staff: fs.staff,
                upkeep: fs.upkeep,
                state: FacilityState::Operating,
                access: fs.access,
                night_open: fs.night_open,
                quality: fs.quality,
                enrolled: 0.0,
                history: Vec::new(),
            });
        }

        for ts in &seed.teams {
            world.teams.push(Team {
                name: format!("{}{}", seed.name, ts.name),
                kind: ts.kind,
                district: id,
                strength: ts.strength,
                cohesion: ts.cohesion,
                roster: ts.roster,
                fatigue: 0.15,
                slots: ts.slots,
                slots_used: 0.0,
                upkeep: ts.upkeep,
                members: Vec::new(),
                access_share: ts.access_share,
                on_national_duty: 0.0,
            });
        }

        for bs in &seed.businesses {
            let team_id = TeamId::from_index(world.teams.len());
            world.teams.push(Team {
                name: format!("{}{}", bs.name, defs.text.get("team.resident")),
                kind: TeamKind::ShopResident,
                district: id,
                strength: bs.team_strength,
                cohesion: 0.9,
                roster: bs.team_roster,
                fatigue: 0.1,
                slots: 8.0,
                slots_used: 0.0,
                upkeep: 6.0,
                members: Vec::new(),
                access_share: 0.0,
                on_national_duty: 0.0,
            });
            world.businesses.push(Business {
                name: format!("{}{}", seed.name, bs.name),
                kind: bs.kind,
                district: id,
                team: team_id,
                daily_balance: 0.0,
                cum_balance: 0.0,
                scale: bs.scale,
                state: BusinessState::Operating,
                deficit_days: 0,
                history: Vec::new(),
            });
        }
    }

    // ── 他国の地区（大会で賭けられる領地） ──
    for n in nations.iter().filter(|n| !n.is_home) {
        let id = DistrictId::from_index(world.districts.len());
        let mut district = District {
            name: format!("{}{}", n.name, defs.text.get("district.rival_suffix")),
            owner: n.id,
            cohorts: Vec::new(),
            infra: Infra::default(),
            distance_to: Vec::new(),
            is_home: false,
            history: Vec::new(),
        };
        for (band, occ, head) in [
            (AgeBand::Youth, Occupation::Farmer, 180u32),
            (AgeBand::Prime, Occupation::Manufacturer, 220u32),
            (AgeBand::Middle, Occupation::Clerk, 160u32),
        ] {
            let mut life = Life::new(occ);
            life.ability = Ability { value: 24.0, spread: 7.0 };
            district.cohorts.push(Cohort { age_band: band, headcount: head, life });
        }
        world.districts.push(district);
        world.teams.push(Team {
            name: format!("{}{}", district_name(&world, id), defs.text.get("team.community")),
            kind: TeamKind::Community,
            district: id,
            strength: 24.0,
            cohesion: 0.85,
            roster: 7.0,
            fatigue: 0.1,
            slots: 3.0,
            slots_used: 0.0,
            upkeep: 4.0,
            members: Vec::new(),
            access_share: 0.4,
            on_national_duty: 0.0,
        });
    }

    // 距離行列を地区数に合わせて埋める
    let n = world.districts.len();
    for d in world.districts.iter_mut() {
        while d.distance_to.len() < n {
            d.distance_to.push(90);
        }
        d.distance_to.truncate(n);
    }

    // ── 代表チーム ──
    let home_district = world
        .districts
        .iter()
        .position(|d| d.is_home)
        .map(DistrictId::from_index)
        .unwrap_or(DistrictId(0));
    world.national_team = TeamId::from_index(world.teams.len());
    world.teams.push(Team {
        name: defs.text.get("team.national").to_string(),
        kind: TeamKind::Public,
        district: home_district,
        strength: 40.0,
        cohesion: 0.9,
        roster: 7.0,
        fatigue: 0.1,
        slots: 2.0,
        slots_used: 0.0,
        upkeep: 40.0,
        members: Vec::new(),
        access_share: 0.0,
        on_national_duty: 0.0,
    });

    // ── 追跡人物 ──
    for ps in &sc.people {
        let home_d = DistrictId::from_index(ps.district.min(sc.districts.len().saturating_sub(1)));
        let mut life = Life::new(ps.occupation);
        life.ability = Ability { value: ps.ability, spread: 0.0 };
        life.talent = ps.talent;
        life.motivation = ps.motivation;
        life.household =
            Household { size: ps.household_size, children: ps.children, dependents: ps.dependents };
        // 住民共同チームがあれば所属する
        life.team = world
            .teams
            .iter()
            .position(|t| t.district == home_d && t.kind == TeamKind::Community)
            .map(TeamId::from_index);
        world.people.push(Person {
            name: ps.name.clone(),
            home: home_d,
            age: ps.age,
            life,
            injury: None,
            status: PersonStatus::Active,
            caps: 0,
            selected_for_cup: false,
            milestone: 0,
            reported_status: PersonStatus::Active,
            history: if ps.note_key.is_empty() {
                Vec::new()
            } else {
                vec![HistoryEntry {
                    date: Date::new(1, 1),
                    text_key: ps.note_key.clone(),
                    detail: String::new(),
                }]
            },
        });
    }

    // コホートにも地区の共同チームを紐づける（利用権は team.access_share が決める）
    for di in 0..world.districts.len() {
        let team = world
            .teams
            .iter()
            .position(|t| t.district.index() == di && t.kind == TeamKind::Community)
            .map(TeamId::from_index);
        for c in world.districts[di].cohorts.iter_mut() {
            c.life.team = team;
        }
    }

    // ── 初期在庫と予算 ──
    world.stock_necessity = world.population() as f32 * 3.0;

    let mut treasury = Treasury { allocations: sc.initial_allocations, ..Default::default() };
    treasury.real.staff_total = staff_pools(&world, defs);
    treasury.real.staff_assigned = assigned_staff(&world);
    treasury.real.materials = 200.0;
    treasury.real.public_team_capacity = world.public_team_base;
    treasury.carried_deficit = 0.0;
    treasury.spent = [0.0; 8];
    let _ = sc.initial_reserve;
    let _ = rngset;

    (world, nations, treasury)
}

fn district_name(world: &World, id: DistrictId) -> String {
    world.districts[id.index()].name.clone()
}

/// 施設に既に配置されている人員。プールとの差が「動かせる余力」になる。
fn assigned_staff(world: &World) -> [f32; 5] {
    let mut a = [0.0f32; 5];
    for f in &world.facilities {
        let i = match f.kind {
            FacilityKind::Dojo => 0,
            FacilityKind::Hospital => 1,
            FacilityKind::Nursery => 2,
            FacilityKind::PaymentVenue => 3,
            FacilityKind::Housing => 4,
            _ => continue,
        };
        a[i] += f.staff;
    }
    a
}

/// 職業構成から国の人員プールを求める（実資源: FR-BUD-05）。
fn staff_pools(world: &World, defs: &Defs) -> [f32; 5] {
    let mut pools = [0.0f32; 5];
    for d in &world.districts {
        for c in &d.cohorts {
            let n = c.headcount as f32;
            let occ = defs.balance.occupation(c.life.occupation);
            match c.life.occupation {
                Occupation::Coach => pools[0] += n * occ.coach_supply,
                Occupation::Medic => pools[1] += n * occ.medical_supply,
                Occupation::Caregiver => pools[2] += n * 0.02,
                Occupation::Official => pools[3] += n * 0.05,
                Occupation::Builder => pools[4] += n * 0.05,
                _ => {}
            }
        }
    }
    pools
}
