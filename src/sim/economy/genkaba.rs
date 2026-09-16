//! 現カバ決済（design.md §8.2 / FR-ECO-01）
//!
//! 確定事項:
//! - 商品・サービスは**勝敗にかかわらず引き渡される**。決済が成立しない状態を作らない。
//! - 勝敗が決めるのは消費者側の支払負担の倍率（勝利=軽減、敗北=加重）。
//! - 双方が時間と疲労を必ず消費する。
//! - 負担は店舗側の受取に加算される。生産は増えない。

use rand_chacha::ChaCha8Rng;

use crate::sim::defs::EconomyParams;
use crate::sim::rng;

#[derive(Clone, Copy, Debug)]
pub struct Bout {
    pub p_win: f32,
    pub won: bool,
    /// 支払負担の倍率
    pub multiplier: f32,
}

/// 実効戦力の二乗比。強さの差が素直に効き、同戦力で 0.5 になる。
pub fn win_probability(consumer: f32, shop: f32) -> f32 {
    let c = consumer.max(0.01);
    let s = shop.max(0.01);
    (c * c) / (c * c + s * s)
}

pub fn play(r: &mut ChaCha8Rng, consumer: f32, shop: f32, eco: &EconomyParams) -> Bout {
    let p = win_probability(consumer, shop);
    let won = rng::chance(r, p);
    Bout {
        p_win: p,
        won,
        multiplier: if won { eco.genkaba_win_mult } else { eco.genkaba_lose_mult },
    }
}

/// 期待負担倍率。balance 調整時に 1.0 近傍かを確認するために使う。
pub fn expected_multiplier(p_win: f32, eco: &EconomyParams) -> f32 {
    p_win * eco.genkaba_win_mult + (1.0 - p_win) * eco.genkaba_lose_mult
}
