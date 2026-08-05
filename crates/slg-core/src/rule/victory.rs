//! 胜利条件引擎
//!
//! 支持可配置的胜利/失败条件，包括逻辑组合（And/Or）。
//! 每个势力独立评估，满足任一条件即获胜。

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::entity::faction::FactionState;
use slg_data::ids::{FactionId, TileKey};

// ---------------------------------------------------------------------------
// 胜利条件
// ---------------------------------------------------------------------------

/// 胜利条件枚举
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VictoryCondition {
    /// 占领指定区域（所有指定格子均归属该势力）
    OccupyRegion {
        tiles: std::collections::BTreeSet<TileKey>,
        label: String,
    },
    /// 占领至少 N 格
    OccupyCount { min_tiles: usize },
    /// 消灭指定势力（该势力不再拥有任何格子）
    EliminateFaction { faction: FactionId },
    /// 存活至少 N tick
    SurviveTicks { ticks: u64 },
    /// 资源达到阈值
    ResourceThreshold { resource: String, amount: u64 },
    /// 控制所有城池（简化实现：占领超过半数格子）
    ControlAllCities,
    /// 逻辑与（所有子条件均满足）
    And(Vec<VictoryCondition>),
    /// 逻辑或（任一子条件满足）
    Or(Vec<VictoryCondition>),
}

impl VictoryCondition {
    /// 评估条件是否满足
    pub fn evaluate(
        &self,
        faction_id: &FactionId,
        faction: &FactionState,
        tile_owners: &BTreeMap<TileKey, FactionId>,
        current_tick: u64,
    ) -> bool {
        match self {
            VictoryCondition::OccupyRegion { tiles, .. } => tiles
                .iter()
                .all(|tile| tile_owners.get(tile) == Some(faction_id)),

            VictoryCondition::OccupyCount { min_tiles } => {
                let count = tile_owners.values().filter(|f| *f == faction_id).count();
                count >= *min_tiles
            }

            VictoryCondition::EliminateFaction { faction: target } => {
                !tile_owners.values().any(|f| f == target)
            }

            VictoryCondition::SurviveTicks { ticks } => current_tick >= *ticks,

            VictoryCondition::ResourceThreshold { resource, amount } => match resource.as_str() {
                "gold" => faction.resources.gold >= *amount,
                "food" => faction.resources.food >= *amount,
                "wood" => faction.resources.wood >= *amount,
                "iron" => faction.resources.iron >= *amount,
                "stone" => faction.resources.stone >= *amount,
                _ => false,
            },

            VictoryCondition::ControlAllCities => {
                let total = tile_owners.len();
                let owned = tile_owners.values().filter(|f| *f == faction_id).count();
                total > 0 && owned > total / 2
            }

            VictoryCondition::And(conditions) => {
                if conditions.is_empty() {
                    return false;
                }
                conditions
                    .iter()
                    .all(|c| c.evaluate(faction_id, faction, tile_owners, current_tick))
            }

            VictoryCondition::Or(conditions) => conditions
                .iter()
                .any(|c| c.evaluate(faction_id, faction, tile_owners, current_tick)),
        }
    }
}

// ---------------------------------------------------------------------------
// 胜利结果
// ---------------------------------------------------------------------------

/// 单次胜利结果
#[derive(Debug, Clone)]
pub struct VictoryResult {
    pub faction: FactionId,
    pub condition_label: String,
    pub tick: u64,
}

// ---------------------------------------------------------------------------
// 胜利状态
// ---------------------------------------------------------------------------

/// 胜利状态：跟踪所有势力的胜利条件与已达成结果
#[derive(Debug, Default)]
pub struct VictoryState {
    /// (标签, 条件) 列表
    pub conditions: Vec<(String, VictoryCondition)>,
    /// 已达成的胜利结果
    pub results: Vec<VictoryResult>,
}

impl VictoryState {
    /// 添加一条胜利条件
    pub fn add_condition(&mut self, label: String, condition: VictoryCondition) {
        self.conditions.push((label, condition));
    }

