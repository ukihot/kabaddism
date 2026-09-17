//! 日次の生産・決済・収支（design.md §8 / FR-ECO-*）
//!
//! 要点:
//! - 財・サービスの生産は**労働からのみ**生まれる。カバディの勝敗は生産を増やさない（FR-ECO-05）。
//! - GDP は新規生産の標準価値の合計。中間投入を重複計上せず、所有移転を生産に数えない（FR-ECO-06）。
//! - 「どれだけ生産したか」と「誰が受け取れたか」は別の指標として持つ（FR-ECO-07）。

pub mod accounts;
pub mod card;
pub mod genkaba;

use std::sync::Arc;

use super::context::{self, DistrictContext};
use super::defs::{BudgetField, Defs, Good};
use super::environment;
use super::ids::{CohortId, DistrictId, Target};
use super::world::{BusinessKind, BusinessState, Life, PayMethod, World};
use super::{Game, rng, teams};

/// 世帯数。すべての K建て量は「1世帯あたり」で持ち、ここで規模に戻す。
fn households(weight: f32, size: f32) -> f32 {
    weight / size.max(0.5)
}

/// 未清算負担の全国合計（K）。会計恒等式の両端で同じ関数を使う。
pub fn total_obligation(world: &World) -> f32 {
    let mut sum = 0.0;
    for d in &world.districts {
        for c in &d.cohorts {
            sum += c.life.obligation * households(c.headcount as f32, c.life.household.size);
        }
    }
    for p in &world.people {
        sum += p.life.obligation * households(1.0, p.life.household.size);
    }
    sum
}

// ───────────────────────────── ② 生産 ─────────────────────────────

/// 労働 → 財・サービス。所得ではなく実物を作る。
pub fn produce(game: &mut Game, env: super::world::Environment) {
    let defs: Arc<Defs> = Arc::clone(&game.defs);
    let eco = defs.balance.economy;
    let amp = defs.balance.noise.production;

    let weather = environment::buffered(env.weather, env.resilience);
    let supply = environment::buffered(env.supply, env.resilience);

    let mut prod_n = 0.0;
    let mut prod_s = 0.0;
    let mut prod_b = 0.0;
    let mut inter_n = 0.0;

    {
        let World { districts, people, .. } = &mut game.world;
        for d in districts.iter_mut() {
            for c in d.cohorts.iter_mut() {
                let w = c.headcount as f32;
                let out =
                    produce_one(&mut c.life, w, &defs, weather, supply, &mut game.rng.daily, amp);
                prod_n += out.0;
                prod_s += out.1;
                prod_b += out.2;
                inter_n += out.3;
            }
        }
        for p in people.iter_mut() {
            if p.status == super::world::PersonStatus::Emigrated {
                continue;
            }
            let out =
                produce_one(&mut p.life, 1.0, &defs, weather, supply, &mut game.rng.daily, amp);
            prod_n += out.0;
            prod_s += out.1;
            prod_b += out.2;
            inter_n += out.3;
        }
    }

    // 中間投入は生産された必需品から消える（重複計上しない）
    let l = &mut game.ledger;
    l.produced_necessity = prod_n;
    l.produced_service = prod_s;
    l.produced_buildwork = prod_b;
    l.intermediate_necessity = inter_n;

    // 実質 GDP: 標準価値を固定して評価する
    l.gdp =
        (prod_n * eco.value_necessity + prod_s * eco.value_service + prod_b * eco.value_buildwork)
            - inter_n * eco.value_necessity;

    game.world.stock_necessity += prod_n - inter_n;
    // サービスは在庫できない。建設仕事は1日だけ持ち越す。
    game.world.stock_service = prod_s;
    // 前日分の建設仕事の未使用は失われる（labour は貯められない）
    let leftover = game.world.stock_buildwork;
    game.ledger.expired_buildwork = leftover.max(0.0);
    game.world.stock_buildwork = prod_b;
}

