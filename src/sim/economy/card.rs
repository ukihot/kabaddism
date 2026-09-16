//! カバディカード（design.md §8.3 / FR-ECO-02, 03）
//!
//! 利用時に負担 (Obligation) を K建てで累積し、週末大会での清算力と突き合わせる。
//! 清算不足は繰り越し、利用枠を縮小する。回復手段は2系統（労務 / 公的・共同体）。

use crate::sim::defs::EconomyParams;
use crate::sim::world::{CardGuild, Life};

/// 利用枠 = 基準 × 契約チーム戦力係数 × 出場余力係数 × 履行実績係数
pub fn credit_limit(
    guild: &CardGuild,
    life: &Life,
    team_strength: f32,
    spare_slots: f32,
    eco: &EconomyParams,
) -> f32 {
    let team_factor = (team_strength / 25.0).clamp(eco.credit_team_min, eco.credit_team_max);
    let availability = (spare_slots / 2.0).clamp(0.0, 1.0);
    let record = (eco.credit_record_min
        + (eco.credit_record_max - eco.credit_record_min) * life.settlement_record.clamp(0.0, 1.0))
    .clamp(eco.credit_record_min, eco.credit_record_max);
    (guild.credit_base * team_factor * (0.25 + 0.75 * availability) * record).max(0.0)
}

/// 週末の清算結果。
#[derive(Clone, Copy, Debug, Default)]
pub struct Settlement {
    /// 自力・労務で清算した額（世帯あたり K）
    pub settled: f32,
    /// 公的・共同体が肩代わりした額（世帯あたり K）
    pub relieved: f32,
    /// 繰り越した未清算額（世帯あたり K）
    pub carried: f32,
    /// 要求率を満たしたか
    pub met: bool,
}

/// 清算力 = Σ(出場チームの戦力 × 出場枠 × コンディション) × 会場係数
pub fn clearing_power(team_strength: f32, slots: f32, condition: f32, venue: f32) -> f32 {
    team_strength * slots.clamp(0.0, 4.0) * condition.clamp(0.0, 1.2) * (0.6 + 0.4 * venue)
}

/// 清算を行い、利用枠と履行実績を更新する。
pub fn settle(
    life: &mut Life,
    power: f32,
    relief_available: f32,
    eco: &EconomyParams,
) -> Settlement {
    let before = life.obligation;
    if before <= 1e-6 {
        life.settlement_record = (life.settlement_record * 0.75 + 0.25).min(1.0);
        life.labor_credit = 0.0;
        return Settlement { met: true, ..Default::default() };
    }

    // 回復手段1: 労務による直接清算（余暇を労働に振り替えた分）
    let own = power * eco.settlement_rate + life.labor_credit;
    let settled = own.min(before);
    let mut remaining = before - settled;

    // 回復手段2: 公的・共同体による肩代わり
    let relieved = remaining.min(relief_available).min(eco.public_relief_cap).max(0.0);
    remaining -= relieved;

    life.obligation = remaining;
    life.labor_credit = 0.0;

    let ratio = ((settled + relieved) / before).clamp(0.0, 1.0);
    let met = ratio + 1e-4 >= eco.required_settlement_ratio;
    if !met {
        life.credit_limit *= eco.credit_shrink;
    }
    life.settlement_record = (life.settlement_record * 0.75 + ratio * 0.25).clamp(0.0, 1.0);

    Settlement { settled, relieved, carried: remaining, met }
}

/// 組合の準備率。保証額に対する裏付けの比率（FR-ECO-04）。
pub fn coverage(guild: &CardGuild) -> f32 {
    if guild.guaranteed <= 1e-6 { 1.0 } else { (guild.reserve / guild.guaranteed).clamp(0.0, 4.0) }
}