    /// 检查某势力是否满足任一胜利条件
    ///
    /// 若该势力已获胜则跳过，返回 `None`。
    pub fn check_victory(
        &mut self,
        faction_id: &FactionId,
        faction: &FactionState,
        tile_owners: &BTreeMap<TileKey, FactionId>,
        current_tick: u64,
    ) -> Option<VictoryResult> {
        // 已经胜利的势力不再检查
        if self.results.iter().any(|r| &r.faction == faction_id) {
            return None;
        }

        for (label, condition) in &self.conditions {
            if condition.evaluate(faction_id, faction, tile_owners, current_tick) {
                let result = VictoryResult {
                    faction: faction_id.clone(),
                    condition_label: label.clone(),
                    tick: current_tick,
                };
                self.results.push(result.clone());
                return Some(result);
            }
        }

        None
    }

    /// 检查失败条件：势力是否已覆灭（无任何领地）
    pub fn check_defeat(
        &self,
        faction_id: &FactionId,
        tile_owners: &BTreeMap<TileKey, FactionId>,
    ) -> bool {
        !tile_owners.values().any(|f| f == faction_id)
    }
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::faction::{FactionPersonality, FactionResources};
    use crate::map::grid::HexCoord;

    /// 创建一个用于测试的 FactionState
    fn create_faction() -> FactionState {
        FactionState {
            resources: FactionResources {
                gold: 1000,
                food: 500,
                wood: 100,
                iron: 50,
                stone: 30,
                troops: 200,
            },
            personality: FactionPersonality {
                aggression: 0.5,
                expansion: 0.5,
                diplomacy: 0.5,
                caution: 0.5,
            },
            main_city: Some(HexCoord::new(5, 5)),
            diplomacy: BTreeMap::new(),
            ..Default::default()
        }
    }

    // --- OccupyRegion ---

    #[test]
    fn test_occupy_region_success() {
        let tiles: std::collections::BTreeSet<TileKey> = [
            HexCoord::new(0, 0).to_tile_key(),
            HexCoord::new(1, 0).to_tile_key(),
        ]
        .into_iter()
        .collect();
        let condition = VictoryCondition::OccupyRegion {
            tiles,
            label: "中原".to_string(),
        };
        let faction = create_faction();
        let mut tile_owners = BTreeMap::new();
        tile_owners.insert(HexCoord::new(0, 0).to_tile_key(), "f1".to_string());
        tile_owners.insert(HexCoord::new(1, 0).to_tile_key(), "f1".to_string());
        tile_owners.insert(HexCoord::new(2, 0).to_tile_key(), "f2".to_string());

        assert!(condition.evaluate(&"f1".to_string(), &faction, &tile_owners, 50));
    }

    #[test]
    fn test_occupy_region_partial_fail() {
        let tiles: std::collections::BTreeSet<TileKey> = [
            HexCoord::new(0, 0).to_tile_key(),
            HexCoord::new(1, 0).to_tile_key(),
        ]
        .into_iter()
        .collect();
        let condition = VictoryCondition::OccupyRegion {
            tiles,
            label: "中原".to_string(),
        };
        let faction = create_faction();
        let mut tile_owners = BTreeMap::new();
        tile_owners.insert(HexCoord::new(0, 0).to_tile_key(), "f1".to_string());
        tile_owners.insert(HexCoord::new(1, 0).to_tile_key(), "f2".to_string());

        assert!(!condition.evaluate(&"f1".to_string(), &faction, &tile_owners, 50));
    }

    // --- OccupyCount ---

    #[test]
    fn test_occupy_count_met() {
        let condition = VictoryCondition::OccupyCount { min_tiles: 10 };
        let faction = create_faction();
        let mut tile_owners = BTreeMap::new();
        for i in 0..10 {
            tile_owners.insert(HexCoord::new(i, 0).to_tile_key(), "faction_1".to_string());
        }

        assert!(condition.evaluate(&"faction_1".to_string(), &faction, &tile_owners, 100));
    }