/// 戻り値: (必需品, サービス, 建設仕事, 中間投入)
fn produce_one(
    life: &mut Life,
    weight: f32,
    defs: &Defs,
    weather: f32,
    supply: f32,
    r: &mut rand_chacha::ChaCha8Rng,
    amp: f32,
) -> (f32, f32, f32, f32) {
    let occ = defs.balance.occupation(life.occupation);
    let eco = &defs.balance.economy;

    // 労務による直接清算のぶんも働いている（FR-ECO-03 回復手段1）
    let hours = (life.time.work + life.time.extra_labor) / 60.0;
    life.labor_credit += life.time.extra_labor * eco.labor_settlement_rate * life.household.size;

    if occ.output == Good::None || hours <= 0.0 {
        return (0.0, 0.0, 0.0, 0.0);
    }

    let c = life.condition.factor();
    let env_factor = match occ.output {
        Good::Necessity => {
            if life.occupation == super::world::Occupation::Farmer {
                weather
            } else {
                supply
            }
        }
        Good::BuildWork => supply,
        _ => 1.0,
    };
    let units = hours * occ.productivity * c * env_factor * rng::noise(r, amp) * weight;
    let inter = units * occ.intermediate;

    match occ.output {
        Good::Necessity => (units, 0.0, 0.0, inter),
        Good::Service => (0.0, units, 0.0, inter),
        Good::BuildWork => (0.0, 0.0, units, inter),
        Good::None => (0.0, 0.0, 0.0, 0.0),
    }
}

// ───────────────────────────── ② 決済 ─────────────────────────────

#[derive(Clone, Copy)]
enum Who {
    Cohort(usize, usize),
    Person(usize),
}

struct Row {
    who: Who,
    district: usize,
    /// 人数
    weight: f32,
    /// 1人あたりの需要
    need_n: f32,
    need_s: f32,
    /// 受取の取り合いにおける請求力
    claim: f32,
    /// 決済のために立てられる実効戦力
    strength: f32,
    can_fight: bool,
    household_size: f32,
    alloc_n: f32,
    alloc_s: f32,
}

