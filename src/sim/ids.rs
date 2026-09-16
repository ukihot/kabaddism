//! 型付き ID（design.md §4.3）
//!
//! いずれも対応する `Vec` の添字。エンティティは削除せず `status` で無効化するため、
//! ID はセーブ・ニュース参照をまたいで安定する。

use serde::{Deserialize, Serialize};

macro_rules! id_type {
    ($(#[$m:meta])* $name:ident) => {
        $(#[$m])*
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
        pub struct $name(pub u16);

        impl $name {
            #[inline]
            pub fn index(self) -> usize {
                self.0 as usize
            }
            #[inline]
            pub fn from_index(i: usize) -> Self {
                debug_assert!(i <= u16::MAX as usize);
                $name(i as u16)
            }
        }
    };
}

id_type!(/// 地区
    DistrictId);
id_type!(/// 施設
    FacilityId);
id_type!(/// チーム
    TeamId);
id_type!(/// 追跡人物
    PersonId);
id_type!(/// 国家（自国 + 対戦国）
    NationId);
id_type!(/// 事業者（店舗・生産者）
    BusinessId);
id_type!(/// 進行中の事業
    ProjectId);
id_type!(/// 問題（発生してから解決するまで）
    IssueId);
id_type!(/// ニュースの続報スレッド
    ThreadId);

/// コホートは地区に属するため、地区 ID と地区内添字の組で指す。
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub struct CohortId {
    pub district: DistrictId,
    pub index: u16,
}

impl CohortId {
    pub fn new(district: DistrictId, index: usize) -> Self {
        CohortId { district, index: index as u16 }
    }
}

/// ニュース見出しからの遷移先（FR-NEWS-03）。
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub enum Subject {
    District(DistrictId),
    Facility(FacilityId),
    Person(PersonId),
    Team(TeamId),
    Business(BusinessId),
    Cohort(CohortId),
    Project(ProjectId),
    Nation(NationId),
}

/// イベント判定・効果適用の対象（design.md §11.1 Scope）。
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub enum Target {
    District(DistrictId),
    Facility(FacilityId),
    Person(PersonId),
    Cohort(CohortId),
    Business(BusinessId),
    Nation,
}

impl Target {
    pub fn district(self) -> Option<DistrictId> {
        match self {
            Target::District(d) => Some(d),
            Target::Cohort(c) => Some(c.district),
            _ => None,
        }
    }

    pub fn as_subject(self) -> Subject {
        match self {
            Target::District(d) => Subject::District(d),
            Target::Facility(f) => Subject::Facility(f),
            Target::Person(p) => Subject::Person(p),
            Target::Cohort(c) => Subject::Cohort(c),
            Target::Business(b) => Subject::Business(b),
            Target::Nation => Subject::Nation(NationId(0)),
        }
    }
}
