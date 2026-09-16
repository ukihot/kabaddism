//! 暦（FR-TIME-08）
//!
//! 1年 = 360日（12ヶ月 × 30日）。週末は7日周期。
//! 予算編成日と世界大会日は年内の別日に固定配置する。

use serde::{Deserialize, Serialize};

pub const DAYS_PER_MONTH: u16 = 30;
pub const MONTHS_PER_YEAR: u16 = 12;
pub const DAYS_PER_YEAR: u16 = DAYS_PER_MONTH * MONTHS_PER_YEAR;

/// ゲーム内日付。`doy` は 1..=360。
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub struct Date {
    pub year: u16,
    pub doy: u16,
}

impl Default for Date {
    fn default() -> Self {
        Date { year: 1, doy: 1 }
    }
}

impl Date {
    pub fn new(year: u16, doy: u16) -> Self {
        debug_assert!((1..=DAYS_PER_YEAR).contains(&doy));
        Date { year, doy }
    }

    /// 翌日。年をまたぐ。
    pub fn next(self) -> Self {
        if self.doy >= DAYS_PER_YEAR {
            Date { year: self.year + 1, doy: 1 }
        } else {
            Date { year: self.year, doy: self.doy + 1 }
        }
    }

    pub fn month(self) -> u16 {
        (self.doy - 1) / DAYS_PER_MONTH + 1
    }

    pub fn day(self) -> u16 {
        (self.doy - 1) % DAYS_PER_MONTH + 1
    }

    /// 7日周期の週末（FR-TIME-09）。町内カバディ大会の開催日。
    pub fn is_weekend(self) -> bool {
        self.doy % 7 == 0
    }

    /// 通算日数。指標の推移や履歴の並べ替えに使う。
    pub fn absolute(self) -> u32 {
        (self.year as u32 - 1) * DAYS_PER_YEAR as u32 + self.doy as u32
    }

    /// この日から見て、次に `doy` が来るまでの日数。当日なら 0。
    pub fn days_until(self, doy: u16) -> u16 {
        if doy >= self.doy {
            doy - self.doy
        } else {
            DAYS_PER_YEAR - self.doy + doy
        }
    }
}

impl std::fmt::Display for Date {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}年{}月{}日", self.year, self.month(), self.day())
    }
}

/// 年次イベントの配置（データ駆動: balance.ron）。
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct CalendarDefs {
    /// 予算編成日（年内 doy）
    pub budget_doy: u16,
    /// 世界大会日（年内 doy）
    pub cup_doy: u16,
}

impl Default for CalendarDefs {
    fn default() -> Self {
        CalendarDefs { budget_doy: 60, cup_doy: 330 }
    }
}

impl CalendarDefs {
    /// その日に到達したら進行を停止すべきか（FR-TIME-05）。
    pub fn stop_reason(&self, date: Date) -> Option<StopReason> {
        if date.doy == self.budget_doy {
            Some(StopReason::Budget)
        } else if date.doy == self.cup_doy {
            Some(StopReason::Cup)
        } else {
            None
        }
    }
}

/// 進行を止める理由。年次イベントのみ（FR-TIME-07）。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum StopReason {
    Budget,
    Cup,
}