/// 現カバ決済・カード利用を1日分処理する。
///
/// 供給が需要に満たないときの配分は請求力で決まる。これにより
/// 「どれだけ生産したか」と「誰が受け取れたか」が乖離し得る（FR-ECO-07 / AC-07）。
pub fn settle_daily(game: &mut Game) {
    let defs: Arc<Defs> = Arc::clone(&game.defs);
    let eco = defs.balance.economy;
    let ctxs = context::build(&game.world);

    let mut rows = collect_rows(&game.world, &eco);
    if rows.is_empty() {
        return;
    }

    // ── 配分 ──
    allocate(&mut rows, game.world.stock_necessity, game.world.stock_service, &eco);
    cap_by_delivery(&mut rows, &game.world, &eco);

    // ── 支払 ──
    let mut district_receipts = vec![0.0f32; game.world.districts.len()];
    let mut district_units = vec![0.0f32; game.world.districts.len()];
    let mut consumed_n = 0.0;
    let mut consumed_s = 0.0;
    let mut incurred = 0.0;
    let mut genkaba_paid = 0.0;
    let mut receipt_value = 0.0;

    let mut agency_left = game.world.agency_capacity;
    let mut support_paid = 0.0f32;

    for row in &rows {
        let ctx = &ctxs[row.district];
        let value_pc = row.alloc_n * eco.value_necessity + row.alloc_s * eco.value_service;
        let hh = households(row.weight, row.household_size);
        let gross_household = if hh > 0.0 { value_pc * row.weight / hh } else { 0.0 };
        // 家賃補助・生活保障は、世帯が負う額を肩代わりする。財源は住宅・生活支援の枠。
        let support =
            (ctx.infra.housing_support + ctx.infra.livelihood_support).min(gross_household);
        let value_household = gross_household - support;
        support_paid += support * hh;
        let total_value = value_household * hh;

        consumed_n += row.alloc_n * row.weight;
        consumed_s += row.alloc_s * row.weight;
        receipt_value += value_pc * row.weight;
        district_units[row.district] += (row.alloc_n + row.alloc_s) * row.weight;

        let guild_usable = game.world.guild.coverage > 0.3;
        let life = life_mut(&mut game.world, row.who);
        life.receipt.necessity = row.alloc_n;
        life.receipt.service = row.alloc_s;

        if total_value <= 1e-6 {
            continue;
        }

        // 決済方式を決める。
        let can_card = life.obligation + value_household <= life.credit_limit;
        let method = if !row.can_fight && agency_left >= total_value * 0.1 {
            // 本人が戦えない世帯には代替決済経路がある（FR-ECO-08）
            agency_left -= total_value * 0.1;
            PayMethod::Proxy
        } else if can_card && guild_usable {
            PayMethod::Card
        } else {
            PayMethod::Genkaba
        };

        match method {
            PayMethod::Card | PayMethod::Proxy => {
                life.obligation += value_household;
                incurred += total_value;
                district_receipts[row.district] += total_value + support * hh;
            }
            PayMethod::Genkaba => {
                // 商品は必ず引き渡される。勝敗が決めるのは支払負担の倍率だけ。
                let shop = if row.alloc_s > row.alloc_n * 0.8 {
                    ctx.luxury_strength
                } else {
                    ctx.shop_strength
                };
                let bout = genkaba::play(&mut game.rng.daily, row.strength, shop, &eco);
                let paid = total_value * bout.multiplier;
                genkaba_paid += paid;
                district_receipts[row.district] += paid + support * hh;
                // 双方が時間と疲労を消費する
                life.condition.fatigue =
                    (life.condition.fatigue + eco.genkaba_fatigue).clamp(0.0, 1.5);
            }
        }
        life.last_pay = method;
    }

    // ── 在庫の更新 ──
    let spoil = game.world.stock_necessity.max(0.0) * eco.spoil_rate;
    game.world.stock_necessity = (game.world.stock_necessity - consumed_n - spoil).max(0.0);
    let perished = (game.world.stock_service - consumed_s).max(0.0);
    game.world.stock_service = 0.0;
    game.world.agency_capacity = agency_left;

    {
        let l = &mut game.ledger;
        l.consumed_necessity = consumed_n;
        l.consumed_service = consumed_s;
        l.spoiled_necessity = spoil;
        l.perished_service = perished;
        l.obligation_incurred += incurred;
        l.genkaba_paid += genkaba_paid;
        l.receipt_value += receipt_value;
    }

    // 支援は公費から出る。枠が尽きれば支援は続かない（FR-BUD-05）。
    if support_paid > 0.0 {
        let field = BudgetField::Housing;
        let avail = game.treasury.remaining(field).max(0.0);
        let pay = support_paid.min(avail);
        game.treasury.commit(field, pay);
        if pay + 1e-3 < support_paid {
            // 枠が足りなければ支援は縮小する
            for d in game.world.districts.iter_mut() {
                d.infra.housing_support *= 0.9;
                d.infra.livelihood_support *= 0.9;
            }
        }
    }

    // ── 事業者の収支（FR-ECO-04） ──
    update_businesses(game, &district_receipts, &district_units, &eco);

    // ── 組合の準備と利用枠（FR-ECO-02） ──
    update_guild(game, incurred, &eco);
    refresh_credit_limits(game, &eco);
}

fn collect_rows(world: &World, eco: &super::defs::EconomyParams) -> Vec<Row> {
    let mut rows = Vec::new();
    for (di, d) in world.districts.iter().enumerate() {
        for (ci, c) in d.cohorts.iter().enumerate() {
            rows.push(make_row(world, Who::Cohort(di, ci), di, c.headcount as f32, &c.life, eco));
        }
    }
    for (pi, p) in world.people.iter().enumerate() {
        if p.status == super::world::PersonStatus::Emigrated {
            continue;
        }
        rows.push(make_row(world, Who::Person(pi), p.home.index(), 1.0, &p.life, eco));
    }
    rows
}