    #[test]
    fn test_occupy_count_not_met() {
        let condition = VictoryCondition::OccupyCount { min_tiles: 10 };
        let faction = create_faction();
        let mut tile_owners = BTreeMap::new();
        for i in 0..9 {
            tile_owners.insert(HexCoord::new(i, 0).to_tile_key(), "faction_1".to_string());
        }

        assert!(!condition.evaluate(&"faction_1".to_string(), &faction, &tile_owners, 100));
    }

    #[test]
    fn test_occupy_count_wrong_faction() {
        let condition = VictoryCondition::OccupyCount { min_tiles: 10 };
        let faction = create_faction();
        let mut tile_owners = BTreeMap::new();
        for i in 0..10 {
            tile_owners.insert(HexCoord::new(i, 0).to_tile_key(), "faction_1".to_string());
        }

        assert!(!condition.evaluate(&"faction_2".to_string(), &faction, &tile_owners, 100));
    }

    // --- SurviveTicks ---

    #[test]
    fn test_survive_ticks_exact() {
        let condition = VictoryCondition::SurviveTicks { ticks: 100 };
        let faction = create_faction();
        let tile_owners = BTreeMap::new();

        assert!(condition.evaluate(&"f1".to_string(), &faction, &tile_owners, 100));
    }

    #[test]
    fn test_survive_ticks_not_yet() {
        let condition = VictoryCondition::SurviveTicks { ticks: 100 };
        let faction = create_faction();
        let tile_owners = BTreeMap::new();

        assert!(!condition.evaluate(&"f1".to_string(), &faction, &tile_owners, 50));
    }

    // --- EliminateFaction ---

    #[test]
    fn test_eliminate_faction_still_alive() {
        let condition = VictoryCondition::EliminateFaction {
            faction: "faction_2".to_string(),
        };
        let faction = create_faction();
        let mut tile_owners = BTreeMap::new();
        tile_owners.insert(HexCoord::new(0, 0).to_tile_key(), "faction_2".to_string());

        assert!(!condition.evaluate(&"faction_1".to_string(), &faction, &tile_owners, 100));
    }

    #[test]
    fn test_eliminate_faction_eliminated() {
        let condition = VictoryCondition::EliminateFaction {
            faction: "faction_2".to_string(),
        };
        let faction = create_faction();
        let tile_owners = BTreeMap::new();

        assert!(condition.evaluate(&"faction_1".to_string(), &faction, &tile_owners, 100));
    }

    // --- ResourceThreshold ---

    #[test]
    fn test_resource_threshold_gold_met() {
        let condition = VictoryCondition::ResourceThreshold {
            resource: "gold".to_string(),
            amount: 500,
        };
        let faction = create_faction();
        let tile_owners = BTreeMap::new();

        assert!(condition.evaluate(&"f1".to_string(), &faction, &tile_owners, 0));
    }

    #[test]
    fn test_resource_threshold_gold_not_met() {
        let condition = VictoryCondition::ResourceThreshold {
            resource: "gold".to_string(),
            amount: 2000,
        };
        let faction = create_faction();
        let tile_owners = BTreeMap::new();

        assert!(!condition.evaluate(&"f1".to_string(), &faction, &tile_owners, 0));
    }

    #[test]
    fn test_resource_threshold_food() {
        let condition = VictoryCondition::ResourceThreshold {
            resource: "food".to_string(),
            amount: 500,
        };
        let faction = create_faction();
        let tile_owners = BTreeMap::new();

        assert!(condition.evaluate(&"f1".to_string(), &faction, &tile_owners, 0));
    }

    // --- ControlAllCities ---

