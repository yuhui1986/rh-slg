//! 效用评分系统
//!
//! Layer 2 战术层：候选动作效用评分（占地/攻城/增援/侦察），取 Top-N。

use std::collections::BTreeMap;

use slg_data::ids::{tile_key, FactionId, TileKey};

use crate::ai::diplomacy::DiplomacyActionType;
use crate::entity::faction::{FactionResources, FactionState};
use crate::map::grid::HexCoord;

// ---------------------------------------------------------------------------
// 候选动作
// ---------------------------------------------------------------------------

/// 候选动作
#[derive(Debug, Clone)]
pub enum CandidateAction {
    OccupyTile {
        target: HexCoord,
        value: f64,
    },
    MarchAttack {
        target: HexCoord,
        army_count: u32,
    },
    Recruit {
        city: HexCoord,
        count: u32,
    },
    #[allow(dead_code)]
    Build {
        city: HexCoord,
        building: String,
    },
    #[allow(dead_code)]
    SendDiplomacy {
        target: FactionId,
        action: DiplomacyActionType,
    },
    #[allow(dead_code)]
    Reinforce {
        from: HexCoord,
        to: HexCoord,
    },
    #[allow(dead_code)]
    Scout {
        target: HexCoord,
    },
}

// ---------------------------------------------------------------------------
// 效用评分结果
// ---------------------------------------------------------------------------

/// 效用评分结果
#[derive(Debug, Clone)]
pub struct ScoredAction {
    pub action: CandidateAction,
    pub score: f64,
}

// ---------------------------------------------------------------------------
// 效用评分函数
// ---------------------------------------------------------------------------

/// 计算占地效用
///
/// 公式：`score = 资源价值 * 距离衰减 * (1 - 敌方密度 * 0.1) * 性格攻击倾向`
pub fn score_occupy(
    target: HexCoord,
    faction: &FactionState,
    tile_levels: &BTreeMap<TileKey, u8>,
    tile_owners: &BTreeMap<TileKey, FactionId>,
    faction_id: &FactionId,
) -> f64 {
    let key = target.to_tile_key();

    // 资源价值（level / 9.0，最大为 1.0）
    let level = tile_levels.get(&key).copied().unwrap_or(1) as f64;
    let resource_value = level / 9.0;

    // 距离衰减（用六边形格数近似）
    let main_city = faction.main_city.unwrap_or(HexCoord::new(0, 0));
    let dist = target.distance(main_city) as f64;
    let distance_decay = 1.0 / (1.0 + dist * 0.01);

    // 敌方密度（周围有多少非己方格）
    let enemy_count = target
        .neighbors()
        .iter()
        .filter(|n| {
            let nkey = n.to_tile_key();
            tile_owners.get(&nkey).is_some_and(|o| o != faction_id)
        })
        .count() as f64;
    let enemy_factor = (1.0 - enemy_count * 0.1).max(0.1);

    // 性格加权
    let aggression = faction.personality.aggression;

    resource_value * distance_decay * enemy_factor * aggression
}

/// 占地效用评分（增强版）
///
/// 在基础评分之上加入扩张潜力与性格多维权重，使势力差异更明显。
pub fn score_occupy_enhanced(
    target: HexCoord,
    faction: &FactionState,
    tile_owners: &BTreeMap<TileKey, FactionId>,
    tile_levels: &BTreeMap<TileKey, u8>,
    faction_id: &FactionId,
    _current_tick: u64,
) -> f64 {
    let key = target.to_tile_key();

    // 1. 资源价值
    let level = tile_levels.get(&key).copied().unwrap_or(1) as f64;
    let resource_value = level / 9.0;

    // 2. 距离衰减
    let main_city = faction.main_city.unwrap_or(HexCoord::new(0, 0));
    let dist = target.distance(main_city) as f64;
    let distance_decay = 1.0 / (1.0 + dist * 0.01);

    // 3. 威胁评估（周围敌方格数量）
    let enemy_count = target
        .neighbors()
        .iter()
        .filter(|n| {
            let nkey = n.to_tile_key();
            tile_owners.get(&nkey).is_some_and(|o| o != faction_id)
        })
        .count() as f64;
    let threat_factor = (1.0 - enemy_count * 0.15).max(0.1);

    // 4. 扩张潜力（周围空格数量）
    let empty_count = target
        .neighbors()
        .iter()
        .filter(|n| !tile_owners.contains_key(&n.to_tile_key()))
        .count() as f64;
    let expansion_potential = 1.0 + empty_count * 0.1;

    // 5. 性格加权
    let aggression = faction.personality.aggression;
    let expansion = faction.personality.expansion;
    let caution = faction.personality.caution;

    // 综合评分
    let base_score = resource_value * distance_decay * threat_factor * expansion_potential;

    // 性格影响
    let personality_modifier = aggression * 0.3 + expansion * 0.5 + (1.0 - caution) * 0.2;

    base_score * personality_modifier
}

