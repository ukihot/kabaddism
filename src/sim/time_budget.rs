//! 1日24時間の配分（design.md §7 / FR-POP-03）
//!
//! 配分は優先度順に確定し、**残余が練習に回る**。政策は commute / care / shopping / 参加可能性 を
//! 動かすだけで、練習時間そのものには触れない。これが「制度に固定ボーナスを付けない」の実装形。

use std::sync::Arc;

use super::context::{self, DistrictContext};
use super::defs::Defs;
use super::ids::DistrictId;
use super::world::{FacilityKind, Life, MINUTES_PER_DAY, Occupation, PayMethod, World};
use super::{Game, rng};

/// その日の時間配分を全コホート・全追跡人物について確定する。
pub fn allocate(game: &mut Game) {
    let defs: Arc<Defs> = Arc::clone(&game.defs);
    let ctxs = context::build(&game.world);
    let weekend = game.date.is_weekend();
    let noise_amp = defs.balance.noise.participation;

    // 練習需要（人数）を地区ごとに集計してから、道場の受入枠で頭打ちにする。
    let mut demand = vec![0.0f32; game.world.districts.len()];

    let World { districts, people, .. } = &mut game.world;

    // ── パス1: 練習以外の配分を確定し、練習需要を積む ──
    let mut cohort_plan: Vec<Vec<Plan>> = Vec::with_capacity(districts.len());
    for (di, d) in districts.iter_mut().enumerate() {
        let ctx = &ctxs[di];
        let mut plans = Vec::with_capacity(d.cohorts.len());
        for c in &mut d.cohorts {
            let plan = allocate_life(&mut c.life, ctx, &defs, weekend);
            demand[di] += c.headcount as f32 * plan.possibility * plan.wants();
            plans.push(plan);
        }
        cohort_plan.push(plans);
    }

    let mut person_plan: Vec<Plan> = Vec::with_capacity(people.len());
    for p in people.iter_mut() {
        if !p.is_active() {
            p.life.time.practice = 0.0;
            p.life.participation = 0.0;
            person_plan.push(Plan::none());
            continue;
        }
        let di = p.home.index();
        let plan = allocate_life(&mut p.life, &ctxs[di], &defs, weekend);
        demand[di] += plan.possibility * plan.wants();
        person_plan.push(plan);
    }

    // ── パス2: 受入枠で按分し、練習時間を確定する ──
    let scale: Vec<f32> = demand
        .iter()
        .enumerate()
        .map(|(di, dem)| {
            let cap = ctxs[di].dojo_capacity;
            if *dem <= 0.0 { 1.0 } else { (cap / dem).clamp(0.0, 1.0) }
        })
        .collect();

    for (di, d) in districts.iter_mut().enumerate() {
        for (c, plan) in d.cohorts.iter_mut().zip(&cohort_plan[di]) {
            apply_practice(&mut c.life, plan, scale[di], &mut game.rng.daily, noise_amp);
        }
    }
    for (p, plan) in people.iter_mut().zip(&person_plan) {
        if p.is_active() {
            let di = p.home.index();
            apply_practice(&mut p.life, plan, scale[di], &mut game.rng.daily, noise_amp);
        }
    }

    // 在籍者数を道場へ配分する（受入停止イベントの判定材料になる）
    record_enrollment(&mut game.world, &demand, &ctxs);
}

#[derive(Clone, Copy)]
struct Plan {
    possibility: f32,
    desired: f32,
}

impl Plan {
    fn none() -> Self {
        Plan { possibility: 0.0, desired: 0.0 }
    }
    fn wants(&self) -> f32 {
        if self.desired > 0.0 { 1.0 } else { 0.0 }
    }
}

