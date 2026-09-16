//! 育成（design.md §9 / FR-POP-06）
//!
//! 成長は「練習時間 × 指導の質 × 才能 × コンディション」を経由する。
//! **`Ability` を書き換えるのはこの関数と `events::apply`（負傷・離脱）だけ**（design.md §7）。

use std::sync::Arc;

use super::context::{self, DistrictContext};
use super::defs::Defs;
use super::world::{AgeBand, Life, World};
use super::{Game, rng};

pub fn run(game: &mut Game) {
    let defs: Arc<Defs> = Arc::clone(&game.defs);
    let ctxs = context::build(&game.world);
    let amp = defs.balance.noise.training;

    // 地区ごとの指導の質。在籍者が定員を超えると質が落ちる。
    let quality: Vec<f32> = (0..game.world.districts.len())
        .map(|di| {
            let enrolled: f32 = game
                .world
                .facilities
                .iter()
                .filter(|f| f.district.index() == di && f.kind == super::world::FacilityKind::Dojo)
                .map(|f| f.enrolled)
                .sum();
            ctxs[di].coaching_quality(enrolled, &defs)
        })
        .collect();

    let World { districts, people, .. } = &mut game.world;

    for (di, d) in districts.iter_mut().enumerate() {
        for c in &mut d.cohorts {
            let age = representative_age(c.age_band);
            grow(&mut c.life, age, quality[di], &ctxs[di], &defs, &mut game.rng.daily, amp);
        }
    }

    for p in people.iter_mut() {
        if !p.is_active() {
            continue;
        }
        let di = p.home.index();
        grow(&mut p.life, p.age as f32, quality[di], &ctxs[di], &defs, &mut game.rng.daily, amp);
    }
}

fn grow(
    life: &mut Life,
    age: f32,
    quality: f32,
    ctx: &DistrictContext,
    defs: &Defs,
    r: &mut rand_chacha::ChaCha8Rng,
    amp: f32,
) {
    let t = &defs.balance.training;
    let practice_h = life.time.practice / 60.0;
    let c = life.condition.factor();

    // 日常変動は成長量への乗算ノイズ。イベントの発生には使わない（design.md §9）。
    let n = rng::noise(r, amp);
    let growth = practice_h * quality * life.talent * c * t.growth_k * n;

    // 放置すれば下がる。高齢では減衰が成長を上回り、自然に引退へ向かう。
    let age_factor = 1.0 + (age - t.decay_age_start).max(0.0) * t.decay_age_slope;
    let decay = t.decay_k * life.ability.value * age_factor;

    life.ability.value = (life.ability.value + growth - decay).clamp(0.0, t.ability_cap);
    life.experience += practice_h * t.experience_rate;

    // コンディションが低い状態での練習は、成長が小さく疲労だけ増える
    life.condition.fatigue += practice_h * t.intensity * (2.0 - c).max(0.5) * t.fatigue_k;

    // 練習できない日が続くと意欲が下がり、できた日は上がる
    let target = if practice_h > 0.15 { 0.85 } else { 0.35 };
    life.motivation += (target - life.motivation) * 0.01;

    // 生活余力と参加機会が意欲を支える
    life.motivation += (life.life_slack - 0.5) * 0.002 + (ctx.dojo_openness - 0.5) * 0.001;
    life.motivation = life.motivation.clamp(0.05, 1.3);
}

pub fn representative_age(band: AgeBand) -> f32 {
    match band {
        AgeBand::Child => 12.0,
        AgeBand::Youth => 20.0,
        AgeBand::Prime => 30.0,
        AgeBand::Middle => 45.0,
        AgeBand::Senior => 68.0,
    }
}

/// 国民平均カバディ能力（FR-STAT-01）。自国の地区のみ。
pub fn mean_ability(world: &World, home: super::ids::NationId) -> f32 {
    let mut num = 0.0;
    let mut den = 0.0;
    for (_, d) in world.owned_districts(home) {
        for c in &d.cohorts {
            num += c.headcount as f32 * c.life.ability.value;
            den += c.headcount as f32;
        }
    }
    if den > 0.0 { num / den } else { 0.0 }
}

/// 基準能力達成率（FR-STAT-02 育成分野）。
pub fn baseline_rate(world: &World, home: super::ids::NationId, baseline: f32) -> f32 {
    let mut ok = 0.0;
    let mut all = 0.0;
    for (_, d) in world.owned_districts(home) {
        for c in &d.cohorts {
            let n = c.headcount as f32;
            all += n;
            // 平均と分散から、基準を超える割合を線形近似する
            let spread = c.life.ability.spread.max(0.1);
            let z = (c.life.ability.value - baseline) / spread;
            ok += n * (0.5 + 0.5 * z.clamp(-1.0, 1.0));
        }
    }
    if all > 0.0 { ok / all } else { 0.0 }
}

// ───────────────────────────── 世代交代 ─────────────────────────────

/// 加齢・引退・復帰（FR-POP-07 / FR-POP-09）。
/// v1 では出生・死亡は扱わず、年齢加算と引退のみ。
pub fn lifecycle(game: &mut Game) {
    for i in 0..game.world.people.len() {
        let p = &mut game.world.people[i];
        match p.status {
            super::world::PersonStatus::Active => {
                // 能力が落ちきった高齢選手は引退する
                if p.age >= 33 && p.life.ability.value < 25.0 {
                    p.status = super::world::PersonStatus::Retired;
                    p.life.occupation = super::world::Occupation::Retired;
                }
            }
            super::world::PersonStatus::Paused => {
                // 生活余力が戻れば復帰する（対応策は一つではない）
                if p.life.life_slack > 0.45 && p.life.condition.fatigue < 0.6 {
                    p.status = super::world::PersonStatus::Active;
                    p.life.motivation = (p.life.motivation + 0.2).min(1.0);
                }
            }
            _ => {}
        }
    }
}

/// 年に一度の加齢。
pub fn age_everyone(game: &mut Game) {
    for p in game.world.people.iter_mut() {
        p.age = p.age.saturating_add(1);
    }
}
