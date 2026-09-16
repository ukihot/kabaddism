//! K建て会計と恒等式チェック（design.md §8.1 / FR-ECO-06）
//!
//! K は評価単位であって通貨ではない。勝敗による所有移転も、カードの負担累積・清算も
//! **GDP に一切入らない**。移転取引であって生産ではないため。
//!
//! `identities_hold` は debug ビルドで毎日呼ばれ、経済が発散する前に落とす。

use serde::{Deserialize, Serialize};

/// その日の実物フローと決済フロー。
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct Ledger {
    // ── 実物（単位） ──
    pub stock_open_necessity: f32,
    pub stock_open_service: f32,
    pub stock_open_buildwork: f32,
    pub produced_necessity: f32,
    pub produced_service: f32,
    pub produced_buildwork: f32,
    /// 中間投入として消えた必需品
    pub intermediate_necessity: f32,
    /// 世帯が受け取った量
    pub consumed_necessity: f32,
    pub consumed_service: f32,
    /// 事業の工事に投入された建設仕事
    pub used_buildwork: f32,
    /// 腐敗・陳腐化（サービスは在庫できない）
    pub perished_service: f32,
    /// 必需品の劣化
    pub spoiled_necessity: f32,
    /// 使われずに失われた建設仕事（労働は貯められない）
    pub expired_buildwork: f32,
    pub stock_close_necessity: f32,
    pub stock_close_service: f32,
    pub stock_close_buildwork: f32,

    // ── 決済（K） ──
    pub obligation_open: f32,
    pub obligation_incurred: f32,
    pub obligation_settled: f32,
    /// 公的・共同体による肩代わり分（清算に含む）
    pub obligation_relieved: f32,
    pub obligation_close: f32,
    /// 現カバでその場で受け渡された負担
    pub genkaba_paid: f32,

    // ── 集計 ──
    /// 実質 GDP（標準価値固定）
    pub gdp: f32,
    /// 受取の総額（K）。生産とは別指標（FR-ECO-07）
    pub receipt_value: f32,
}

impl Ledger {
    pub fn begin(
        &mut self,
        stock_necessity: f32,
        stock_service: f32,
        stock_buildwork: f32,
        obligation: f32,
    ) {
        *self = Ledger::default();
        self.stock_open_necessity = stock_necessity;
        self.stock_open_service = stock_service;
        self.stock_open_buildwork = stock_buildwork;
        self.obligation_open = obligation;
    }
}

fn close_enough(a: f32, b: f32) -> bool {
    let scale = a.abs().max(b.abs()).max(1.0);
    (a - b).abs() <= 1e-3 * scale
}

/// 会計の恒等式（design.md §8.1）。
///
/// ```text
/// 生産 = 消費 + 在庫変化                        （実物）
/// Σ負担の増加 = Σ負担の清算 + Σ未清算残高の増加   （決済）
/// ```
pub fn identities_hold(l: &Ledger) -> Result<(), String> {
    // 実物: 期首在庫 + 生産 = 消費（中間投入含む） + 期末在庫
    let lhs_n = l.stock_open_necessity + l.produced_necessity;
    let rhs_n = l.consumed_necessity
        + l.intermediate_necessity
        + l.spoiled_necessity
        + l.stock_close_necessity;
    if !close_enough(lhs_n, rhs_n) {
        return Err(format!("必需品の実物収支が合わない: {lhs_n} != {rhs_n}"));
    }

    let lhs_s = l.stock_open_service + l.produced_service;
    let rhs_s = l.consumed_service + l.perished_service + l.stock_close_service;
    if !close_enough(lhs_s, rhs_s) {
        return Err(format!("サービスの実物収支が合わない: {lhs_s} != {rhs_s}"));
    }

    let lhs_b = l.stock_open_buildwork + l.produced_buildwork;
    let rhs_b = l.used_buildwork + l.expired_buildwork + l.stock_close_buildwork;
    if !close_enough(lhs_b, rhs_b) {
        return Err(format!("建設仕事の実物収支が合わない: {lhs_b} != {rhs_b}"));
    }

    // 決済: 期首負担 + 発生 − 清算 = 期末負担
    let lhs_o = l.obligation_open + l.obligation_incurred;
    let rhs_o = l.obligation_settled + l.obligation_relieved + l.obligation_close;
    if !close_enough(lhs_o, rhs_o) {
        return Err(format!("決済負担の収支が合わない: {lhs_o} != {rhs_o}"));
    }

    // 非負であること（負担が無限に負へ回らない）
    if l.obligation_close < -1e-3 {
        return Err(format!("未清算残高が負: {}", l.obligation_close));
    }
    Ok(())
}
