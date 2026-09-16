//! 年次予算編成（design.md §12.1 / FR-BUD-*）
//!
//! 予算は K建ての**枠**であり、実行時点で必要な実資源（人員・物資・公共チーム稼働）が
//! 確保できるとは限らない。枠と実資源は別々に判定する（FR-BUD-05）。

use serde::{Deserialize, Serialize};

use super::defs::{BudgetField, StaffRole};

/// 実資源。予算枠とは別に管理する。
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct Resources {
    /// 役割別の人員総数（StaffRole の添字順）
    pub staff_total: [f32; 5],
    /// 施設・事業へ配置済みの人員
    pub staff_assigned: [f32; 5],
    /// 公共チームの本年度稼働枠（K相当）
    pub public_team_capacity: f32,
    /// 資材・建設能力の備蓄
    pub materials: f32,
}

impl StaffRole {
    pub fn index(self) -> usize {
        match self {
            StaffRole::Coach => 0,
            StaffRole::Medic => 1,
            StaffRole::Nursery => 2,
            StaffRole::Official => 3,
            StaffRole::Builder => 4,
        }
    }
    pub const ALL: [StaffRole; 5] = [
        StaffRole::Coach,
        StaffRole::Medic,
        StaffRole::Nursery,
        StaffRole::Official,
        StaffRole::Builder,
    ];
}

impl Resources {
    pub fn available(&self, role: StaffRole) -> f32 {
        (self.staff_total[role.index()] - self.staff_assigned[role.index()]).max(0.0)
    }
    pub fn assign(&mut self, role: StaffRole, amount: f32) {
        self.staff_assigned[role.index()] += amount;
    }
    pub fn release(&mut self, role: StaffRole, amount: f32) {
        let i = role.index();
        self.staff_assigned[i] = (self.staff_assigned[i] - amount).max(0.0);
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Treasury {
    /// 分野別の年度枠（K）
    pub allocations: [f32; 8],
    pub spent: [f32; 8],
    /// 既存事業の継続費用（K/年）
    pub committed_upkeep: f32,
    /// 次年度の収入見込み（K）
    pub revenue_estimate: f32,
    /// 前年度からの赤字持ち越し（K）
    pub carried_deficit: f32,
    /// 本年度に実際に発生した歳入（K）
    pub revenue_accrued: f32,
    /// 本年度に支払った継続費用（K）
    pub upkeep_paid: f32,
    pub real: Resources,
}

impl Treasury {
    pub fn remaining(&self, field: BudgetField) -> f32 {
        self.allocations[field.index()] - self.spent[field.index()]
    }

    pub fn total_allocated(&self) -> f32 {
        self.allocations.iter().sum()
    }

    pub fn total_spent(&self) -> f32 {
        self.spent.iter().sum()
    }

    /// 枠から引き当てる。枠が足りなければ false（実資源の判定は別: FR-BUD-05）。
    pub fn commit(&mut self, field: BudgetField, amount: f32) -> bool {
        if self.remaining(field) + 1e-4 < amount {
            return false;
        }
        self.spent[field.index()] += amount;
        true
    }

    pub fn refund(&mut self, field: BudgetField, amount: f32) {
        let i = field.index();
        self.spent[i] = (self.spent[i] - amount).max(0.0);
    }

    /// 収支見込み（FR-STAT-01 の常時表示）。
    pub fn balance_forecast(&self) -> f32 {
        self.revenue_estimate - self.total_allocated() - self.committed_upkeep - self.carried_deficit
    }
}

/// 予算編成画面に渡す提示内容（FR-BUD-02）。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BudgetBriefing {
    pub year: u16,
    /// 次年度の収入見込み
    pub revenue_estimate: f32,
    /// 既存事業の継続費用
    pub committed_upkeep: f32,
    /// 前年度からの赤字持ち越し
    pub carried_deficit: f32,
    /// 前年度の実績
    pub last_revenue: f32,
    pub last_spent: f32,
    /// 現在の実資源
    pub resources: Resources,
    /// 前年度の配分（初期値として提示する）
    pub previous: [f32; 8],
}

impl BudgetBriefing {
    /// 配分可能額。これを超える配分も**許可する**（FR-BUD-06）。結果は翌年度に現れる。
    pub fn allocatable(&self) -> f32 {
        (self.revenue_estimate - self.committed_upkeep - self.carried_deficit).max(0.0)
    }
}
