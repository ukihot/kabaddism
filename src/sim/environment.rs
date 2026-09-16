//! 外部環境の変化（FR-SIM-02 ストリームC / FR-SIM-08）
//!
//! **変動の発生と、それを受け止める能力を分ける。** 天候・供給は乱数で動くが、
//! 実際の打撃は resilience（備蓄・代替手段）で緩衝される。

use super::world::Environment;
use super::{Game, rng};

pub fn update(game: &mut Game) -> Environment {
    let n = game.defs.balance.noise;
    let e = &mut game.world.environment;

    // 平均回帰つきのランダムウォーク。日ごとに跳ねすぎない。
    let w = rng::range(&mut game.rng.env, -n.weather_step, n.weather_step);
    e.weather = (e.weather + w + (0.7 - e.weather) * 0.05).clamp(0.05, 1.0);

    let s = rng::range(&mut game.rng.env, -n.supply_step, n.supply_step);
    e.supply = (e.supply + s + (0.7 - e.supply) * 0.05).clamp(0.05, 1.0);

    // 受け止める能力は在庫と公共の稼働枠から決まる（乱数では動かさない）
    let stock_cover = (game.world.stock_necessity / (game.world.population() as f32).max(1.0)).min(1.0);
    let public_cover = (game.world.public_team_base / 40.0).min(1.0);
    let e = &mut game.world.environment;
    e.resilience = (0.25 + 0.5 * stock_cover + 0.25 * public_cover).clamp(0.0, 1.0);

    *e
}

/// 変動 x（0..1、1が良い）を resilience で緩衝した実効係数。
pub fn buffered(x: f32, resilience: f32) -> f32 {
    (1.0 - (1.0 - x) * (1.0 - 0.7 * resilience)).clamp(0.1, 1.2)
}
