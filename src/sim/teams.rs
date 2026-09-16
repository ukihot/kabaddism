//! チームの日次更新（FR-TEAM-*）
//!
//! チームの競技力は**住民の能力から導出する**。政策が数値を直接書き込むことはしない。
//! 政策が動かせるのは「誰が使えるか（利用権）」「何回出られるか（出場枠）」だけ。

use super::ids::{DistrictId, NationId, TeamId};
use super::world::{BusinessKind, BusinessState, Occupation, TeamKind, World};
use super::Game;

pub fn refresh(game: &mut Game) {
    // 地区ごとの能力指標を先に作る（借用の衝突を避ける）
    let n = game.world.districts.len();
    let mut mean_ability = vec![0.0f32; n];
    let mut clerk_ability = vec![0.0f32; n];
    let mut top_ability = vec![0.0f32; n];
    let mut mean_condition = vec![0.0f32; n];

    for (di, d) in game.world.districts.iter().enumerate() {
        let (mut num, mut den, mut cnum, mut cden, mut cond) = (0.0, 0.0, 0.0, 0.0, 0.0);
        for c in &d.cohorts {
            let w = c.headcount as f32;
            num += w * c.life.ability.value;
            cond += w * c.life.condition.factor();
            den += w;
            if matches!(c.life.occupation, Occupation::Clerk | Occupation::Cook) {
                cnum += w * c.life.ability.value;
                cden += w;
            }
        }
        mean_ability[di] = if den > 0.0 { num / den } else { 0.0 };
        mean_condition[di] = if den > 0.0 { cond / den } else { 1.0 };
        clerk_ability[di] = if cden > 0.0 { cnum / cden } else { mean_ability[di] };
        top_ability[di] = mean_ability[di];
    }
    for p in &game.world.people {
        if p.is_active() {
            let di = p.home.index();
            if p.life.ability.value > top_ability[di] {
                top_ability[di] = p.life.ability.value;
            }
        }
    }

    let luxury: Vec<TeamId> = game
        .world
        .businesses
        .iter()
        .filter(|b| b.kind == BusinessKind::Luxury)
        .map(|b| b.team)
        .collect();

    for (ti, t) in game.world.teams.iter_mut().enumerate() {
        let di = t.district.index();
        let id = TeamId::from_index(ti);

        // 出場枠は毎日戻る。疲労は少しずつ抜ける。
        t.slots_used = 0.0;
        t.fatigue = (t.fatigue * 0.93).max(0.0);
        t.on_national_duty = 0.0;

        let derived = match t.kind {
            // 店舗常駐: 店員の能力
            TeamKind::ShopResident => {
                if luxury.contains(&id) {
                    // 高級店は強豪を用意する（元代表選手を配属する、など）
                    top_ability[di] * 0.95 + clerk_ability[di] * 0.2
                } else {
                    clerk_ability[di] * 0.9
                }
            }
            // 住民共同・公共: 地区平均
            TeamKind::Community | TeamKind::Public => mean_ability[di] * 0.95,
            // 代理業: 平均より上の層を雇う
            TeamKind::Agency => mean_ability[di] * 1.15,
            // 私有・代表: メンバーの能力
            TeamKind::Private => mean_ability[di],
        };

        if t.members.is_empty() {
            t.strength += (derived - t.strength) * 0.1;
        }
        let _ = mean_condition[di];
    }

    // メンバーを持つチーム（代表・私有）はメンバーの能力から直接求める
    let member_lists: Vec<(usize, Vec<super::ids::PersonId>)> = game
        .world
        .teams
        .iter()
        .enumerate()
        .filter(|(_, t)| !t.members.is_empty())
        .map(|(i, t)| (i, t.members.clone()))
        .collect();
    for (i, members) in member_lists {
        let mut sum = 0.0;
        let mut cond = 0.0;
        let mut n = 0.0;
        for pid in &members {
            let p = &game.world.people[pid.index()];
            if p.status != super::world::PersonStatus::Active {
                continue;
            }
            let injured = if p.injury.is_some() { 0.4 } else { 1.0 };
            sum += p.life.ability.value * injured;
            cond += p.life.condition.factor();
            n += 1.0;
        }
        let t = &mut game.world.teams[i];
        if n > 0.0 {
            t.strength = sum / n;
            t.roster = n;
            t.fatigue = (t.fatigue + (1.0 - (cond / n).min(1.0)) * 0.05).clamp(0.0, 1.5);
        } else {
            t.strength = 0.0;
            t.roster = 0.1;
        }
    }

    // 閉店した事業者のチームは稼働しない
    let closed: Vec<TeamId> = game
        .world
        .businesses
        .iter()
        .filter(|b| b.state == BusinessState::Closed)
        .map(|b| b.team)
        .collect();
    for tid in closed {
        game.world.teams[tid.index()].slots = 0.0;
    }
}

/// 世帯が決済のために立てられる実効戦力。
/// 契約チームがあればその戦力 × 利用権、なければ本人の能力から。
pub fn household_strength(world: &World, life: &super::world::Life) -> f32 {
    let personal = life.ability.value * 0.6 * life.condition.factor();
    match life.team {
        Some(t) => {
            let team = world.team(t);
            let share = team.access_share.max(0.05);
            (team.effective_strength() * share).max(personal)
        }
        None => personal,
    }
}

/// 代表チーム（FR-STAT-01 の常時表示）。
pub fn national(game: &Game) -> &super::world::Team {
    game.world.team(game.world.national_team)
}

/// 自国のチーム利用権の集中度（FR-STAT-02 分配分野）。
/// 0 に近いほど均等、1 に近いほど一部に集中している。
pub fn access_concentration(world: &World, home: NationId) -> f32 {
    let mut shares: Vec<f32> = Vec::new();
    for (id, _) in world.owned_districts(home) {
        shares.push(team_access_in(world, id));
    }
    if shares.is_empty() {
        return 0.0;
    }
    let mean = shares.iter().sum::<f32>() / shares.len() as f32;
    if mean <= 0.0 {
        return 1.0;
    }
    let var = shares.iter().map(|s| (s - mean).powi(2)).sum::<f32>() / shares.len() as f32;
    (var.sqrt() / mean).clamp(0.0, 1.0)
}

pub fn team_access_in(world: &World, district: DistrictId) -> f32 {
    let mut acc = 0.0;
    let mut n = 0.0;
    for t in world.teams.iter().filter(|t| t.district == district) {
        if matches!(t.kind, TeamKind::Community | TeamKind::Public | TeamKind::Agency) {
            acc += t.access_share;
            n += 1.0;
        }
    }
    if n > 0.0 { acc / n } else { 0.0 }
}