fn make_row(
    world: &World,
    who: Who,
    district: usize,
    weight: f32,
    life: &Life,
    eco: &super::defs::EconomyParams,
) -> Row {
    let strength = teams::household_strength(world, life);
    // 請求力: 強いチームを持ち、利用枠の大きい世帯ほど多く取れる
    let raw = 1.0 + strength / 25.0 + life.credit_limit / world.guild.credit_base.max(0.1);
    Row {
        who,
        district,
        weight,
        need_n: eco.need_necessity,
        need_s: eco.need_service * (0.3 + 0.7 * life.life_slack),
        claim: raw.powf(eco.claim_power_exponent),
        strength,
        can_fight: life.can_fight(),
        household_size: life.household.size,
        alloc_n: 0.0,
        alloc_s: 0.0,
    }
}

fn allocate(rows: &mut [Row], stock_n: f32, stock_s: f32, _eco: &super::defs::EconomyParams) {
    allocate_good(rows, stock_n, true);
    allocate_good(rows, stock_s, false);
}

fn allocate_good(rows: &mut [Row], available: f32, necessity: bool) {
    let need = |r: &Row| if necessity { r.need_n } else { r.need_s };
    let total_need: f32 = rows.iter().map(|r| need(r) * r.weight).sum();
    if total_need <= 0.0 {
        return;
    }
    if available >= total_need {
        for r in rows.iter_mut() {
            if necessity { r.alloc_n = r.need_n } else { r.alloc_s = r.need_s }
        }
        return;
    }

    // 不足時は請求力で按分する。ここが分配の集中が観測される場所（AC-07）。
    let weighted: f32 = rows.iter().map(|r| need(r) * r.weight * r.claim).sum();
    let mut given = 0.0;
    for r in rows.iter_mut() {
        let share = if weighted > 0.0 {
            available * (need(r) * r.weight * r.claim) / weighted
        } else {
            0.0
        };
        let per_capita = (share / r.weight.max(1e-6)).min(need(r));
        if necessity {
            r.alloc_n = per_capita
        } else {
            r.alloc_s = per_capita
        }
        given += per_capita * r.weight;
    }

    // 上限で余った分を、満たされていない需要へ配り直す
    let mut leftover = (available - given).max(0.0);
    for _ in 0..2 {
        if leftover <= 1e-4 {
            break;
        }
        let unmet: f32 = rows
            .iter()
            .map(|r| {
                let a = if necessity { r.alloc_n } else { r.alloc_s };
                (need(r) - a).max(0.0) * r.weight
            })
            .sum();
        if unmet <= 1e-6 {
            break;
        }
        let ratio = (leftover / unmet).min(1.0);
        let mut used = 0.0;
        for r in rows.iter_mut() {
            let a = if necessity { r.alloc_n } else { r.alloc_s };
            let add = (need(r) - a).max(0.0) * ratio;
            if necessity {
                r.alloc_n += add
            } else {
                r.alloc_s += add
            }
            used += add * r.weight;
        }
        leftover -= used;
    }
}

/// 事業者が引き渡せる量には限りがある。閉店が続けば、在庫があっても届かない。
fn cap_by_delivery(rows: &mut [Row], world: &World, eco: &super::defs::EconomyParams) {
    let n = world.districts.len();
    let mut cap = vec![0.0f32; n];
    for b in &world.businesses {
        if b.state != BusinessState::Closed {
            cap[b.district.index()] += b.scale * eco.delivery_per_scale;
        }
    }
    let mut demanded = vec![0.0f32; n];
    for r in rows.iter() {
        demanded[r.district] += (r.alloc_n + r.alloc_s) * r.weight;
    }
    for r in rows.iter_mut() {
        let d = r.district;
        if demanded[d] > cap[d] && demanded[d] > 0.0 {
            let k = cap[d] / demanded[d];
            r.alloc_n *= k;
            r.alloc_s *= k;
        }
    }
}

