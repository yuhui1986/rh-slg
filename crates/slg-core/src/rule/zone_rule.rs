//! 区域规则引擎
//!
//! 支持区域内资源倍率、移动代价、防御加成、通行限制。
//! 纯数据结构 + 查询函数，不依赖任何引擎类型。

use serde::{Deserialize, Serialize};
use slg_data::ids::{FactionId, TileKey};
use std::collections::{BTreeMap, BTreeSet};

// ---------------------------------------------------------------------------
// 区域规则
// ---------------------------------------------------------------------------

/// 区域规则
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZoneRule {
    /// 区域标识
    pub zone_id: String,
    /// 区域包含的格子集合
    pub tiles: BTreeSet<TileKey>,
    /// 区域效果列表
    pub effects: Vec<ZoneEffect>,
    /// 是否激活
    pub active: bool,
}

// ---------------------------------------------------------------------------
// 区域效果
// ---------------------------------------------------------------------------

/// 区域效果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ZoneEffect {
    /// 资源产出倍率（同类倍率相乘）
    ResourceMultiplier { resource: String, factor: f64 },
    /// 移动代价倍率（多个倍率相乘）
    MovementCostMultiplier { factor: f64 },
    /// 防御加成（多个加成相加）
    DefenseBonus { factor: f64 },
    /// 通行限制（白名单模式：非空列表中不包含的势力不可通行）
    RestrictAccess { allowed_factions: Vec<FactionId> },
}

// ---------------------------------------------------------------------------
// 区域规则存储
// ---------------------------------------------------------------------------

/// 区域规则存储
///
/// 管理所有区域规则，提供按格子查询活跃效果的接口。
#[derive(Debug, Default)]
pub struct ZoneRuleStore {
    pub rules: BTreeMap<String, ZoneRule>,
}

impl ZoneRuleStore {
    /// 注册区域规则（按 zone_id 覆盖）
    pub fn register(&mut self, rule: ZoneRule) {
        self.rules.insert(rule.zone_id.clone(), rule);
    }

    /// 移除区域规则
    pub fn remove(&mut self, zone_id: &str) -> Option<ZoneRule> {
        self.rules.remove(zone_id)
    }

    /// 获取指定格子的所有活跃区域效果
    pub fn get_effects_for_tile(&self, tile_key: TileKey) -> Vec<&ZoneEffect> {
        self.rules
            .values()
            .filter(|r| r.active && r.tiles.contains(&tile_key))
            .flat_map(|r| &r.effects)
            .collect()
    }

    /// 计算资源倍率（多个同类倍率相乘）
    pub fn get_resource_multiplier(&self, tile_key: TileKey, resource: &str) -> f64 {
        let mut multiplier = 1.0;
        for effect in self.get_effects_for_tile(tile_key) {
            if let ZoneEffect::ResourceMultiplier {
                resource: res,
                factor,
            } = effect
            {
                if res == resource {
                    multiplier *= factor;
                }
            }
        }
        multiplier
    }

    /// 计算移动代价倍率（多个倍率相乘）
    pub fn get_movement_cost_multiplier(&self, tile_key: TileKey) -> f64 {
        let mut multiplier = 1.0;
        for effect in self.get_effects_for_tile(tile_key) {
            if let ZoneEffect::MovementCostMultiplier { factor } = effect {
                multiplier *= factor;
            }
        }
        multiplier
    }

    /// 计算防御加成（多个加成相加，返回总加成比例）
    pub fn get_defense_bonus(&self, tile_key: TileKey) -> f64 {
        let mut bonus = 0.0;
        for effect in self.get_effects_for_tile(tile_key) {
            if let ZoneEffect::DefenseBonus { factor } = effect {
                bonus += factor;
            }
        }
        bonus
    }

