//! 外部データ定義（NFR-07 / FR-POL-01）
//!
//! カード・施設・イベント・文面・バランス係数はすべてここで型付けし、`assets/` の RON から読む。
//! 再ビルドなしに調整できることが要件なので、コードに数値も日本語も埋めない。
//! ビルド時に埋め込んだ既定データも持ち、`assets/` が見つからない環境（テスト・配布物）でも動く。

use serde::{Deserialize, Serialize};

use super::calendar::CalendarDefs;
use super::world::{AccessRule, AgeBand, FacilityKind, InfraField, Occupation, TeamKind};

// ───────────────────────────── バランス係数 ─────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Balance {
    pub calendar: CalendarDefs,
    pub time: TimeParams,
    pub occupations: Vec<OccupationDef>,
    pub economy: EconomyParams,
    pub training: TrainingParams,
    pub events: EventParams,
    pub cup: CupParams,
    pub budget: BudgetParams,
    pub noise: NoiseParams,
}

impl Default for Balance {
    fn default() -> Self {
        Balance {
            calendar: CalendarDefs::default(),
            time: TimeParams::default(),
            occupations: Vec::new(),
            economy: EconomyParams::default(),
            training: TrainingParams::default(),
            events: EventParams::default(),
            cup: CupParams::default(),
            budget: BudgetParams::default(),
            noise: NoiseParams::default(),
        }
    }
}

impl Balance {
    pub fn occupation(&self, o: Occupation) -> &OccupationDef {
        self.occupations
            .iter()
            .find(|d| d.occupation == o)
            .unwrap_or_else(|| panic!("balance.ron に職業 {o:?} の定義がない"))
    }
}

/// 時間予算のパラメータ（design.md §7）
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct TimeParams {
    pub base_sleep: f32,
    /// 住環境の悪さによる睡眠の減少（最大値、分）
    pub sleep_penalty_max: f32,
    /// 通勤の基準時間（分）と交通整備度による短縮率
    pub commute_base: f32,
    pub commute_transit_relief: f32,
    /// 子ども1人あたりの育児時間（分）と保育供給による短縮率
    pub care_per_child: f32,
    pub care_childcare_relief: f32,
    /// 被扶養者1人あたりの介護時間（分）
    pub care_per_dependent: f32,
    pub care_base: f32,
    /// 決済に要する時間（分）
    pub shopping_card: f32,
    pub shopping_genkaba: f32,
    pub shopping_proxy: f32,
    /// 決済会場の整備による短縮率
    pub shopping_venue_relief: f32,
    /// 週末町内大会に要する時間（分）
    pub weekend_settlement_minutes: f32,
    /// 生活余力の基準（この分数を超える余暇があれば life_slack = 1）
    pub slack_reference: f32,
}

impl Default for TimeParams {
    fn default() -> Self {
        TimeParams {
            base_sleep: 480.0,
            sleep_penalty_max: 90.0,
            commute_base: 90.0,
            commute_transit_relief: 0.55,
            care_per_child: 150.0,
            care_childcare_relief: 0.6,
            care_per_dependent: 90.0,
            care_base: 60.0,
            shopping_card: 20.0,
            shopping_genkaba: 55.0,
            shopping_proxy: 15.0,
            shopping_venue_relief: 0.45,
            weekend_settlement_minutes: 120.0,
            slack_reference: 240.0,
        }
    }
}

/// 生産物の種類。K建ての標準価値は `GoodsValues` で固定する（FR-ECO-06）。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Good {
    /// 生活必需品（食料・日用品）
    Necessity,
    /// サービス（飲食・接客・医療・指導）
    Service,
    /// 建設仕事（事業の進捗に使う。総固定資本形成として GDP に入る）
    BuildWork,
    /// 生産しない職業
    None,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct OccupationDef {
    pub occupation: Occupation,
    /// 所定労働時間（分）
    pub work_minutes: f32,
    /// 1時間あたりの産出量
    pub productivity: f32,
    pub output: Good,
    /// 産出1単位あたりの中間投入（Necessity 単位）。GDP から差し引く。
    pub intermediate: f32,
    /// 指導能力の供給（人時あたり）
    pub coach_supply: f32,
    /// 医療能力の供給（人時あたり）
    pub medical_supply: f32,
    /// 練習への意欲補正
    pub motivation_bias: f32,
}