fn life_mut(world: &mut World, who: Who) -> &mut Life {
    match who {
        Who::Cohort(di, ci) => &mut world.districts[di].cohorts[ci].life,
        Who::Person(pi) => &mut world.people[pi].life,
    }
}

/// 店舗・生産者・カード組合は無限の体力を持たない（FR-ECO-04）。
fn update_businesses(
    game: &mut Game,
    receipts: &[f32],
    units: &[f32],
    eco: &super::defs::EconomyParams,
) {
    let n = game.world.districts.len();
    let mut scale_sum = vec![0.0f32; n];
    for b in &game.world.businesses {
        if b.state != BusinessState::Closed {
            scale_sum[b.district.index()] += b.scale;
        }
    }

    for i in 0..game.world.businesses.len() {
        let b = &game.world.businesses[i];
        if b.state == BusinessState::Closed {
            continue;
        }
        let di = b.district.index();
        let share = if scale_sum[di] > 0.0 { b.scale / scale_sum[di] } else { 0.0 };
        let revenue = receipts[di] * share;
        let handled = units[di] * share;
        // 仕入（中間投入）と常駐チームの維持費
        let procurement = handled * eco.value_necessity * eco.procurement_ratio;
        let team_cost = eco.shop_team_cost * b.scale;
        let balance = revenue - procurement - team_cost;

        let b = &mut game.world.businesses[i];
        b.daily_balance = balance;
        b.cum_balance += balance;
        if balance < 0.0 {
            b.deficit_days += 1;
        } else {
            b.deficit_days = b.deficit_days.saturating_sub(1);
        }

        if b.deficit_days >= eco.deficit_days_close {
            b.state = BusinessState::Closed;
            b.scale = 0.0;
        } else if b.deficit_days >= eco.deficit_days_shrink && b.state == BusinessState::Operating {
            b.state = BusinessState::Shrinking;
            b.scale = (b.scale * 0.8).max(0.1);
        } else if b.deficit_days == 0 && b.state == BusinessState::Shrinking {
            b.state = BusinessState::Operating;
        }
    }
}

fn update_guild(game: &mut Game, incurred: f32, eco: &super::defs::EconomyParams) {
    let public = game.world.public_team_capacity;
    let stock = game.world.stock_necessity * eco.value_necessity * eco.guild_reserve_rate;
    let support: f32 = game.treasury.remaining(BudgetField::PublicTeams).max(0.0);

    let outstanding = total_obligation(&game.world);
    let g = &mut game.world.guild;
    g.guaranteed += incurred;
    g.reserve = public + stock + support * 0.2;
    g.coverage = card::coverage(g);
    g.outstanding = outstanding;
}

fn refresh_credit_limits(game: &mut Game, eco: &super::defs::EconomyParams) {
    let guild = game.world.guild.clone();
    let strengths: Vec<(f32, f32)> = {
        let w = &game.world;
        let mut v = Vec::new();
        for d in &w.districts {
            for c in &d.cohorts {
                v.push(team_state(w, &c.life));
            }
        }
        for p in &w.people {
            v.push(team_state(w, &p.life));
        }
        v
    };

    let mut k = 0;
    for d in game.world.districts.iter_mut() {
        for c in d.cohorts.iter_mut() {
            let (s, slots) = strengths[k];
            k += 1;
            c.life.credit_limit = card::credit_limit(&guild, &c.life, s, slots, eco);
        }
    }
    for p in game.world.people.iter_mut() {
        let (s, slots) = strengths[k];
        k += 1;
        p.life.credit_limit = card::credit_limit(&guild, &p.life, s, slots, eco);
    }
}

fn team_state(world: &World, life: &Life) -> (f32, f32) {
    match life.team {
        Some(t) => {
            let team = world.team(t);
            (team.effective_strength() * team.access_share.max(0.05), team.spare_slots())
        }
        None => (teams::household_strength(world, life), 1.0),
    }
}

