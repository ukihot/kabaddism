//! 世界の状態（design.md §4.2）
//!
//! 設計上の要点: **コホートと追跡人物は同じ計算関数を通す**（FR-POP-01）。
//! そのために、両者が共有する生活状態を [`Life`] に切り出し、`headcount` を重みとして扱うだけにする。
//! これを破ると「集団は改善したのに選手は改善しない」という説明不能な挙動が出る。

use serde::{Deserialize, Serialize};

use super::ids::*;

// ───────────────────────────── 共通の小さな型 ─────────────────────────────

/// 職業。所定労働時間と生産性は balance.ron の表から引く（コードに数値を埋めない）。
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub enum Occupation {
    /// 農業
    Farmer,
    /// 製造
    Manufacturer,
    /// 建設
    Builder,
    /// 医療
    Medic,
    /// 飲食
    Cook,
    /// 接客・小売
    Clerk,
    /// 指導者
    Coach,
    /// 行政職員
    Official,
    /// 学生
    Student,
    /// 家事・育児・介護
    Caregiver,
    /// 専業選手
    Athlete,
    /// 引退・高齢
    Retired,
}

impl Occupation {
    pub const ALL: [Occupation; 12] = [
        Occupation::Farmer,
        Occupation::Manufacturer,
        Occupation::Builder,
        Occupation::Medic,
        Occupation::Cook,
        Occupation::Clerk,
        Occupation::Coach,
        Occupation::Official,
        Occupation::Student,
        Occupation::Caregiver,
        Occupation::Athlete,
        Occupation::Retired,
    ];

    pub fn index(self) -> usize {
        Occupation::ALL.iter().position(|o| *o == self).unwrap()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub enum AgeBand {
    Child,
    Youth,
    Prime,
    Middle,
    Senior,
}

/// 1日1440分の配分（design.md §7 / FR-POP-03）。単位は分。
///
/// `practice` は残余からのみ確保される。他の項目を削ることでしか練習時間は増えない。
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct TimeBudget {
    pub sleep: f32,
    pub work: f32,
    pub commute: f32,
    pub care: f32,
    pub shopping: f32,
    pub practice: f32,
    pub leisure: f32,
    /// 決済不足を労務で埋めるために余暇から振り替えた時間（FR-ECO-03 回復手段1）
    pub extra_labor: f32,
}

pub const MINUTES_PER_DAY: f32 = 1440.0;

impl TimeBudget {
    pub fn total(&self) -> f32 {
        self.sleep
            + self.work
            + self.commute
            + self.care
            + self.shopping
            + self.practice
            + self.leisure
            + self.extra_labor
    }
}

/// 健康・栄養・疲労。
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Condition {
    /// 0..1
    pub health: f32,
    /// 0..1
    pub nutrition: f32,
    /// 0..1.5 程度。1.0 を超えると無理押し領域。
    pub fatigue: f32,
}

impl Default for Condition {
    fn default() -> Self {
        Condition { health: 0.85, nutrition: 0.8, fatigue: 0.25 }
    }
}

impl Condition {
    /// コンディション係数 c ∈ [0, 1.2]（design.md §9）
    pub fn factor(&self) -> f32 {
        let base = 0.45 * self.health + 0.35 * self.nutrition + 0.4 * (1.0 - self.fatigue).max(0.0);
        base.clamp(0.0, 1.2)
    }
}

/// 能力。コホートは平均と分散、人物は分散 0。
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct Ability {
    /// 0..100
    pub value: f32,
    pub spread: f32,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Household {
    pub size: f32,
    /// 未就学児の人数（保育所の有無で care 時間が変わる）
    pub children: f32,
    /// 本人が戦えない被扶養者（FR-ECO-08 の代替決済経路の対象）
    pub dependents: f32,
}

impl Default for Household {
    fn default() -> Self {
        Household { size: 2.6, children: 0.4, dependents: 0.2 }
    }
}

/// 一人あたり・一日あたりの受取（FR-ECO-07: 生産とは別指標）。
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct Receipt {
    pub necessity: f32,
    pub service: f32,
}

/// 決済方式。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum PayMethod {
    /// 現カバ（店頭で対戦して即時決済）
    Genkaba,
    /// カバディカード（週末の町内大会でまとめて清算）
    Card,
    /// 代理・公的肩代わり（FR-ECO-08）
    Proxy,
}

/// コホートと追跡人物が共有する生活状態。**両者は同じ関数を通る**（FR-POP-01）。
/// 値はすべて「1人あたり」または「1世帯あたり」に正規化してあり、規模は `headcount` が持つ。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Life {
    pub occupation: Occupation,
    pub household: Household,
    pub time: TimeBudget,
    pub condition: Condition,
    pub ability: Ability,
    /// 成長速度の係数。不変。
    pub talent: f32,
    pub motivation: f32,
    pub experience: f32,
    /// 世帯あたりの決済負担（K）
    pub obligation: f32,
    /// 世帯あたりの利用枠（K）
    pub credit_limit: f32,
    /// 過去4週の清算率（履行実績係数の素）
    pub settlement_record: f32,
    /// 所属チーム（共同チーム・私有チーム等）
    pub team: Option<TeamId>,
    /// 直近日の受取（1人あたり）
    pub receipt: Receipt,
    /// 直近日に使った決済方式
    pub last_pay: PayMethod,
    /// 直近日に練習へ参加できたか（0..1。コホートでは参加率）
    pub participation: f32,
    /// 生活余力（必需品充足後に残る可処分時間の割合 0..1）
    pub life_slack: f32,
    /// イベントで上乗せされた決済の待ち時間（分）。日々減衰する。
    pub pending_congestion: f32,
    /// 週内に労務で積み上げた清算原資（K）。週末清算で使い切る。
    pub labor_credit: f32,
}

impl Life {
    pub fn new(occupation: Occupation) -> Self {
        Life {
            occupation,
            household: Household::default(),
            time: TimeBudget::default(),
            condition: Condition::default(),
            ability: Ability { value: 20.0, spread: 6.0 },
            talent: 1.0,
            motivation: 0.6,
            experience: 0.0,
            obligation: 0.0,
            credit_limit: 12.0,
            settlement_record: 1.0,
            team: None,
            receipt: Receipt::default(),
            last_pay: PayMethod::Card,
            participation: 0.0,
            life_slack: 0.0,
            pending_congestion: 0.0,
            labor_credit: 0.0,
        }
    }