    /// 检查势力是否可通行
    ///
    /// 遍历该格子上所有活跃的 RestrictAccess 效果。
    /// 任何一个效果的白名单非空且不包含该势力时，返回 false。
    pub fn can_faction_pass(&self, tile_key: TileKey, faction: &FactionId) -> bool {
        for effect in self.get_effects_for_tile(tile_key) {
            if let ZoneEffect::RestrictAccess { allowed_factions } = effect {
                if !allowed_factions.is_empty() && !allowed_factions.contains(faction) {
                    return false;
                }
            }
        }
        true
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::grid::HexCoord;

    fn create_test_zone() -> ZoneRule {
        let mut tiles = BTreeSet::new();
        tiles.insert(HexCoord::new(5, 5).to_tile_key());
        tiles.insert(HexCoord::new(6, 5).to_tile_key());

        ZoneRule {
            zone_id: "test_zone".to_string(),
            tiles,
            effects: vec![
                ZoneEffect::ResourceMultiplier {
                    resource: "gold".to_string(),
                    factor: 2.0,
                },
                ZoneEffect::DefenseBonus { factor: 0.5 },
            ],
            active: true,
        }
    }

    // -----------------------------------------------------------------------
    // 资源倍率
    // -----------------------------------------------------------------------

    #[test]
    fn test_resource_multiplier_applies() {
        let mut store = ZoneRuleStore::default();
        store.register(create_test_zone());

        let key = HexCoord::new(5, 5).to_tile_key();
        // gold 倍率 2.0
        assert!(
            (store.get_resource_multiplier(key, "gold") - 2.0).abs() < f64::EPSILON,
            "gold multiplier should be 2.0"
        );
        // food 无倍率 → 1.0
        assert!(
            (store.get_resource_multiplier(key, "food") - 1.0).abs() < f64::EPSILON,
            "food multiplier should be 1.0 (no effect)"
        );
    }

    #[test]
    fn test_resource_multiplier_outside_zone() {
        let mut store = ZoneRuleStore::default();
        store.register(create_test_zone());

        // 区域外的格子
        let key = HexCoord::new(0, 0).to_tile_key();
        assert!(
            (store.get_resource_multiplier(key, "gold") - 1.0).abs() < f64::EPSILON,
            "tile outside zone should have 1.0 multiplier"
        );
    }

    #[test]
    fn test_resource_multiplier_stacks() {
        let mut store = ZoneRuleStore::default();

        // 在同一区域注册两个效果
        let mut tiles = BTreeSet::new();
        let key = HexCoord::new(10, 10).to_tile_key();
        tiles.insert(key);

        store.register(ZoneRule {
            zone_id: "zone_a".to_string(),
            tiles: tiles.clone(),
            effects: vec![ZoneEffect::ResourceMultiplier {
                resource: "wood".to_string(),
                factor: 1.5,
            }],
            active: true,
        });
        store.register(ZoneRule {
            zone_id: "zone_b".to_string(),
            tiles,
            effects: vec![ZoneEffect::ResourceMultiplier {
                resource: "wood".to_string(),
                factor: 2.0,
            }],
            active: true,
        });

        // 1.5 × 2.0 = 3.0
        assert!(
            (store.get_resource_multiplier(key, "wood") - 3.0).abs() < f64::EPSILON,
            "stacked multipliers should multiply: 1.5 * 2.0 = 3.0"
        );
    }

    // -----------------------------------------------------------------------
    // 移动代价倍率
    // -----------------------------------------------------------------------

    #[test]
    fn test_movement_cost_multiplier() {
        let mut store = ZoneRuleStore::default();
        let mut rule = create_test_zone();
        rule.effects
            .push(ZoneEffect::MovementCostMultiplier { factor: 0.5 });
        store.register(rule);

        let key = HexCoord::new(5, 5).to_tile_key();
        assert!(
            (store.get_movement_cost_multiplier(key) - 0.5).abs() < f64::EPSILON,
            "movement cost multiplier should be 0.5"
        );
    }

    #[test]
    fn test_movement_cost_multiplier_outside_zone() {
        let mut store = ZoneRuleStore::default();
        let mut rule = create_test_zone();
        rule.effects
            .push(ZoneEffect::MovementCostMultiplier { factor: 0.5 });
        store.register(rule);

        let key = HexCoord::new(0, 0).to_tile_key();
        assert!(
            (store.get_movement_cost_multiplier(key) - 1.0).abs() < f64::EPSILON,
            "tile outside zone should have 1.0 movement cost multiplier"
        );
    }

    // -----------------------------------------------------------------------
    // 防御加成
    // -----------------------------------------------------------------------

    #[test]
    fn test_defense_bonus() {
        let mut store = ZoneRuleStore::default();
        store.register(create_test_zone());

        let key = HexCoord::new(5, 5).to_tile_key();
        assert!(
            (store.get_defense_bonus(key) - 0.5).abs() < f64::EPSILON,
            "defense bonus should be 0.5"
        );
    }

    #[test]
    fn test_defense_bonus_stacks_additively() {
        let mut store = ZoneRuleStore::default();

        let mut tiles = BTreeSet::new();
        let key = HexCoord::new(10, 10).to_tile_key();
        tiles.insert(key);

        store.register(ZoneRule {
            zone_id: "fort_1".to_string(),
            tiles: tiles.clone(),
            effects: vec![ZoneEffect::DefenseBonus { factor: 0.3 }],
            active: true,
        });
        store.register(ZoneRule {
            zone_id: "fort_2".to_string(),
            tiles,
            effects: vec![ZoneEffect::DefenseBonus { factor: 0.2 }],
            active: true,
        });

        // 0.3 + 0.2 = 0.5
        assert!(
            (store.get_defense_bonus(key) - 0.5).abs() < f64::EPSILON,
            "defense bonuses should add: 0.3 + 0.2 = 0.5"
        );
    }

    // -----------------------------------------------------------------------
    // 通行限制
    // -----------------------------------------------------------------------

    #[test]
    fn test_restrict_access_allowed() {
        let mut store = ZoneRuleStore::default();
        let mut rule = create_test_zone();
        rule.effects.push(ZoneEffect::RestrictAccess {
            allowed_factions: vec!["faction_1".to_string()],
        });
        store.register(rule);

        let key = HexCoord::new(5, 5).to_tile_key();
        assert!(
            store.can_faction_pass(key, &"faction_1".to_string()),
            "allowed faction should pass"
        );
    }

    #[test]
    fn test_restrict_access_denied() {
        let mut store = ZoneRuleStore::default();
        let mut rule = create_test_zone();
        rule.effects.push(ZoneEffect::RestrictAccess {
            allowed_factions: vec!["faction_1".to_string()],
        });
        store.register(rule);

        let key = HexCoord::new(5, 5).to_tile_key();
        assert!(
            !store.can_faction_pass(key, &"faction_2".to_string()),
            "non-allowed faction should be denied"
        );
    }

    #[test]
    fn test_restrict_access_empty_list_allows_all() {
        let mut store = ZoneRuleStore::default();
        let mut rule = create_test_zone();
        rule.effects.push(ZoneEffect::RestrictAccess {
            allowed_factions: vec![],
        });
        store.register(rule);

        let key = HexCoord::new(5, 5).to_tile_key();
        assert!(
            store.can_faction_pass(key, &"anyone".to_string()),
            "empty allowed_factions list should allow everyone"
        );
    }

    // -----------------------------------------------------------------------
    // 非活跃区域
    // -----------------------------------------------------------------------

    #[test]
    fn test_inactive_zone_no_effect() {
        let mut store = ZoneRuleStore::default();
        let mut rule = create_test_zone();
        rule.active = false;
        store.register(rule);

        let key = HexCoord::new(5, 5).to_tile_key();
        assert!(
            (store.get_resource_multiplier(key, "gold") - 1.0).abs() < f64::EPSILON,
            "inactive zone should not apply resource multiplier"
        );
        assert!(
            (store.get_defense_bonus(key) - 0.0).abs() < f64::EPSILON,
            "inactive zone should not apply defense bonus"
        );
        assert!(
            store.can_faction_pass(key, &"anyone".to_string()),
            "inactive zone should not restrict access"
        );
    }

    // -----------------------------------------------------------------------
    // 注册/移除
    // -----------------------------------------------------------------------

    #[test]
    fn test_register_and_remove() {
        let mut store = ZoneRuleStore::default();
        store.register(create_test_zone());
        assert!(store.rules.contains_key("test_zone"));

        let removed = store.remove("test_zone");
        assert!(removed.is_some());
        assert!(!store.rules.contains_key("test_zone"));

        // 移除不存在的区域返回 None
        assert!(store.remove("nonexistent").is_none());
    }

    #[test]
    fn test_register_overwrite() {
        let mut store = ZoneRuleStore::default();
        store.register(create_test_zone());

        // 用同 zone_id 覆盖
        let mut tiles = BTreeSet::new();
        tiles.insert(HexCoord::new(10, 10).to_tile_key());
        store.register(ZoneRule {
            zone_id: "test_zone".to_string(),
            tiles,
            effects: vec![],
            active: true,
        });

        // 旧格子不再受影响
        let old_key = HexCoord::new(5, 5).to_tile_key();
        assert!(
            (store.get_resource_multiplier(old_key, "gold") - 1.0).abs() < f64::EPSILON,
            "old tile should no longer be affected after overwrite"
        );
    }

    // -----------------------------------------------------------------------
    // get_effects_for_tile
    // -----------------------------------------------------------------------

    #[test]
    fn test_get_effects_for_tile_returns_correct_effects() {
        let mut store = ZoneRuleStore::default();
        store.register(create_test_zone());

        let key = HexCoord::new(5, 5).to_tile_key();
        let effects = store.get_effects_for_tile(key);
        assert_eq!(effects.len(), 2, "zone has 2 effects");
    }

    #[test]
    fn test_get_effects_for_tile_empty_outside_zone() {
        let mut store = ZoneRuleStore::default();
        store.register(create_test_zone());

        let key = HexCoord::new(0, 0).to_tile_key();
        let effects = store.get_effects_for_tile(key);
        assert!(
            effects.is_empty(),
            "tile outside zone should have no effects"
        );
    }
}