// ───────────────────────────── 週末の清算 ─────────────────────────────

/// 町内カバディ大会でのカード清算（FR-TIME-09 / FR-ECO-02, 03）。自動進行する。
pub fn weekly_settlement(game: &mut Game) {
    let defs: Arc<Defs> = Arc::clone(&game.defs);
    let eco = defs.balance.economy;
    let ctxs = context::build(&game.world);

    // 公的・共同体による肩代わりの原資（予算と公共チームの稼働枠）
    let budget_relief = game.treasury.remaining(BudgetField::PublicTeams).max(0.0);
    let mut relief_pool = budget_relief * 0.25 + game.world.public_team_capacity;
    let relief_start = relief_pool;

    let mut settled_total = 0.0;
    let mut relieved_total = 0.0;
    let mut unmet_households = 0.0;
    let mut all_households = 0.0;

    let states: Vec<(f32, f32)> = {
        let w = &game.world;
        let mut v = Vec::new();
        for d in &w.districts {
            for c in &d.cohorts {
                v.push(team_state(w, &c.life));
            }
        }
        for p in &w.people {
            v.push(team_state(w, &p.life));
        }
        v
    };

    let mut k = 0;
    let World { districts, people, .. } = &mut game.world;
    for (di, d) in districts.iter_mut().enumerate() {
        let venue = ctxs[di].infra.payment_venue;
        for c in d.cohorts.iter_mut() {
            let (s, slots) = states[k];
            k += 1;
            let hh = households(c.headcount as f32, c.life.household.size);
            all_households += hh;
            let power = card::clearing_power(s, slots, c.life.condition.factor(), venue);
            let avail = if hh > 0.0 { relief_pool / hh } else { 0.0 };
            let r = card::settle(&mut c.life, power, avail, &eco);
            settled_total += r.settled * hh;
            relieved_total += r.relieved * hh;
            relief_pool = (relief_pool - r.relieved * hh).max(0.0);
            if !r.met {
                unmet_households += hh;
            }
            c.life.condition.fatigue =
                (c.life.condition.fatigue + eco.weekend_fatigue).clamp(0.0, 1.5);
        }
    }
    for p in people.iter_mut() {
        let (s, slots) = states[k];
        k += 1;
        let hh = households(1.0, p.life.household.size);
        all_households += hh;
        let venue = ctxs[p.home.index()].infra.payment_venue;
        let power = card::clearing_power(s, slots, p.life.condition.factor(), venue);
        let avail = if hh > 0.0 { relief_pool / hh } else { 0.0 };
        let r = card::settle(&mut p.life, power, avail, &eco);
        settled_total += r.settled * hh;
        relieved_total += r.relieved * hh;
        relief_pool = (relief_pool - r.relieved * hh).max(0.0);
        if !r.met {
            unmet_households += hh;
        }
        p.life.condition.fatigue = (p.life.condition.fatigue + eco.weekend_fatigue).clamp(0.0, 1.5);
    }

    game.ledger.obligation_settled += settled_total;
    game.ledger.obligation_relieved += relieved_total;

    // 肩代わりに使った分は公共チーム・予算の実支出になる
    let used = (relief_start - relief_pool).max(0.0);
    let from_public = used.min(game.world.public_team_capacity);
    game.world.public_team_capacity -= from_public;
    let from_budget = used - from_public;
    if from_budget > 0.0 {
        game.treasury.commit(BudgetField::PublicTeams, from_budget);
    }

    let g = &mut game.world.guild;
    g.guaranteed = (g.guaranteed - settled_total - relieved_total).max(0.0);
    g.outstanding = unmet_households;
    let _ = all_households;
}

/// 地区ごとの決済負担（FR-STAT-02 経済分野）。
pub fn obligation_by_district(world: &World, d: DistrictId) -> f32 {
    let dd = world.district(d);
    let mut sum = 0.0;
    for c in &dd.cohorts {
        sum += c.life.obligation * households(c.headcount as f32, c.life.household.size);
    }
    sum
}

