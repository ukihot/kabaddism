//! 回復（睡眠・栄養・医療・住環境）と負傷の経過
//!
//! 疲労は練習・仕事・現カバ決済・週末大会で蓄積し、ここで戻る（FR-POP-04）。
//! 高疲労の継続は成長効率を下げ、負傷確率を上げる（FR-POP-05）。負傷判定自体は events が持つ。

use std::sync::Arc;

use super::Game;
use super::context::{self, DistrictContext};
use super::defs::Defs;
use super::world::{Life, World};

pub fn recover(game: &mut Game) {
    let defs: Arc<Defs> = Arc::clone(&game.defs);
    let ctxs = context::build(&game.world);

    let World { districts, people, .. } = &mut game.world;

    for (di, d) in districts.iter_mut().enumerate() {
        for c in &mut d.cohorts {
            restore(&mut c.life, &ctxs[di], &defs);
        }
    }
    for p in people.iter_mut() {
        let di = p.home.index();
        restore(&mut p.life, &ctxs[di], &defs);

        // 負傷の経過。医療アクセスが高いほど早く復帰する（FR-NEG「診察・治療待ち」の裏返し）。
        if let Some(inj) = p.injury.as_mut() {
            let speed = 1.0 + ctxs[di].infra.medical;
            let step = speed.round().max(1.0) as u16;
            inj.days_remaining = inj.days_remaining.saturating_sub(step);
            if inj.days_remaining == 0 {
                p.injury = None;
            }
        }
    }
}

fn restore(life: &mut Life, ctx: &DistrictContext, defs: &Defs) {
    let e = &defs.balance.events;
    let eco = &defs.balance.economy;

    // 住環境（寮の設備を含む）が睡眠の質を決める
    let housing = (ctx.infra.housing * 0.7 + ctx.dormitory_quality * 0.3).clamp(0.0, 1.0);
    let sleep_recovery = life.time.sleep * e.recover_sleep * (0.6 + 0.4 * housing);
    let medical_recovery = e.recover_medical * ctx.infra.medical;
    life.condition.fatigue =
        (life.condition.fatigue - sleep_recovery - medical_recovery).clamp(0.0, 1.5);

    // 栄養は「実際に受け取った量」で決まる。生産量ではない（FR-ECO-07）。
    let need = eco.need_necessity.max(0.01);
    let fill = (life.receipt.necessity / need).clamp(0.0, 1.2);
    let food_env = (ctx.infra.food * 0.4
        + (ctx.canteen_capacity / ctx.population.max(1.0)).min(1.0) * 0.2)
        .clamp(0.0, 0.6);
    let target_nutrition = (fill * 0.8 + food_env).clamp(0.0, 1.0);
    life.condition.nutrition += (target_nutrition - life.condition.nutrition) * 0.15;

    // 健康は栄養・医療・疲労の帰結
    let target_health = (0.45 + 0.3 * life.condition.nutrition + 0.25 * ctx.infra.medical
        - 0.3 * (life.condition.fatigue - 0.6).max(0.0))
    .clamp(0.05, 1.0);
    life.condition.health += (target_health - life.condition.health) * 0.08;
    life.condition.health = life.condition.health.clamp(0.05, 1.0);

    // 混雑の上乗せは日々ほどける
    life.pending_congestion = (life.pending_congestion * 0.8 - 1.0).max(0.0);
}