/// 防御效用评分
///
/// 评估己方领地的防御紧迫度，用于在 Layer 0/Layer 2 中决定是否回防。
pub fn score_defend(
    coord: HexCoord,
    faction: &FactionState,
    tile_owners: &BTreeMap<TileKey, FactionId>,
    faction_id: &FactionId,
) -> f64 {
    let key = coord.to_tile_key();

    // 只评估己方领地
    if tile_owners.get(&key) != Some(faction_id) {
        return 0.0;
    }

    // 计算威胁等级（周围敌方格数量）
    let threat = coord
        .neighbors()
        .iter()
        .filter(|n| {
            let nkey = n.to_tile_key();
            tile_owners.get(&nkey).is_some_and(|o| o != faction_id)
        })
        .count() as f64;

    // 计算战略价值（与主城的距离——越近越重要）
    let main_city = faction.main_city.unwrap_or(HexCoord::new(0, 0));
    let dist_to_main = coord.distance(main_city) as f64;
    let strategic_value = 1.0 / (1.0 + dist_to_main * 0.05);

    // 防御评分 = 威胁 × 战略价值 × 谨慎度
    threat * strategic_value * faction.personality.caution
}

/// 征兵效用评分
///
/// 兵力不足 + 资源充足 + 高攻击性 => 征兵优先级高。
pub fn score_recruit(
    faction: &FactionState,
    current_troops: u32,
    resources: &FactionResources,
) -> f64 {
    // 兵力不足时征兵优先级高
    let troop_ratio = current_troops as f64 / 1000.0; // 假设 1000 是理想兵力
    let troop_need = (1.0 - troop_ratio).max(0.0);

    // 资源充足时征兵优先级高
    let resource_available = (resources.gold as f64 / 1000.0).min(1.0);

    // 性格影响
    let aggression = faction.personality.aggression;

    troop_need * resource_available * aggression
}

// ---------------------------------------------------------------------------
// 候选动作生成
// ---------------------------------------------------------------------------