fn allocate_life(life: &mut Life, ctx: &DistrictContext, defs: &Defs, weekend: bool) -> Plan {
    let tp = &defs.balance.time;
    let occ = defs.balance.occupation(life.occupation);
    let t = &mut life.time;

    // 睡眠: 住環境が悪いほど削られる
    let housing = (ctx.infra.housing * 0.75 + ctx.dormitory_quality * 0.25).clamp(0.0, 1.0);
    t.sleep = tp.base_sleep - tp.sleep_penalty_max * (1.0 - housing);

    // 労働: カバディ休暇協定は所定を短縮する
    t.work = occ.work_minutes * (1.0 - 0.25 * ctx.infra.work_relief);

    // 通勤: 交通整備度と地区の距離
    let transit = (ctx.infra.transit + ctx.transit_facility).clamp(0.0, 1.0);
    t.commute = if t.work > 0.0 {
        let dist = 0.5 + 0.5 * (ctx.mean_distance / 60.0).min(1.5);
        tp.commute_base * dist * (1.0 - tp.commute_transit_relief * transit)
    } else {
        0.0
    };

    // 家事・育児・介護: 託児所の利用可否で減る
    let nursery_avail = if ctx.population > 0.0 {
        (ctx.nursery_capacity / (ctx.population * 0.06).max(1.0)).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let childcare = (ctx.infra.childcare * 0.4 + nursery_avail * 0.6).clamp(0.0, 1.0);
    t.care = tp.care_base
        + life.household.children
            * tp.care_per_child
            * (1.0 - tp.care_childcare_relief * childcare)
        + life.household.dependents * tp.care_per_dependent;

    // 決済に要する時間: 前日に使った方式と会場の混雑で決まる
    let venue = (ctx.infra.payment_venue * 0.6
        + (ctx.venue_capacity / ctx.population.max(1.0)).min(1.0) * 0.4)
        .clamp(0.0, 1.0);
    let base = match life.last_pay {
        PayMethod::Card => tp.shopping_card,
        PayMethod::Genkaba => tp.shopping_genkaba,
        PayMethod::Proxy => tp.shopping_proxy,
    };
    t.shopping = base * (1.0 - tp.shopping_venue_relief * venue) + life.pending_congestion;
    if weekend {
        // 週末の町内大会（カード清算）にも時間がかかる
        t.shopping += tp.weekend_settlement_minutes * (1.0 - 0.3 * venue);
    }

    // 残余
    let used = t.sleep + t.work + t.commute + t.care + t.shopping;
    let mut leisure = (MINUTES_PER_DAY - used).max(0.0);

    // 生活余力（FR-STAT-02 生活分野の指標）
    life.life_slack = (leisure / tp.slack_reference).clamp(0.0, 1.0);

    // 決済負担が利用枠を圧迫していると、余暇を労務に振り替えて清算に充てる。
    // 練習時間が削られる形で育成へ跳ね返る（FR-ECO-03 回復手段1）。
    let pressure = (life.obligation / life.credit_limit.max(0.1) - 0.7).max(0.0);
    t.extra_labor = (pressure * 0.5).min(0.6) * leisure;
    leisure -= t.extra_labor;

    // 練習: 余暇 × 意欲 × 参加可能性。上限は道場の受入枠（パス2で按分）。
    let night_worker = occ.work_minutes >= 420.0;
    let possibility = ctx.participation_possibility(night_worker);
    let motivation = (life.motivation * occ.motivation_bias).clamp(0.0, 1.5);
    let desired = (leisure * motivation * possibility).max(0.0);

    t.leisure = leisure;
    t.practice = 0.0;
    Plan { possibility, desired }
}

fn apply_practice(
    life: &mut Life,
    plan: &Plan,
    scale: f32,
    rng: &mut rand_chacha::ChaCha8Rng,
    noise_amp: f32,
) {
    let n = rng::noise(rng, noise_amp);
    let practice = (plan.desired * scale * n).max(0.0).min(life.time.leisure);
    life.time.practice = practice;
    life.time.leisure -= practice;
    life.participation =
        if plan.desired > 0.0 { (plan.possibility * scale * n).clamp(0.0, 1.0) } else { 0.0 };
}

/// 道場の在籍者数を、実効定員に比例して配分する。
fn record_enrollment(world: &mut World, demand: &[f32], ctxs: &[DistrictContext]) {
    for i in 0..world.facilities.len() {
        let f = &world.facilities[i];
        if f.kind != FacilityKind::Dojo {
            continue;
        }
        let di = f.district.index();
        let ctx = &ctxs[di];
        let share = if ctx.dojo_capacity > 0.0 {
            (f.capacity * f.staffing().min(1.0)) / ctx.dojo_capacity
        } else {
            0.0
        };
        world.facilities[i].enrolled = demand[di] * share;
    }
}

/// 地区の平均練習時間（分）。指標・回帰テスト用。
pub fn mean_practice(world: &World, district: DistrictId) -> f32 {
    let d = world.district(district);
    let mut num = 0.0;
    let mut den = 0.0;
    for c in &d.cohorts {
        num += c.headcount as f32 * c.life.time.practice;
        den += c.headcount as f32;
    }
    if den > 0.0 { num / den } else { 0.0 }
}

/// 職業が生産を担うか（生産と競技力の区別: FR-ECO-05）。
pub fn is_productive(o: Occupation) -> bool {
    !matches!(
        o,
        Occupation::Student | Occupation::Caregiver | Occupation::Retired | Occupation::Athlete
    )
}