impl Default for OccupationDef {
    fn default() -> Self {
        OccupationDef {
            occupation: Occupation::Clerk,
            work_minutes: 480.0,
            productivity: 1.0,
            output: Good::Service,
            intermediate: 0.0,
            coach_supply: 0.0,
            medical_supply: 0.0,
            motivation_bias: 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct EconomyParams {
    /// 標準価値（K/単位）。商品ごとに固定し、実質 GDP の評価に使う。
    pub value_necessity: f32,
    pub value_service: f32,
    pub value_buildwork: f32,
    /// 1人1日あたりの必需品需要
    pub need_necessity: f32,
    /// 1人1日あたりのサービス需要（余力に応じて減る）
    pub need_service: f32,
    /// 現カバの負担倍率（勝ち / 負け）。期待値が 1.0 近傍になるよう調整する。
    pub genkaba_win_mult: f32,
    pub genkaba_lose_mult: f32,
    /// 現カバ1回あたりの疲労
    pub genkaba_fatigue: f32,
    /// 利用枠の係数レンジ
    pub credit_team_min: f32,
    pub credit_team_max: f32,
    pub credit_record_min: f32,
    pub credit_record_max: f32,
    /// 清算力 → K の換算レート
    pub settlement_rate: f32,
    /// 週末に清算を求められる割合（これを下回ると未清算扱い）
    pub required_settlement_ratio: f32,
    /// 未清算時の利用枠縮小率
    pub credit_shrink: f32,
    /// 労務による直接清算のレート（分あたり K）
    pub labor_settlement_rate: f32,
    /// 公的・共同体による肩代わりの上限（K/世帯/週）
    pub public_relief_cap: f32,
    /// 事業者の常駐チーム維持費（K/日）
    pub shop_team_cost: f32,
    /// 赤字がこの日数続くと縮小 → 閉店
    pub deficit_days_shrink: u16,
    pub deficit_days_close: u16,
    /// 組合の準備率がこれを下回ると全世帯の利用枠を縮小
    pub guild_coverage_floor: f32,
    /// 分配: 受取の取り合いの強さ（0 なら均等、大きいほど強者に集中）
    pub claim_power_exponent: f32,
    /// 事業者の取扱規模1あたりに引き渡せる量（単位/日）
    pub delivery_per_scale: f32,
    /// 必需品の日次劣化率
    pub spoil_rate: f32,
    /// 事業者の仕入率（受取に対する中間投入の割合）
    pub procurement_ratio: f32,
    /// 在庫が組合の準備に算入される割合
    pub guild_reserve_rate: f32,
    /// 週末町内大会による疲労
    pub weekend_fatigue: f32,
}

impl Default for EconomyParams {
    fn default() -> Self {
        EconomyParams {
            value_necessity: 1.0,
            value_service: 1.5,
            value_buildwork: 2.0,
            need_necessity: 1.0,
            need_service: 0.5,
            genkaba_win_mult: 0.6,
            genkaba_lose_mult: 1.4,
            genkaba_fatigue: 0.02,
            credit_team_min: 0.5,
            credit_team_max: 2.0,
            credit_record_min: 0.6,
            credit_record_max: 1.2,
            settlement_rate: 0.6,
            required_settlement_ratio: 0.8,
            credit_shrink: 0.85,
            labor_settlement_rate: 0.01,
            public_relief_cap: 6.0,
            shop_team_cost: 1.2,
            deficit_days_shrink: 30,
            deficit_days_close: 90,
            guild_coverage_floor: 0.9,
            claim_power_exponent: 0.6,
            delivery_per_scale: 900.0,
            spoil_rate: 0.02,
            procurement_ratio: 0.7,
            guild_reserve_rate: 0.5,
            weekend_fatigue: 0.05,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct TrainingParams {
    /// 成長係数 k
    pub growth_k: f32,
    /// 能力の自然減衰
    pub decay_k: f32,
    /// 加齢による減衰の増加が始まる年齢
    pub decay_age_start: f32,
    pub decay_age_slope: f32,
    /// 練習の疲労係数
    pub fatigue_k: f32,
    /// 練習強度
    pub intensity: f32,
    /// 指導者1人が見られる在籍者数
    pub coach_ratio: f32,
    /// 指導の質の下限（指導者ゼロでも独習はできる）
    pub quality_floor: f32,
    /// 能力の上限
    pub ability_cap: f32,
    /// 経験の蓄積レート
    pub experience_rate: f32,
}

impl Default for TrainingParams {
    fn default() -> Self {
        TrainingParams {
            growth_k: 0.055,
            decay_k: 0.00055,
            decay_age_start: 28.0,
            decay_age_slope: 0.09,
            fatigue_k: 0.022,
            intensity: 1.0,
            coach_ratio: 12.0,
            quality_floor: 0.25,
            ability_cap: 100.0,
            experience_rate: 0.01,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct EventParams {
    /// 回復（睡眠・栄養・医療）の係数
    pub recover_sleep: f32,
    pub recover_housing: f32,
    pub recover_medical: f32,
    /// 負傷からの復帰日数の基準
    pub injury_days_base: f32,
}

impl Default for EventParams {
    fn default() -> Self {
        EventParams {
            recover_sleep: 0.00016,
            recover_housing: 0.035,
            recover_medical: 0.03,
            injury_days_base: 21.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct CupParams {
    /// 代表の登録人数
    pub squad_size: usize,
    /// ロジスティックの傾き
    pub logistic_slope: f32,
    /// 賭ける領地の数（自国が供出する上限）
    pub stake_districts: usize,
    /// 他国の年次成長の振れ幅
    pub nation_growth_noise: f32,
    /// 代表合宿・大会による疲労
    pub cup_fatigue: f32,
}

impl Default for CupParams {
    fn default() -> Self {
        CupParams {
            squad_size: 7,
            logistic_slope: 0.09,
            stake_districts: 1,
            nation_growth_noise: 0.06,
            cup_fatigue: 0.12,
        }
    }
}

/// 予算の分野（FR-BUD-03）。配列の添字と一致させる。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum BudgetField {
    /// 教育・指導者育成
    Education,
    /// 医療・回復支援
    Medical,
    /// 住宅・生活支援
    Housing,
    /// 交通・町の基盤
    Transit,
    /// 道場・競技施設
    Facilities,
    /// 公共チーム・経済参加支援
    PublicTeams,
    /// 代表強化
    National,
    /// 予備枠
    Reserve,
}

impl BudgetField {
    pub const ALL: [BudgetField; 8] = [
        BudgetField::Education,
        BudgetField::Medical,
        BudgetField::Housing,
        BudgetField::Transit,
        BudgetField::Facilities,
        BudgetField::PublicTeams,
        BudgetField::National,
        BudgetField::Reserve,
    ];

    pub fn index(self) -> usize {
        BudgetField::ALL.iter().position(|f| *f == self).unwrap()
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct BudgetParams {
    /// GDP に対する歳入率
    pub revenue_rate: f32,
    /// 公共チーム稼働枠に換算するレート（K あたりの枠）
    pub public_team_rate: f32,
    /// 建設人員の供給（Builder の産出からの換算）
    pub construction_rate: f32,
    /// 赤字（配分 > 収入）の翌年度への持ち越し率
    pub deficit_carry: f32,
}

impl Default for BudgetParams {
    fn default() -> Self {
        BudgetParams {
            revenue_rate: 0.22,
            public_team_rate: 0.35,
            construction_rate: 1.0,
            deficit_carry: 1.0,
        }
    }
}

/// 日次ノイズの振れ幅（FR-SIM-09: 明示パラメータとして持ち、S/N をテストで測れるようにする）
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct NoiseParams {
    pub production: f32,
    pub training: f32,
    pub participation: f32,
    pub weather_step: f32,
    pub supply_step: f32,
}

impl Default for NoiseParams {
    fn default() -> Self {
        NoiseParams {
            production: 0.06,
            training: 0.10,
            participation: 0.05,
            weather_step: 0.08,
            supply_step: 0.06,
        }
    }
}

// ───────────────────────────── 政策カード ─────────────────────────────

/// 政策の効果が触れてよい先（design.md §10）。
///
/// **`Ability` と `Stats` の項目をこの enum に作らない。**
/// 制度から能力値への直接加算をコード上に存在させないための型による規律（FR-POL-06）。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Effect {
    /// 地区の生活・事業環境
    Infra { field: InfraField, delta: f32 },
    /// 施設の新設（工期を伴う）
    BuildFacility {
        kind: FacilityKind,
        name_key: String,
        capacity: f32,
        staff_required: f32,
        upkeep: f32,
        access: AccessRule,
        night_open: bool,
    },
    /// 既存施設の定員・質の変更
    ExpandFacility { kind: FacilityKind, capacity_delta: f32, quality_delta: f32 },
    /// 利用権の変更（建てただけでは使えない、を動かす唯一の手段）
    FacilityAccess { kind: FacilityKind, access: AccessRule },
    /// 夜間開放
    FacilityHours { kind: FacilityKind, night_open: bool },
    /// 人員の養成（成果まで時間がかかる）
    TrainStaff { role: StaffRole, amount: f32 },
    /// 人員の派遣（派遣元の余力を使う）
    DispatchStaff { role: StaffRole, amount: f32, days: u16 },
    /// 利用枠の基準値
    CreditBase { delta: f32 },
    /// 公共チームの稼働枠
    PublicTeamCapacity { delta: f32 },
    /// 住民共同チームへの助成
    CommunityTeamGrant { strength: f32, access_share: f32 },
    /// 代理業チームの稼働枠
    AgencyCapacity { delta: f32 },
    /// 企業スポンサー（民間の支援。信頼と景気に左右される）
    SponsorProgram { strength: f32 },
    /// 代表合同合宿（連携は上がるが疲労と所属先への影響がある）
    NationalCamp { cohesion: f32, fatigue: f32 },
    /// 監査。数値を改善せず、内訳を見えるようにする。
    Audit,
    /// 医療機関との診療協定（診療能力の範囲内で配分）
    MedicalAgreement { capacity: f32 },
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum StaffRole {
    Coach,
    Medic,
    Nursery,
    Official,
    Builder,
}

/// 実行可能条件（FR-POL-03）。満たさない場合は不足を列挙して提示する。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Requirement {
    /// 予算枠（分野と額）
    Budget { field: BudgetField },
    /// 人員
    Staff { role: StaffRole, amount: f32 },
    /// 対象地区に該当施設が存在すること
    Facility { kind: FacilityKind },
    /// 前提となる政策が完了していること
    PriorPolicy { id: String },
    /// 追跡人物が対象に存在すること
    TrackedPerson,
    /// 建設能力
    Construction { amount: f32 },
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum TargetScope {
    /// 地区を1つ選ぶ
    District,
    /// 自国全体
    Nation,
    /// 代表チーム
    NationalTeam,
}

/// AC-03（政策回帰テスト）のための期待値。カード定義に併記する。
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct ExpectedChange {
    pub metric: MetricKey,
    /// +1 なら増える、-1 なら減る
    pub direction: i8,
    /// 何日以内に現れるか
    pub within_days: u16,
}

/// 回帰テストと UI の内訳表示が参照する指標キー。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum MetricKey {
    PracticeMinutes,
    Participation,
    AverageAbility,
    LifeSlack,
    Gdp,
    Fatigue,
    CoachCapacity,
    MedicalCapacity,
    NurseryCapacity,
    DojoCapacity,
    CreditLimit,
    Receipt,
    NationalStrength,
    NationalCohesion,
    GuildVisibility,
    Scouting,
    Trust,
    CommuteMinutes,
    CareMinutes,
    ShoppingMinutes,
    WorkMinutes,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct PolicyDef {
    pub id: String,
    /// 俗称
    pub name: String,
    /// 大仰な公文書名（FR-POL-07）
    pub official_name: String,
    /// 進行日数
    pub days: u8,
    /// 初期費用（K）
    pub initial_cost: f32,
    /// 継続費用（K/年）
    pub upkeep: f32,
    pub field: BudgetField,
    pub requires: Vec<Requirement>,
    pub target: TargetScope,
    pub effects: Vec<Effect>,
    /// 効果発現まで（日）
    pub lead_time: [u16; 2],
    /// 工期（日）。0 なら決定と同時に効く。
    pub construction_days: u16,
    /// 想定される変化と不確実性（表示用。NFR-10 によりキーで持つ）
    pub uncertainty_key: String,
    /// この政策が対処する問題のタグ（推奨枠の絞り込みに使う: FR-POL-05）
    pub addresses: Vec<String>,
    /// 回帰テストの期待値（AC-03）
    pub expect: Vec<ExpectedChange>,
}

impl Default for PolicyDef {
    fn default() -> Self {
        PolicyDef {
            id: String::new(),
            name: String::new(),
            official_name: String::new(),
            days: 1,
            initial_cost: 0.0,
            upkeep: 0.0,
            field: BudgetField::Reserve,
            requires: Vec::new(),
            target: TargetScope::District,
            effects: Vec::new(),
            lead_time: [0, 0],
            construction_days: 0,
            uncertainty_key: String::new(),
            addresses: Vec::new(),
            expect: Vec::new(),
        }
    }
}

// ───────────────────────────── イベント ─────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Scope {
    District,
    Facility,
    Person,
    Cohort,
    Business,
    Nation,
}

/// イベントの発火条件。**全国平均は使わない**（FR-SIM-05）。
/// 参照するのは常に対象そのものの状態。
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum Trigger {
    LifeSlackBelow(f32),
    FatigueAbove(f32),
    HealthBelow(f32),
    NutritionBelow(f32),
    MedicalAccessBelow(f32),
    FoodAccessBelow(f32),
    HousingBelow(f32),
    TransitBelow(f32),
    TrustBelow(f32),
    ScoutingBelow(f32),
    /// 道場の在籍者が実効定員を超えている
    OverEnrolled(f32),
    /// 施設の種別（Facility スコープの絞り込み）
    FacilityKindIs(FacilityKind),
    /// 施設の人員充足率
    StaffingBelow(f32),
    /// 施設の設備の質
    QualityBelow(f32),
    /// 世帯の未清算負担が利用枠に対して
    ObligationRatioAbove(f32),
    /// 組合の準備率
    GuildCoverageBelow(f32),
    /// 事業者の累積収支
    BusinessBalanceBelow(f32),
    /// 決済会場の整備度
    PaymentVenueBelow(f32),
    /// 財政の予備枠
    ReserveBelow(f32),
    /// 進行中の事業が資源不足で止まっている
    ProjectStalled,
    /// 練習参加率
    ParticipationBelow(f32),
    /// 人物が負傷していない
    NotInjured,
    /// 人物の能力が地区平均を上回る
    AbilityAbove(f32),
}

/// イベントが起こす結果。`Effect` が触れない領域（負傷・離脱・移籍）はここだけが持つ。
/// design.md §7: `Ability` を書き換えるのは training::run と events::apply のみ。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Outcome {
    /// 対象地区・コホートの生活環境に作用する
    Effect(Effect),
    /// 練習欠席（当日の練習時間を削る）
    MissPractice { ratio: f32 },
    /// 負傷
    Injure { severity: f32 },
    /// 競技休止（猶予後に活動休止）
    Pause { grace_days: u16 },
    /// 国外移籍
    Emigrate,
    /// 施設の受け入れ停止
    SuspendFacility,
    /// 施設の設備故障
    DamageFacility { quality: f32 },
    /// 職員の長期休養（人員が減る）
    StaffLeave { role: StaffRole, amount: f32, days: u16 },
    /// 事業の停止
    StallProject,
    /// 全世帯の利用枠を一律縮小
    ShrinkCredit { ratio: f32 },
    /// 民間投資の保留
    HoldInvestment { trust: f32 },
    /// 地方予選への不参加
    MissSelection,
    /// 混雑による決済時間の増加
    CongestPayment { minutes: f32 },
    /// 事業者の縮小
    ShrinkBusiness { ratio: f32 },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct EventDef {
    pub id: String,
    pub scope: Scope,
    pub trigger: Vec<Trigger>,
    /// 日次確率（FR-SIM-04: 政策日数で期待発生回数が歪まないよう、必ず日次で引く）
    pub base_chance: f32,
    /// 0 = 予兆, 1 = 本番, 2 = 深刻（FR-SIM-07）
    pub stage: u8,
    /// 前段イベントの id。予兆 → 本番の連鎖を強制する。
    pub prerequisite: Option<String>,
    pub cooldown_days: u16,
    pub outcomes: Vec<Outcome>,
    /// 記事の見出し・本文テンプレートのキー
    pub news_key: String,
    pub kind: super::news::ArticleKind,
    pub weight: u8,
    /// 対応策として提示する政策 id（FR-NEG-02: 2つ以上。唯一解を作らない）
    pub remedies: Vec<String>,
}

impl Default for EventDef {
    fn default() -> Self {
        EventDef {
            id: String::new(),
            scope: Scope::District,
            trigger: Vec::new(),
            base_chance: 0.0,
            stage: 0,
            prerequisite: None,
            cooldown_days: 30,
            outcomes: Vec::new(),
            news_key: String::new(),
            kind: super::news::ArticleKind::Economy,
            weight: 3,
            remedies: Vec::new(),
        }
    }
}

// ───────────────────────────── 初期シナリオ ─────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Scenario {
    pub nation_name: String,
    pub districts: Vec<DistrictSeed>,
    pub rivals: Vec<NationSeed>,
    pub people: Vec<PersonSeed>,
    pub initial_allocations: [f32; 8],
    pub initial_reserve: f32,
    pub given_names: Vec<String>,
    pub family_names: Vec<String>,
}

impl Default for Scenario {
    fn default() -> Self {
        Scenario {
            nation_name: String::new(),
            districts: Vec::new(),
            rivals: Vec::new(),
            people: Vec::new(),
            initial_allocations: [0.0; 8],
            initial_reserve: 0.0,
            given_names: Vec::new(),
            family_names: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct DistrictSeed {
    pub name: String,
    pub is_home: bool,
    pub infra: super::world::Infra,
    /// 地区間の移動コスト（分）。対称行列の行。
    pub distance_to: Vec<u8>,
    pub cohorts: Vec<CohortSeed>,
    pub facilities: Vec<FacilitySeed>,
    pub businesses: Vec<BusinessSeed>,
    pub teams: Vec<TeamSeed>,
}

impl Default for DistrictSeed {
    fn default() -> Self {
        DistrictSeed {
            name: String::new(),
            is_home: false,
            infra: super::world::Infra::default(),
            distance_to: Vec::new(),
            cohorts: Vec::new(),
            facilities: Vec::new(),
            businesses: Vec::new(),
            teams: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct CohortSeed {
    pub age_band: AgeBand,
    pub occupation: Occupation,
    pub headcount: u32,
    pub ability: f32,
    pub spread: f32,
    pub motivation: f32,
    pub household_size: f32,
    pub children: f32,
    pub dependents: f32,
    pub talent: f32,
}

impl Default for CohortSeed {
    fn default() -> Self {
        CohortSeed {
            age_band: AgeBand::Prime,
            occupation: Occupation::Clerk,
            headcount: 100,
            ability: 20.0,
            spread: 6.0,
            motivation: 0.6,
            household_size: 2.6,
            children: 0.4,
            dependents: 0.2,
            talent: 1.0,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct FacilitySeed {
    pub name: String,
    pub kind: FacilityKind,
    pub capacity: f32,
    pub staff_required: f32,
    pub staff: f32,
    pub upkeep: f32,
    pub access: AccessRule,
    pub night_open: bool,
    pub quality: f32,
}

impl Default for FacilitySeed {
    fn default() -> Self {
        FacilitySeed {
            name: String::new(),
            kind: FacilityKind::Dojo,
            capacity: 40.0,
            staff_required: 3.0,
            staff: 3.0,
            upkeep: 20.0,
            access: AccessRule::Public,
            night_open: false,
            quality: 0.7,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct BusinessSeed {
    pub name: String,
    pub kind: super::world::BusinessKind,
    pub team_strength: f32,
    pub team_roster: f32,
    pub scale: f32,
}

impl Default for BusinessSeed {
    fn default() -> Self {
        BusinessSeed {
            name: String::new(),
            kind: super::world::BusinessKind::Grocer,
            team_strength: 24.0,
            team_roster: 7.0,
            scale: 1.0,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct TeamSeed {
    pub name: String,
    pub kind: TeamKind,
    pub strength: f32,
    pub cohesion: f32,
    pub roster: f32,
    pub slots: f32,
    pub upkeep: f32,
    pub access_share: f32,
}

impl Default for TeamSeed {
    fn default() -> Self {
        TeamSeed {
            name: String::new(),
            kind: TeamKind::Community,
            strength: 22.0,
            cohesion: 0.9,
            roster: 7.0,
            slots: 3.0,
            upkeep: 8.0,
            access_share: 0.5,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct PersonSeed {
    pub name: String,
    pub district: usize,
    pub age: u8,
    pub occupation: Occupation,
    pub ability: f32,
    pub talent: f32,
    pub motivation: f32,
    pub children: f32,
    pub dependents: f32,
    pub household_size: f32,
    pub note_key: String,
}

impl Default for PersonSeed {
    fn default() -> Self {
        PersonSeed {
            name: String::new(),
            district: 0,
            age: 22,
            occupation: Occupation::Clerk,
            ability: 30.0,
            talent: 1.0,
            motivation: 0.7,
            children: 0.0,
            dependents: 0.0,
            household_size: 2.0,
            note_key: String::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct NationSeed {
    pub name: String,
    pub strength: f32,
    pub growth: f32,
}

impl Default for NationSeed {
    fn default() -> Self {
        NationSeed { name: String::new(), strength: 55.0, growth: 1.01 }
    }
}

// ───────────────────────────── 文面 ─────────────────────────────

/// 表示文字列。コードに日本語を埋め込まない（NFR-10）。
/// `HashMap` ではなくソート済み `Vec` で持ち、反復順序を決定論にする（NFR-01）。
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TextTable {
    pub entries: Vec<(String, String)>,
}

impl TextTable {
    pub fn get<'a>(&'a self, key: &'a str) -> &'a str {
        match self.entries.binary_search_by(|(k, _)| k.as_str().cmp(key)) {
            Ok(i) => &self.entries[i].1,
            Err(_) => key,
        }
    }

    fn sort(&mut self) {
        self.entries.sort_by(|a, b| a.0.cmp(&b.0));
    }

    /// `{key}` 形式のプレースホルダを置換する。
    pub fn format(&self, key: &str, args: &[(&str, &str)]) -> String {
        let mut s = self.get(key).to_string();
        for (k, v) in args {
            s = s.replace(&format!("{{{k}}}"), v);
        }
        s
    }
}

// ───────────────────────────── Defs 本体 ─────────────────────────────

#[derive(Clone, Debug, Default)]
pub struct Defs {
    pub balance: Balance,
    pub policies: Vec<PolicyDef>,
    pub events: Vec<EventDef>,
    pub scenario: Scenario,
    pub text: TextTable,
}

/// ビルド時に埋め込む既定データ。`assets/` を配布しない経路でも動くようにする。
mod embedded {
    pub const BALANCE: &str = include_str!("../../assets/data/balance.ron");
    pub const POLICIES: &str = include_str!("../../assets/data/policies.ron");
    pub const EVENTS: &str = include_str!("../../assets/data/events.ron");
    pub const SCENARIO: &str = include_str!("../../assets/data/scenario.ron");
    pub const TEXT: &str = include_str!("../../assets/text/ja.ron");
}

#[derive(Debug)]
pub struct DefsError {
    pub file: String,
    pub message: String,
}

impl std::fmt::Display for DefsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.file, self.message)
    }
}

impl std::error::Error for DefsError {}

fn parse<T: for<'de> Deserialize<'de>>(file: &str, src: &str) -> Result<T, DefsError> {
    ron::from_str(src).map_err(|e| DefsError { file: file.to_string(), message: e.to_string() })
}

impl Defs {
    /// 埋め込みデータから読む。テストとフォールバックに使う。
    pub fn embedded() -> Result<Self, DefsError> {
        let mut defs = Defs {
            balance: parse("balance.ron", embedded::BALANCE)?,
            policies: parse("policies.ron", embedded::POLICIES)?,
            events: parse("events.ron", embedded::EVENTS)?,
            scenario: parse("scenario.ron", embedded::SCENARIO)?,
            text: parse("ja.ron", embedded::TEXT)?,
        };
        defs.text.sort();
        defs.validate()?;
        Ok(defs)
    }

    /// `assets/` ディレクトリから読む。見つからないファイルは埋め込みで補う。
    pub fn load_from(dir: &std::path::Path) -> Result<Self, DefsError> {
        let read = |rel: &str, fallback: &'static str| -> String {
            std::fs::read_to_string(dir.join(rel)).unwrap_or_else(|_| fallback.to_string())
        };
        let mut defs = Defs {
            balance: parse("balance.ron", &read("data/balance.ron", embedded::BALANCE))?,
            policies: parse("policies.ron", &read("data/policies.ron", embedded::POLICIES))?,
            events: parse("events.ron", &read("data/events.ron", embedded::EVENTS))?,
            scenario: parse("scenario.ron", &read("data/scenario.ron", embedded::SCENARIO))?,
            text: parse("ja.ron", &read("text/ja.ron", embedded::TEXT))?,
        };
        defs.text.sort();
        defs.validate()?;
        Ok(defs)
    }

    /// 実行ファイルの隣・カレント・CARGO_MANIFEST_DIR の順に `assets/` を探す。
    pub fn load_default() -> Result<Self, DefsError> {
        let mut candidates: Vec<std::path::PathBuf> = Vec::new();
        if let Ok(exe) = std::env::current_exe()
            && let Some(dir) = exe.parent()
        {
            candidates.push(dir.join("assets"));
        }
        candidates.push(std::path::PathBuf::from("assets"));
        candidates.push(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets"));
        for c in candidates {
            if c.join("data/balance.ron").exists() {
                return Defs::load_from(&c);
            }
        }
        Defs::embedded()
    }

    pub fn policy(&self, id: &str) -> Option<&PolicyDef> {
        self.policies.iter().find(|p| p.id == id)
    }

    pub fn event(&self, id: &str) -> Option<&EventDef> {
        self.events.iter().find(|e| e.id == id)
    }

    /// 定義の整合性検査。ロード時に欠損を検出する（design.md §4.1）。
    fn validate(&self) -> Result<(), DefsError> {
        let err = |m: String| DefsError { file: "defs".into(), message: m };
        for o in Occupation::ALL {
            if !self.balance.occupations.iter().any(|d| d.occupation == o) {
                return Err(err(format!("balance.ron: 職業 {o:?} の定義が欠けている")));
            }
        }
        for p in &self.policies {
            for r in &p.requires {
                if let Requirement::PriorPolicy { id } = r
                    && self.policy(id).is_none()
                {
                    return Err(err(format!("policies.ron: {} の前提 {} が存在しない", p.id, id)));
                }
            }
        }
        for e in &self.events {
            if let Some(pre) = &e.prerequisite
                && self.event(pre).is_none()
            {
                return Err(err(format!("events.ron: {} の前段 {} が存在しない", e.id, pre)));
            }
            // FR-NEG-02: 対応策は2つ以上（唯一解を作らない）
            if e.stage > 0 && e.remedies.len() < 2 {
                return Err(err(format!("events.ron: {} の対応策が2件未満", e.id)));
            }
            for r in &e.remedies {
                if self.policy(r).is_none() {
                    return Err(err(format!("events.ron: {} の対応策 {} が存在しない", e.id, r)));
                }
            }
        }
        Ok(())
    }
}