    #[test]
    fn test_control_all_cities_majority() {
        let condition = VictoryCondition::ControlAllCities;
        let faction = create_faction();
        let mut tile_owners = BTreeMap::new();
        // 6 格中占 4 格 > 50%
        for i in 0..4 {
            tile_owners.insert(HexCoord::new(i, 0).to_tile_key(), "f1".to_string());
        }
        for i in 4..6 {
            tile_owners.insert(HexCoord::new(i, 0).to_tile_key(), "f2".to_string());
        }

        assert!(condition.evaluate(&"f1".to_string(), &faction, &tile_owners, 0));
    }

    #[test]
    fn test_control_all_cities_half_not_enough() {
        let condition = VictoryCondition::ControlAllCities;
        let faction = create_faction();
        let mut tile_owners = BTreeMap::new();
        // 恰好 50% 不满足 > 50%
        for i in 0..5 {
            tile_owners.insert(HexCoord::new(i, 0).to_tile_key(), "f1".to_string());
        }
        for i in 5..10 {
            tile_owners.insert(HexCoord::new(i, 0).to_tile_key(), "f2".to_string());
        }

        assert!(!condition.evaluate(&"f1".to_string(), &faction, &tile_owners, 0));
    }

    // --- And / Or 组合 ---

    #[test]
    fn test_and_condition_all_met() {
        let condition = VictoryCondition::And(vec![
            VictoryCondition::SurviveTicks { ticks: 50 },
            VictoryCondition::OccupyCount { min_tiles: 5 },
        ]);
        let faction = create_faction();
        let mut tile_owners = BTreeMap::new();
        for i in 0..5 {
            tile_owners.insert(HexCoord::new(i, 0).to_tile_key(), "f1".to_string());
        }

        assert!(condition.evaluate(&"f1".to_string(), &faction, &tile_owners, 100));
    }

    #[test]
    fn test_and_condition_one_not_met() {
        let condition = VictoryCondition::And(vec![
            VictoryCondition::SurviveTicks { ticks: 200 },
            VictoryCondition::OccupyCount { min_tiles: 5 },
        ]);
        let faction = create_faction();
        let mut tile_owners = BTreeMap::new();
        for i in 0..5 {
            tile_owners.insert(HexCoord::new(i, 0).to_tile_key(), "f1".to_string());
        }

        // tick 100 < 200
        assert!(!condition.evaluate(&"f1".to_string(), &faction, &tile_owners, 100));
    }

    #[test]
    fn test_or_condition_second_met() {
        let condition = VictoryCondition::Or(vec![
            VictoryCondition::SurviveTicks { ticks: 200 },
            VictoryCondition::OccupyCount { min_tiles: 5 },
        ]);
        let faction = create_faction();
        let mut tile_owners = BTreeMap::new();
        for i in 0..5 {
            tile_owners.insert(HexCoord::new(i, 0).to_tile_key(), "f1".to_string());
        }

        // tick 100 < 200，但已占 5 格
        assert!(condition.evaluate(&"f1".to_string(), &faction, &tile_owners, 100));
    }

    #[test]
    fn test_or_condition_none_met() {
        let condition = VictoryCondition::Or(vec![
            VictoryCondition::SurviveTicks { ticks: 200 },
            VictoryCondition::OccupyCount { min_tiles: 10 },
        ]);
        let faction = create_faction();
        let mut tile_owners = BTreeMap::new();
        for i in 0..5 {
            tile_owners.insert(HexCoord::new(i, 0).to_tile_key(), "f1".to_string());
        }

        assert!(!condition.evaluate(&"f1".to_string(), &faction, &tile_owners, 100));
    }

    // --- VictoryState 集成 ---

    #[test]
    fn test_victory_state_check() {
        let mut state = VictoryState::default();
        state.add_condition(
            "统一".to_string(),
            VictoryCondition::OccupyCount { min_tiles: 10 },
        );

        let faction = create_faction();
        let mut tile_owners = BTreeMap::new();
        for i in 0..10 {
            tile_owners.insert(HexCoord::new(i, 0).to_tile_key(), "faction_1".to_string());
        }

        let result = state.check_victory(&"faction_1".to_string(), &faction, &tile_owners, 100);
        assert!(result.is_some());
        let r = result.unwrap();
        assert_eq!(r.faction, "faction_1");
        assert_eq!(r.condition_label, "统一");
        assert_eq!(r.tick, 100);
    }

