//! 決定論的乱数（design.md §6 / FR-SIM-02 / NFR-01）
//!
//! ストリームを用途別に分けるのは、片方の消費回数の変化がもう片方の結果を変えないようにするため。
//! イベントが何件発生しても天候は変わらない。

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RngSet {
    /// A: 日常の小さな変動（売上・参加人数・練習成果）
    pub daily: ChaCha8Rng,
    /// B: 条件付きイベント
    pub events: ChaCha8Rng,
    /// C: 外部環境（天候・供給・他国動向）
    pub env: ChaCha8Rng,
    /// D: 大会の試合結果
    pub cup: ChaCha8Rng,
    /// 再現用に保持する元シード
    pub seed: u64,
}

impl RngSet {
    pub fn new(seed: u64) -> Self {
        RngSet {
            daily: ChaCha8Rng::seed_from_u64(seed ^ 0x0000_00A1_0000_00A1),
            events: ChaCha8Rng::seed_from_u64(seed ^ 0x0000_00B2_0000_00B2),
            env: ChaCha8Rng::seed_from_u64(seed ^ 0x0000_00C3_0000_00C3),
            cup: ChaCha8Rng::seed_from_u64(seed ^ 0x0000_00D4_0000_00D4),
            seed,
        }
    }
}

/// 1.0 を中心とする乗算ノイズ。`amp` が振れ幅（0.1 なら ±10%）。
///
/// FR-SIM-09: 日次ノイズは政策の方向性を覆い隠さない幅に留める。
/// 振れ幅は balance.ron の明示パラメータから渡す（テストで信号対雑音比を測れるように）。
pub fn noise(rng: &mut ChaCha8Rng, amp: f32) -> f32 {
    if amp <= 0.0 {
        return 1.0;
    }
    1.0 + rng.random_range(-amp..amp)
}

/// 確率 `p` の抽選。
pub fn chance(rng: &mut ChaCha8Rng, p: f32) -> bool {
    if p <= 0.0 {
        return false;
    }
    if p >= 1.0 {
        return true;
    }
    rng.random::<f32>() < p
}

/// 一様乱数 [lo, hi)。lo == hi なら lo。
pub fn range(rng: &mut ChaCha8Rng, lo: f32, hi: f32) -> f32 {
    if hi <= lo { lo } else { rng.random_range(lo..hi) }
}