/// 受取の集中度（ジニ係数の簡易版）。0 が均等、1 が完全集中（FR-ECO-07 / AC-07）。
pub fn receipt_concentration(world: &World) -> f32 {
    let mut vals: Vec<(f32, f32)> = Vec::new();
    for d in &world.districts {
        for c in &d.cohorts {
            vals.push((c.life.receipt.necessity + c.life.receipt.service, c.headcount as f32));
        }
    }
    if vals.is_empty() {
        return 0.0;
    }
    vals.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let total_w: f32 = vals.iter().map(|v| v.1).sum();
    let total_v: f32 = vals.iter().map(|v| v.0 * v.1).sum();
    if total_v <= 0.0 || total_w <= 0.0 {
        return 0.0;
    }
    let mut cum_w = 0.0;
    let mut cum_v = 0.0;
    let mut area = 0.0;
    for (v, w) in vals {
        let prev_w = cum_w / total_w;
        let prev_v = cum_v / total_v;
        cum_w += w;
        cum_v += v * w;
        let nw = cum_w / total_w;
        let nv = cum_v / total_v;
        area += (nw - prev_w) * (nv + prev_v) / 2.0;
    }
    (1.0 - 2.0 * area).clamp(0.0, 1.0)
}

/// 1日の頭で在庫と負担を記録する（会計恒等式の期首値）。
pub fn open_books(game: &mut Game) {
    let obligation = total_obligation(&game.world);
    game.ledger.begin(
        game.world.stock_necessity,
        game.world.stock_service,
        game.world.stock_buildwork,
        obligation,
    );
}

/// 1日の終わりに期末値を記録する。
pub fn close_books(game: &mut Game) {
    let l = &mut game.ledger;
    l.stock_close_necessity = game.world.stock_necessity;
    l.stock_close_service = game.world.stock_service;
    l.stock_close_buildwork = game.world.stock_buildwork;
    l.obligation_close = total_obligation(&game.world);
}

/// 公共・代理の稼働枠を日次で補充する（予算の実資源としての供給）。
pub fn refill_capacity(game: &mut Game) {
    let rate = game.defs.balance.budget.public_team_rate;
    let alloc = game.treasury.remaining(BudgetField::PublicTeams).max(0.0);
    game.world.public_team_capacity = game.world.public_team_base + alloc * rate / 360.0;
    game.world.agency_capacity = game.world.agency_base;
}

/// 地区の事業者の状態（UI の内訳表示用）。
pub fn business_health(world: &World, d: DistrictId) -> (f32, usize, usize) {
    let mut cum = 0.0;
    let mut open = 0;
    let mut closed = 0;
    for b in world.businesses.iter().filter(|b| b.district == d) {
        cum += b.cum_balance;
        if b.state == BusinessState::Closed {
            closed += 1;
        } else {
            open += 1;
        }
    }
    (cum, open, closed)
}

/// 現カバに回っている世帯の割合（決済能力不足の観測指標）。
pub fn genkaba_share(world: &World) -> f32 {
    let mut n = 0.0;
    let mut g = 0.0;
    for d in &world.districts {
        for c in &d.cohorts {
            let w = c.headcount as f32;
            n += w;
            if c.life.last_pay == PayMethod::Genkaba {
                g += w;
            }
        }
    }
    if n > 0.0 { g / n } else { 0.0 }
}

/// 代替決済経路を使っている世帯の割合（FR-ECO-08 の観測）。
pub fn proxy_share(world: &World) -> f32 {
    let mut n = 0.0;
    let mut g = 0.0;
    for d in &world.districts {
        for c in &d.cohorts {
            let w = c.headcount as f32;
            n += w;
            if c.life.last_pay == PayMethod::Proxy {
                g += w;
            }
        }
    }
    if n > 0.0 { g / n } else { 0.0 }
}

#[allow(dead_code)]
fn unused(_: CohortId, _: Target, _: BusinessKind, _: DistrictContext) {}