    /// 本人が決済のために戦えるか。負傷・高齢・育児等では戦えない（FR-ECO-08）。
    pub fn can_fight(&self) -> bool {
        !matches!(self.occupation, Occupation::Retired)
            && self.condition.health > 0.35
            && self.condition.fatigue < 1.3
    }
}

// ───────────────────────────── 履歴 ─────────────────────────────

/// 人物・施設・地区に残る履歴（FR-SIM-10 / FR-POP-07）。
/// ニュースの続報はここを素にする。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub date: super::calendar::Date,
    /// assets/text/ja.ron のキー
    pub text_key: String,
    pub detail: String,
}

// ───────────────────────────── 地区 ─────────────────────────────

/// 地区の生活・事業環境。政策の `Effect` が触れてよい唯一の面（design.md §10）。
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Infra {
    /// 交通整備度 0..1 → 通勤・練習アクセス
    pub transit: f32,
    /// 住環境 0..1 → 睡眠の質・回復
    pub housing: f32,
    /// 医療アクセス 0..1 → 回復・負傷からの復帰
    pub medical: f32,
    /// 食環境 0..1 → 栄養
    pub food: f32,
    /// 保育供給 0..1 → 家事育児時間
    pub childcare: f32,
    /// 夜間練習アクセス 0..1 → 参加可能性
    pub night_access: f32,
    /// 決済会場整備 0..1 → 決済に要する時間・清算の会場係数
    pub payment_venue: f32,
    /// 家賃補助（K/日/世帯）
    pub housing_support: f32,
    /// 生活保障（K/日/世帯）
    pub livelihood_support: f32,
    /// 制度への信頼 0..1 → 民間投資
    pub trust: f32,
    /// 選考の到達範囲 0..1 → 才能発掘
    pub scouting: f32,
    /// 勤務・練習両立の協定 0..1 → 所定労働時間の短縮
    pub work_relief: f32,
}

impl Default for Infra {
    fn default() -> Self {
        Infra {
            transit: 0.5,
            housing: 0.5,
            medical: 0.5,
            food: 0.5,
            childcare: 0.3,
            night_access: 0.3,
            payment_venue: 0.4,
            housing_support: 0.0,
            livelihood_support: 0.0,
            trust: 0.6,
            scouting: 0.4,
            work_relief: 0.0,
        }
    }
}