    #[test]
    fn test_victory_state_no_repeat() {
        let mut state = VictoryState::default();
        state.add_condition(
            "统一".to_string(),
            VictoryCondition::OccupyCount { min_tiles: 10 },
        );

        let faction = create_faction();
        let mut tile_owners = BTreeMap::new();
        for i in 0..10 {
            tile_owners.insert(HexCoord::new(i, 0).to_tile_key(), "faction_1".to_string());
        }

        // 第一次检查：胜利
        assert!(state
            .check_victory(&"faction_1".to_string(), &faction, &tile_owners, 100)
            .is_some());
        // 第二次检查：已胜利，不再触发
        assert!(state
            .check_victory(&"faction_1".to_string(), &faction, &tile_owners, 200)
            .is_none());
    }

    #[test]
    fn test_victory_state_multi_faction_independent() {
        let mut state = VictoryState::default();
        state.add_condition(
            "占领10格".to_string(),
            VictoryCondition::OccupyCount { min_tiles: 10 },
        );

        let faction = create_faction();
        let mut tile_owners = BTreeMap::new();
        for i in 0..10 {
            tile_owners.insert(HexCoord::new(i, 0).to_tile_key(), "f1".to_string());
        }
        for i in 10..15 {
            tile_owners.insert(HexCoord::new(i, 0).to_tile_key(), "f2".to_string());
        }

        // f1 达成
        assert!(state
            .check_victory(&"f1".to_string(), &faction, &tile_owners, 100)
            .is_some());
        // f2 未达成（只有 5 格）
        assert!(state
            .check_victory(&"f2".to_string(), &faction, &tile_owners, 100)
            .is_none());
    }

    #[test]
    fn test_check_defeat_no_territory() {
        let state = VictoryState::default();
        let mut tile_owners = BTreeMap::new();
        tile_owners.insert(HexCoord::new(0, 0).to_tile_key(), "f1".to_string());

        // f2 无领地，判定失败
        assert!(state.check_defeat(&"f2".to_string(), &tile_owners));
    }

    #[test]
    fn test_check_defeat_has_territory() {
        let state = VictoryState::default();
        let mut tile_owners = BTreeMap::new();
        tile_owners.insert(HexCoord::new(0, 0).to_tile_key(), "f1".to_string());
        tile_owners.insert(HexCoord::new(1, 0).to_tile_key(), "f2".to_string());

        // f2 有领地，不算失败
        assert!(!state.check_defeat(&"f2".to_string(), &tile_owners));
    }

    #[test]
    fn test_nested_and_or() {
        // (存活 50 tick AND 占领 5 格) OR 消灭 faction_2
        let condition = VictoryCondition::Or(vec![
            VictoryCondition::And(vec![
                VictoryCondition::SurviveTicks { ticks: 50 },
                VictoryCondition::OccupyCount { min_tiles: 5 },
            ]),
            VictoryCondition::EliminateFaction {
                faction: "faction_2".to_string(),
            },
        ]);
        let faction = create_faction();

        // 场景 1：满足 And 分支
        let mut tile_owners = BTreeMap::new();
        for i in 0..5 {
            tile_owners.insert(HexCoord::new(i, 0).to_tile_key(), "f1".to_string());
        }
        assert!(condition.evaluate(&"f1".to_string(), &faction, &tile_owners, 100));

        // 场景 2：不满足 And，但满足 EliminateFaction
        let mut tile_owners2 = BTreeMap::new();
        tile_owners2.insert(HexCoord::new(0, 0).to_tile_key(), "f1".to_string());
        assert!(condition.evaluate(&"f1".to_string(), &faction, &tile_owners2, 10));
    }
}