/// 生成候选动作列表
///
/// 扫描主城周围 10 格范围内可占领的格子，按效用评分排序，取 Top-5。
pub fn generate_candidates(
    faction_id: &FactionId,
    faction: &FactionState,
    tile_owners: &BTreeMap<TileKey, FactionId>,
    tile_levels: &BTreeMap<TileKey, u8>,
) -> Vec<ScoredAction> {
    let mut candidates = Vec::new();

    let main_city = faction.main_city.unwrap_or(HexCoord::new(0, 0));

    // 扫描主城周围 10 格范围
    for dq in -10..=10 {
        for dr in -10..=10 {
            let coord = HexCoord::new(main_city.q + dq, main_city.r + dr);
            let key = tile_key(coord.q, coord.r);

            // 跳过己方已占格
            if tile_owners.get(&key) == Some(faction_id) {
                continue;
            }

            // 检查是否相邻己方格（铺路规则）
            let has_adjacent_friendly = coord
                .neighbors()
                .iter()
                .any(|n| tile_owners.get(&n.to_tile_key()) == Some(faction_id));

            if has_adjacent_friendly {
                let score = score_occupy(coord, faction, tile_levels, tile_owners, faction_id);
                candidates.push(ScoredAction {
                    action: CandidateAction::OccupyTile {
                        target: coord,
                        value: score,
                    },
                    score,
                });
            }
        }
    }

    // 按分数降序排序，取 Top-5
    candidates.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    candidates.truncate(5);

    candidates
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// 创建测试用势力状态
    fn create_faction() -> FactionState {
        FactionState {
            resources: crate::entity::faction::FactionResources {
                gold: 1000,
                food: 500,
                wood: 300,
                iron: 200,
                stone: 100,
                troops: 500,
            },
            personality: crate::entity::faction::FactionPersonality {
                aggression: 0.8,
                expansion: 0.9,
                diplomacy: 0.4,
                caution: 0.5,
            },
            main_city: Some(HexCoord::new(5, 5)),
            diplomacy: BTreeMap::new(),
            ..Default::default()
        }
    }

    #[test]
    fn test_score_occupy_positive() {
        let faction = create_faction();
        let mut tile_levels = BTreeMap::new();
        let mut tile_owners = BTreeMap::new();
        let faction_id = "faction_1".to_string();

        tile_levels.insert(tile_key(6, 5), 5);
        tile_owners.insert(tile_key(5, 5), faction_id.clone());

        let score = score_occupy(
            HexCoord::new(6, 5),
            &faction,
            &tile_levels,
            &tile_owners,
            &faction_id,
        );

        assert!(score > 0.0, "score_occupy should return positive score");
    }

    #[test]
    fn test_score_occupy_respects_aggression() {
        let mut faction_aggressive = create_faction();
        faction_aggressive.personality.aggression = 1.0;

        let mut faction_passive = create_faction();
        faction_passive.personality.aggression = 0.1;

        let tile_levels = BTreeMap::new();
        let mut tile_owners = BTreeMap::new();
        let faction_id = "faction_1".to_string();
        tile_owners.insert(tile_key(5, 5), faction_id.clone());

        let score_aggressive = score_occupy(
            HexCoord::new(6, 5),
            &faction_aggressive,
            &tile_levels,
            &tile_owners,
            &faction_id,
        );
        let score_passive = score_occupy(
            HexCoord::new(6, 5),
            &faction_passive,
            &tile_levels,
            &tile_owners,
            &faction_id,
        );

        assert!(
            score_aggressive > score_passive,
            "aggressive faction should score higher"
        );
    }

    #[test]
    fn test_generate_candidates_non_empty() {
        let faction = create_faction();
        let mut tile_owners = BTreeMap::new();
        let tile_levels = BTreeMap::new();
        let faction_id = "faction_1".to_string();

        // 设置主城
        tile_owners.insert(tile_key(5, 5), faction_id.clone());

        let candidates = generate_candidates(&faction_id, &faction, &tile_owners, &tile_levels);

        assert!(
            !candidates.is_empty(),
            "should generate at least one candidate"
        );
    }

    #[test]
    fn test_generate_candidates_only_adjacent() {
        let faction = create_faction();
        let mut tile_owners = BTreeMap::new();
        let tile_levels = BTreeMap::new();
        let faction_id = "faction_1".to_string();

        // 只设置主城
        tile_owners.insert(tile_key(5, 5), faction_id.clone());

        let candidates = generate_candidates(&faction_id, &faction, &tile_owners, &tile_levels);

        // 所有候选动作的目标必须与己方格相邻
        for c in &candidates {
            if let CandidateAction::OccupyTile { target, .. } = &c.action {
                let has_adjacent = target
                    .neighbors()
                    .iter()
                    .any(|n| tile_owners.get(&n.to_tile_key()) == Some(&faction_id));
                assert!(
                    has_adjacent,
                    "candidate target must be adjacent to owned tile"
                );
            }
        }
    }

    #[test]
    fn test_generate_candidates_sorted_descending() {
        let faction = create_faction();
        let mut tile_owners = BTreeMap::new();
        let tile_levels = BTreeMap::new();
        let faction_id = "faction_1".to_string();

        tile_owners.insert(tile_key(5, 5), faction_id.clone());

        let candidates = generate_candidates(&faction_id, &faction, &tile_owners, &tile_levels);

        // 验证降序排列
        for i in 1..candidates.len() {
            assert!(
                candidates[i - 1].score >= candidates[i].score,
                "candidates should be sorted in descending order"
            );
        }
    }

    #[test]
    fn test_generate_candidates_max_5() {
        let faction = create_faction();
        let mut tile_owners = BTreeMap::new();
        let tile_levels = BTreeMap::new();
        let faction_id = "faction_1".to_string();

        tile_owners.insert(tile_key(5, 5), faction_id.clone());

        let candidates = generate_candidates(&faction_id, &faction, &tile_owners, &tile_levels);

        assert!(candidates.len() <= 5, "should return at most 5 candidates");
    }

    // -----------------------------------------------------------------------
    // score_occupy_enhanced 测试
    // -----------------------------------------------------------------------

    #[test]
    fn test_enhanced_occupy_positive() {
        let faction = create_faction();
        let mut tile_levels = BTreeMap::new();
        let mut tile_owners = BTreeMap::new();
        let faction_id = "faction_1".to_string();

        tile_levels.insert(tile_key(6, 5), 5);
        tile_owners.insert(tile_key(5, 5), faction_id.clone());

        let score = score_occupy_enhanced(
            HexCoord::new(6, 5),
            &faction,
            &tile_owners,
            &tile_levels,
            &faction_id,
            0,
        );

        assert!(
            score > 0.0,
            "score_occupy_enhanced should return positive score"
        );
    }

    #[test]
    fn test_enhanced_wei_expands_more_than_shu() {
        use crate::ai::persona::{SHU, WEI};

        let tile_levels = BTreeMap::new();
        let mut tile_owners = BTreeMap::new();

        let wei_id = "faction_wei".to_string();
        let shu_id = "faction_shu".to_string();

        let mut wei_faction = create_faction();
        wei_faction.personality = WEI;

        let mut shu_faction = create_faction();
        shu_faction.personality = SHU;

        tile_owners.insert(tile_key(0, 0), wei_id.clone());
        tile_owners.insert(tile_key(0, 0), shu_id.clone());

        let wei_score = score_occupy_enhanced(
            HexCoord::new(10, 10),
            &wei_faction,
            &BTreeMap::new(),
            &tile_levels,
            &wei_id,
            0,
        );

        let shu_score = score_occupy_enhanced(
            HexCoord::new(10, 10),
            &shu_faction,
            &BTreeMap::new(),
            &tile_levels,
            &shu_id,
            0,
        );

        assert!(
            wei_score > shu_score,
            "Wei (aggressive/expansionist) should score higher than Shu: wei={wei_score}, shu={shu_score}"
        );
    }

    // -----------------------------------------------------------------------
    // score_defend 测试
    // -----------------------------------------------------------------------

    #[test]
    fn test_defend_score_nonzero_for_threatened_tile() {
        let faction = create_faction();
        let mut tile_owners = BTreeMap::new();
        let faction_id = "faction_1".to_string();

        // 己方格
        tile_owners.insert(tile_key(5, 5), faction_id.clone());
        // 邻格有敌方
        tile_owners.insert(tile_key(6, 5), "enemy".to_string());

        let score = score_defend(HexCoord::new(5, 5), &faction, &tile_owners, &faction_id);

        assert!(
            score > 0.0,
            "defend score should be > 0 for threatened tile"
        );
    }

    #[test]
    fn test_defend_score_zero_for_non_owned_tile() {
        let faction = create_faction();
        let mut tile_owners = BTreeMap::new();
        let faction_id = "faction_1".to_string();

        // 该格不属于己方
        tile_owners.insert(tile_key(5, 5), "enemy".to_string());

        let score = score_defend(HexCoord::new(5, 5), &faction, &tile_owners, &faction_id);

        assert_eq!(score, 0.0, "defend score should be 0 for non-owned tile");
    }

    #[test]
    fn test_defend_wu_higher_than_wei() {
        use crate::ai::persona::{WEI, WU};

        let fid = "faction_wu".to_string();
        let mut tile_owners = BTreeMap::new();
        tile_owners.insert(tile_key(5, 5), fid.clone());
        tile_owners.insert(tile_key(6, 5), "enemy".to_string());

        let mut wu_faction = create_faction();
        wu_faction.personality = WU;

        let mut wei_faction = create_faction();
        wei_faction.personality = WEI;

        let wu_score = score_defend(HexCoord::new(5, 5), &wu_faction, &tile_owners, &fid);
        let wei_score = score_defend(HexCoord::new(5, 5), &wei_faction, &tile_owners, &fid);

        assert!(
            wu_score > wei_score,
            "Wu (cautious) should value defense more than Wei: wu={wu_score}, wei={wei_score}"
        );
    }

    // -----------------------------------------------------------------------
    // score_recruit 测试
    // -----------------------------------------------------------------------

    #[test]
    fn test_recruit_high_need() {
        let faction = create_faction();
        let resources = FactionResources {
            gold: 2000,
            ..Default::default()
        };

        // 当前兵力很低 => 高征兵需求
        let score_low = score_recruit(&faction, 100, &resources);
        // 当前兵力很高 => 低征兵需求
        let score_high = score_recruit(&faction, 900, &resources);

        assert!(
            score_low > score_high,
            "recruit score should be higher when troops are low: low={score_low}, high={score_high}"
        );
    }

    #[test]
    fn test_recruit_aggressive_faction_higher() {
        use crate::ai::persona::{CAUTIOUS, WEI};

        let resources = FactionResources {
            gold: 2000,
            ..Default::default()
        };

        let mut wei_faction = create_faction();
        wei_faction.personality = WEI;

        let mut cautious_faction = create_faction();
        cautious_faction.personality = CAUTIOUS;

        let wei_score = score_recruit(&wei_faction, 500, &resources);
        let cautious_score = score_recruit(&cautious_faction, 500, &resources);

        assert!(
            wei_score > cautious_score,
            "aggressive faction should recruit more eagerly: wei={wei_score}, cautious={cautious_score}"
        );
    }
}