/// `Effect` から addressable な `Infra` のフィールド。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum InfraField {
    Transit,
    Housing,
    Medical,
    Food,
    Childcare,
    NightAccess,
    PaymentVenue,
    HousingSupport,
    LivelihoodSupport,
    Trust,
    Scouting,
    WorkRelief,
}

impl Infra {
    pub fn get(&self, f: InfraField) -> f32 {
        match f {
            InfraField::Transit => self.transit,
            InfraField::Housing => self.housing,
            InfraField::Medical => self.medical,
            InfraField::Food => self.food,
            InfraField::Childcare => self.childcare,
            InfraField::NightAccess => self.night_access,
            InfraField::PaymentVenue => self.payment_venue,
            InfraField::HousingSupport => self.housing_support,
            InfraField::LivelihoodSupport => self.livelihood_support,
            InfraField::Trust => self.trust,
            InfraField::Scouting => self.scouting,
            InfraField::WorkRelief => self.work_relief,
        }
    }

    pub fn add(&mut self, f: InfraField, delta: f32) {
        // 0..1 に収める指標と、K建ての支援額（上限なし）を区別する。
        let slot = match f {
            InfraField::Transit => &mut self.transit,
            InfraField::Housing => &mut self.housing,
            InfraField::Medical => &mut self.medical,
            InfraField::Food => &mut self.food,
            InfraField::Childcare => &mut self.childcare,
            InfraField::NightAccess => &mut self.night_access,
            InfraField::PaymentVenue => &mut self.payment_venue,
            InfraField::Trust => &mut self.trust,
            InfraField::Scouting => &mut self.scouting,
            InfraField::WorkRelief => &mut self.work_relief,
            InfraField::HousingSupport => {
                self.housing_support = (self.housing_support + delta).max(0.0);
                return;
            }
            InfraField::LivelihoodSupport => {
                self.livelihood_support = (self.livelihood_support + delta).max(0.0);
                return;
            }
        };
        *slot = (*slot + delta).clamp(0.0, 1.0);
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct District {
    pub name: String,
    /// 領地の所属。世界大会の結果で変わる（FR-CUP-07）。
    pub owner: NationId,
    pub cohorts: Vec<Cohort>,
    pub infra: Infra,
    /// 地区間の移動コスト（分）。添字 = DistrictId
    pub distance_to: Vec<u8>,
    /// 本拠地（賭けの対象外: FR-CUP-04）
    pub is_home: bool,
    pub history: Vec<HistoryEntry>,
}

impl District {
    pub fn population(&self) -> u32 {
        self.cohorts.iter().map(|c| c.headcount).sum()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Cohort {
    pub age_band: AgeBand,
    pub headcount: u32,
    pub life: Life,
}

// ───────────────────────────── 追跡人物 ─────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum PersonStatus {
    Active,
    /// 競技休止（生活事情での離脱）
    Paused,
    Retired,
    /// 国外移籍
    Emigrated,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Injury {
    pub days_remaining: u16,
    pub severity: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Person {
    pub name: String,
    pub home: DistrictId,
    pub age: u8,
    pub life: Life,
    pub injury: Option<Injury>,
    pub status: PersonStatus,
    /// 代表歴
    pub caps: u16,
    /// 直近の代表選出
    pub selected_for_cup: bool,
    /// 既に報じた成長の節目（0=未報, 1=40台, 2=60台, 3=80台）
    pub milestone: u8,
    /// 既に報じた身分。状態が変わった日にだけ記事が出る。
    pub reported_status: PersonStatus,
    pub history: Vec<HistoryEntry>,
}

impl Person {
    /// 育成・競技の対象として動けるか。
    pub fn is_active(&self) -> bool {
        self.status == PersonStatus::Active && self.injury.is_none()
    }
}

// ───────────────────────────── 施設 ─────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub enum FacilityKind {
    /// 道場
    Dojo,
    /// 住宅
    Housing,
    /// 学校
    School,
    /// 病院
    Hospital,
    /// 食堂
    Canteen,
    /// 店舗
    Shop,
    /// 交通
    Transit,
    /// 決済会場
    PaymentVenue,
    /// 託児所
    Nursery,
    /// 選手寮
    Dormitory,
}

/// 誰が利用できるか（FR-TOWN-04）。建てただけでは全員が使えない。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum AccessRule {
    /// 誰でも
    Public,
    /// 当該地区の住民のみ
    Residents,
    /// 会員・契約者のみ
    Members,
    /// 指定選手のみ
    Selected,
}

impl AccessRule {
    /// 地区住民一般から見た実効利用率 0..1。
    pub fn openness(self) -> f32 {
        match self {
            AccessRule::Public => 1.0,
            AccessRule::Residents => 0.9,
            AccessRule::Members => 0.45,
            AccessRule::Selected => 0.12,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum FacilityState {
    UnderConstruction,
    Operating,
    /// 人員不足・故障などで受け入れ停止
    Suspended,
    Closed,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Facility {
    pub name: String,
    pub kind: FacilityKind,
    pub district: DistrictId,
    /// 定員（利用者数）
    pub capacity: f32,
    /// 運営に必要な人員
    pub staff_required: f32,
    /// 実際に配置されている人員
    pub staff: f32,
    /// 維持費（K/年）
    pub upkeep: f32,
    pub state: FacilityState,
    pub access: AccessRule,
    /// 夜間開放
    pub night_open: bool,
    /// 設備の質 0..1（寮の給湯器など）
    pub quality: f32,
    /// 在籍者数（道場の入門者など）
    pub enrolled: f32,
    pub history: Vec<HistoryEntry>,
}

impl Facility {
    pub fn is_running(&self) -> bool {
        self.state == FacilityState::Operating
    }

    /// 人員充足率 0..1。
    pub fn staffing(&self) -> f32 {
        if self.staff_required <= 0.0 {
            1.0
        } else {
            (self.staff / self.staff_required).clamp(0.0, 1.5)
        }
    }
}

// ───────────────────────────── チーム ─────────────────────────────

/// 所有・運営方式（FR-TEAM-02）。所有・運営・費用負担・利用機会は別属性として扱う。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum TeamKind {
    /// 私有
    Private,
    /// 店舗常駐
    ShopResident,
    /// 住民共同
    Community,
    /// 公共
    Public,
    /// 代理業
    Agency,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Team {
    pub name: String,
    pub kind: TeamKind,
    pub district: DistrictId,
    /// 競技力（実効戦力の素）
    pub strength: f32,
    /// 連携 0..1.3
    pub cohesion: f32,
    /// 登録人数
    pub roster: f32,
    /// チーム疲労 0..1.5
    pub fatigue: f32,
    /// 1日に出場できる枠（試合数相当）
    pub slots: f32,
    /// 本日消費済みの出場枠
    pub slots_used: f32,
    /// 維持費（K/年）
    pub upkeep: f32,
    /// 追跡人物のメンバー
    pub members: Vec<PersonId>,
    /// 利用権を持つ世帯の比率 0..1（誰がこのチームを使えるか）
    pub access_share: f32,
    /// 代表活動で抜けている人数
    pub on_national_duty: f32,
}

impl Team {
    /// 実効戦力 S（疲労・出場余力を反映）。
    pub fn effective_strength(&self) -> f32 {
        let avail = (self.roster - self.on_national_duty).max(0.0) / self.roster.max(0.1);
        self.strength * self.cohesion * (1.0 - 0.6 * self.fatigue).max(0.15) * avail.clamp(0.0, 1.0)
    }

    /// 残りの出場余力 0..（FR-TEAM-04）。枯渇すると決済・大会に参加できない。
    pub fn spare_slots(&self) -> f32 {
        (self.slots - self.slots_used).max(0.0)
    }
}

// ───────────────────────────── 事業者 ─────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum BusinessKind {
    /// 生活必需品を扱う小規模店・スーパー
    Grocer,
    /// 高級店（強豪チームを用意する）
    Luxury,
    /// 生産者（農業・製造）
    Producer,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum BusinessState {
    Operating,
    /// 赤字継続による縮小
    Shrinking,
    Closed,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Business {
    pub name: String,
    pub kind: BusinessKind,
    pub district: DistrictId,
    /// 常駐チーム（現カバの相手）
    pub team: TeamId,
    /// 日次収支（K）
    pub daily_balance: f32,
    /// 累積収支（K）
    pub cum_balance: f32,
    /// 取扱規模 0..1（縮小するとこれが下がる）
    pub scale: f32,
    pub state: BusinessState,
    /// 赤字が続いた日数
    pub deficit_days: u16,
    pub history: Vec<HistoryEntry>,
}

// ───────────────────────────── カード組合 ─────────────────────────────

/// カード組合（FR-ECO-04）。加盟店への給付を保証し、その保証には裏付けが要る。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CardGuild {
    /// 利用枠の基準値（K/世帯）
    pub credit_base: f32,
    /// 保証している給付額（K）
    pub guaranteed: f32,
    /// 準備（公共チーム稼働 + 物資 + 公的支援）の評価額（K）
    pub reserve: f32,
    /// 準備率 = reserve / guaranteed
    pub coverage: f32,
    /// 監査により内訳が可視化されているか（カードは数値を改善せず、見えるようにする）
    pub audited: bool,
    /// 直近の未清算残高（K）
    pub outstanding: f32,
}

// ───────────────────────────── 外部環境 ─────────────────────────────

/// 外部環境の変動と、それを受け止める能力は別変数（FR-SIM-08）。
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Environment {
    /// 天候 0..1（1 が良い）
    pub weather: f32,
    /// 供給状況 0..1
    pub supply: f32,
    /// 変動を受け止める能力（備蓄・代替手段）0..1
    pub resilience: f32,
}

impl Default for Environment {
    fn default() -> Self {
        Environment { weather: 0.7, supply: 0.7, resilience: 0.5 }
    }
}

// ───────────────────────────── World ─────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct World {
    pub districts: Vec<District>,
    pub facilities: Vec<Facility>,
    pub teams: Vec<Team>,
    /// 追跡人物のみ（30〜80: FR-POP-08）
    pub people: Vec<Person>,
    pub businesses: Vec<Business>,
    pub guild: CardGuild,
    pub environment: Environment,
    /// 実物の在庫。生産 = 消費 + 在庫変化 を閉じるために持つ（design.md §8.1）
    pub stock_necessity: f32,
    pub stock_service: f32,
    /// 建設能力の未使用分（事業の進捗に使う）
    pub stock_buildwork: f32,
    /// 公共チームの本日の稼働枠（予算から供給される実資源）
    pub public_team_capacity: f32,
    /// 代理業チームの本日の稼働枠
    pub agency_capacity: f32,
    /// 政策で積み上げた公共チーム稼働の基準値
    pub public_team_base: f32,
    /// 政策で積み上げた代理業稼働の基準値
    pub agency_base: f32,
    /// 代表チーム
    pub national_team: TeamId,
}

impl World {
    pub fn population(&self) -> u32 {
        self.districts.iter().map(|d| d.population()).sum()
    }

    /// 自国の地区のみ（領地は大会で移動する）。
    pub fn owned_districts(&self, nation: NationId) -> impl Iterator<Item = (DistrictId, &District)> {
        self.districts
            .iter()
            .enumerate()
            .filter(move |(_, d)| d.owner == nation)
            .map(|(i, d)| (DistrictId::from_index(i), d))
    }

    pub fn district(&self, id: DistrictId) -> &District {
        &self.districts[id.index()]
    }

    pub fn district_mut(&mut self, id: DistrictId) -> &mut District {
        &mut self.districts[id.index()]
    }

    pub fn person(&self, id: PersonId) -> &Person {
        &self.people[id.index()]
    }

    pub fn team(&self, id: TeamId) -> &Team {
        &self.teams[id.index()]
    }

    /// 指定地区・種別の稼働中施設。反復順序は常に Vec の添字順（NFR-01）。
    pub fn facilities_in(
        &self,
        district: DistrictId,
        kind: FacilityKind,
    ) -> impl Iterator<Item = (FacilityId, &Facility)> {
        self.facilities
            .iter()
            .enumerate()
            .filter(move |(_, f)| f.district == district && f.kind == kind)
            .map(|(i, f)| (FacilityId::from_index(i), f))
    }

    /// 地区・種別の実効定員（稼働中・人員充足を反映）。
    pub fn capacity_of(&self, district: DistrictId, kind: FacilityKind) -> f32 {
        self.facilities_in(district, kind)
            .filter(|(_, f)| f.is_running())
            .map(|(_, f)| f.capacity * f.staffing().min(1.0))
            .sum()
    }
}
