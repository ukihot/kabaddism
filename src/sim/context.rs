//! 地区ごとの環境スナップショット
//!
//! 時間配分・育成・経済の各関数は、`World` を可変に触る前にこのスナップショットを作ってから走る。
//! これにより「地区の状態を読みながら住民を書き換える」borrow の衝突を避けつつ、
//! **コホートと追跡人物が同一の入力から同一の関数を通る**ことを保証する（FR-POP-01）。

use super::defs::Defs;
use super::ids::DistrictId;
use super::world::{FacilityKind, Infra, World};

#[derive(Clone, Copy, Debug)]
pub struct DistrictContext {
    pub infra: Infra,
    /// 道場の実効定員（人）
    pub dojo_capacity: f32,
    /// 利用権の開放度 0..1（誰が使えるか: FR-TOWN-04）
    pub dojo_openness: f32,
    /// 夜間に開いている道場の割合 0..1
    pub dojo_night_share: f32,
    /// 指導者の配置人数
    pub coach_staff: f32,
    /// 託児所の実効定員
    pub nursery_capacity: f32,
    /// 病院の実効定員
    pub hospital_capacity: f32,
    /// 食堂の実効定員
    pub canteen_capacity: f32,
    /// 選手寮の設備の質 0..1
    pub dormitory_quality: f32,
    /// 決済会場の実効定員
    pub venue_capacity: f32,
    /// 交通施設による通勤短縮 0..1
    pub transit_facility: f32,
    /// 住民数
    pub population: f32,
    /// 平均移動コスト（分）
    pub mean_distance: f32,
    /// 地区内の店舗チームの平均実効戦力
    pub shop_strength: f32,
    /// 高級店チームの平均実効戦力
    pub luxury_strength: f32,
    /// 住民共同・公共チームの利用権の広がり 0..1
    pub shared_team_access: f32,
    /// 住民共同・公共チームの平均実効戦力
    pub shared_team_strength: f32,
}

impl DistrictContext {
    /// 練習に「通えるか・開いているか・利用権があるか」の積（design.md §7）。
    pub fn participation_possibility(&self, night_worker: bool) -> f32 {
        let reachable = (0.35 + 0.65 * self.infra.transit).min(1.0)
            * (1.0 - (self.mean_distance / 240.0).min(0.6));
        let open = if night_worker {
            (self.infra.night_access * 0.6 + self.dojo_night_share * 0.4).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let has_dojo = if self.dojo_capacity > 0.0 { 1.0 } else { 0.0 };
        (reachable * open * self.dojo_openness * has_dojo).clamp(0.0, 1.0)
    }

    /// 指導の質 q = 指導者数 × 受持可能数 / 在籍者数（design.md §9）
    pub fn coaching_quality(&self, enrolled: f32, defs: &Defs) -> f32 {
        let t = &defs.balance.training;
        if enrolled <= 0.0 {
            return 1.0;
        }
        let seats = self.coach_staff * t.coach_ratio;
        (seats / enrolled).clamp(t.quality_floor, 1.2)
    }
}

pub fn build(world: &World) -> Vec<DistrictContext> {
    (0..world.districts.len())
        .map(|i| build_one(world, DistrictId::from_index(i)))
        .collect()
}

fn build_one(world: &World, id: DistrictId) -> DistrictContext {
    let d = world.district(id);

    let mut dojo_capacity = 0.0;
    let mut dojo_weight = 0.0;
    let mut dojo_openness_acc = 0.0;
    let mut dojo_night_acc = 0.0;
    let mut coach_staff = 0.0;
    for (_, f) in world.facilities_in(id, FacilityKind::Dojo) {
        if !f.is_running() {
            continue;
        }
        let cap = f.capacity * f.staffing().min(1.0) * (0.6 + 0.4 * f.quality);
        dojo_capacity += cap;
        dojo_weight += cap;
        dojo_openness_acc += cap * f.access.openness();
        dojo_night_acc += cap * if f.night_open { 1.0 } else { 0.0 };
        coach_staff += f.staff;
    }
    // 学校の夜間開放は道場の代替枠になる
    for (_, f) in world.facilities_in(id, FacilityKind::School) {
        if f.is_running() && f.night_open {
            let cap = f.capacity * 0.4;
            dojo_capacity += cap;
            dojo_weight += cap;
            dojo_openness_acc += cap * f.access.openness();
            dojo_night_acc += cap;
        }
    }
    let dojo_openness = if dojo_weight > 0.0 { dojo_openness_acc / dojo_weight } else { 0.0 };
    let dojo_night_share = if dojo_weight > 0.0 { dojo_night_acc / dojo_weight } else { 0.0 };

    let dormitory_quality = {
        let mut q = 0.0;
        let mut n = 0.0;
        for (_, f) in world.facilities_in(id, FacilityKind::Dormitory) {
            if f.is_running() {
                q += f.quality;
                n += 1.0;
            }
        }
        if n > 0.0 { q / n } else { 0.0 }
    };

    let transit_facility = {
        let mut q: f32 = 0.0;
        for (_, f) in world.facilities_in(id, FacilityKind::Transit) {
            if f.is_running() {
                q += 0.25 * f.quality * f.staffing().min(1.0);
            }
        }
        q.min(1.0)
    };

    let mut shop_strength = 0.0;
    let mut shop_n = 0.0;
    let mut luxury_strength = 0.0;
    let mut luxury_n = 0.0;
    for b in world.businesses.iter().filter(|b| b.district == id) {
        if b.state == super::world::BusinessState::Closed {
            continue;
        }
        let s = world.team(b.team).effective_strength();
        match b.kind {
            super::world::BusinessKind::Luxury => {
                luxury_strength += s;
                luxury_n += 1.0;
            }
            _ => {
                shop_strength += s;
                shop_n += 1.0;
            }
        }
    }

    let mut shared_access = 0.0;
    let mut shared_strength = 0.0;
    let mut shared_n = 0.0;
    for t in world.teams.iter().filter(|t| t.district == id) {
        if matches!(t.kind, super::world::TeamKind::Community | super::world::TeamKind::Public) {
            shared_access += t.access_share;
            shared_strength += t.effective_strength();
            shared_n += 1.0;
        }
    }

    let mean_distance = if d.distance_to.is_empty() {
        30.0
    } else {
        d.distance_to.iter().map(|v| *v as f32).sum::<f32>() / d.distance_to.len() as f32
    };

    DistrictContext {
        infra: d.infra,
        dojo_capacity,
        dojo_openness,
        dojo_night_share,
        coach_staff,
        nursery_capacity: world.capacity_of(id, FacilityKind::Nursery),
        hospital_capacity: world.capacity_of(id, FacilityKind::Hospital),
        canteen_capacity: world.capacity_of(id, FacilityKind::Canteen),
        dormitory_quality,
        venue_capacity: world.capacity_of(id, FacilityKind::PaymentVenue),
        transit_facility,
        population: d.population() as f32,
        mean_distance,
        shop_strength: if shop_n > 0.0 { shop_strength / shop_n } else { 12.0 },
        luxury_strength: if luxury_n > 0.0 { luxury_strength / luxury_n } else { 30.0 },
        shared_team_access: if shared_n > 0.0 { (shared_access / shared_n).min(1.0) } else { 0.0 },
        shared_team_strength: if shared_n > 0.0 { shared_strength / shared_n } else { 0.0 },
    }
}
