//! 经济系统：圈地产资源
//!
//! 率土核心：圈地 = 资源。**每 tick** 圈地里的每一格按地形 + 资源格 累加产量。
//!
//! 流程（在 `process_tick_phases` 的 `ResourceProduction` 阶段调用）：
//! 1. 遍历每个势力的 owner_map（圈地）
//! 2. 对每格查 `terrain` + `resource`（来自 `MapDocument.resources`）→ 算 `ResourceProduction`
//! 3. 累加到势力 `FactionResources`
//!
//! 产量表（每 tick 基础产量，资源格再 ×2）：
//! - Plains:     food +5
//! - Forest:     wood +3
//! - Mountain:   iron +2, stone +2
//! - Hills:      food +2, wood +1
//! - Swamp:      food +1
//! - Water/Desert/Pass: 0（不可产）
//!
//! 资源格加成：基础产量 × 2（资源类型与基础产量叠加）

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use slg_data::ids::{FactionId, TileKey};

use crate::map::tile::{ResourceType, TerrainType};

// ---------------------------------------------------------------------------
// ResourceProduction：单次 tick 的产量增量
// ---------------------------------------------------------------------------

/// 一次 tick 产出的资源增量
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceProduction {
    pub gold: u64,
    pub food: u64,
    pub wood: u64,
    pub iron: u64,
    pub stone: u64,
}

impl ResourceProduction {
    pub fn zero() -> Self {
        Self::default()
    }

    /// 把另一个 production 加到自己身上
    pub fn add_assign(&mut self, other: &Self) {
        self.gold += other.gold;
        self.food += other.food;
        self.wood += other.wood;
        self.iron += other.iron;
        self.stone += other.stone;
    }
}

// ---------------------------------------------------------------------------
// 产量表：terrain → ResourceProduction
// ---------------------------------------------------------------------------

/// 地形的基础产量（每 tick 增量，不含资源格加成）
///
/// 资源类型枚举穷举：8 个 terrain type 都覆盖到
pub fn base_production(terrain: TerrainType) -> ResourceProduction {
    match terrain {
        TerrainType::Plains => ResourceProduction { food: 5, ..Default::default() },
        TerrainType::Forest => ResourceProduction { wood: 3, ..Default::default() },
        TerrainType::Mountain => ResourceProduction { iron: 2, stone: 2, ..Default::default() },
        TerrainType::Hills => ResourceProduction { food: 2, wood: 1, ..Default::default() },
        TerrainType::Swamp => ResourceProduction { food: 1, ..Default::default() },
        TerrainType::Water | TerrainType::Desert | TerrainType::Pass => ResourceProduction::zero(),
    }
}

/// 资源格加成：在基础产量上 × 2
///
/// 资源类型与基础产量叠加（同种资源 ×2，不同资源相加）：
/// - Gold resource: gold += base.gold*2 + 其他 base
/// - Food resource: food += base.food*2
/// - 等等
pub fn resource_bonus(base: ResourceProduction, resource: ResourceType) -> ResourceProduction {
    let multiplier = 2u64;
    match resource {
        ResourceType::Gold => ResourceProduction {
            gold: base.gold * multiplier,
            food: base.food,
            wood: base.wood,
            iron: base.iron,
            stone: base.stone,
        },
        ResourceType::Food => ResourceProduction {
            gold: base.gold,
            food: base.food * multiplier,
            wood: base.wood,
            iron: base.iron,
            stone: base.stone,
        },
        ResourceType::Wood => ResourceProduction {
            gold: base.gold,
            food: base.food,
            wood: base.wood * multiplier,
            iron: base.iron,
            stone: base.stone,
        },
        ResourceType::Iron => ResourceProduction {
            gold: base.gold,
            food: base.food,
            wood: base.wood,
            iron: base.iron * multiplier,
            stone: base.stone,
        },
        ResourceType::Stone => ResourceProduction {
            gold: base.gold,
            food: base.food,
            wood: base.wood,
            iron: base.iron,
            stone: base.stone * multiplier,
        },
    }
}

/// 计算单格的产量（地形 + 资源格）
pub fn tile_production(terrain: TerrainType, resource: Option<ResourceType>) -> ResourceProduction {
    let base = base_production(terrain);
    match resource {
        Some(r) => resource_bonus(base, r),
        None => base,
    }
}

// ---------------------------------------------------------------------------
// 全局 tick 推进
// ---------------------------------------------------------------------------

/// 推进一圈地产：遍历所有圈地，累加每个势力的产量
///
/// # 参数
/// - `terrain_map`: TileKey → TerrainType（必须包含所有被圈地的格）
/// - `tile_resources`: TileKey → ResourceType（可选资源格；缺 key = None）
/// - `owner_map`: 圈地归属，TileKey → FactionId
///
/// # 返回
/// BTreeMap<FactionId, ResourceProduction>：每个势力在这次 tick 的产量增量
pub fn tick_resource_production(
    terrain_map: &BTreeMap<TileKey, TerrainType>,
    tile_resources: &BTreeMap<TileKey, ResourceType>,
    owner_map: &BTreeMap<TileKey, FactionId>,
) -> BTreeMap<FactionId, ResourceProduction> {
    let mut out: BTreeMap<FactionId, ResourceProduction> = BTreeMap::new();
    for (key, faction) in owner_map {
        let terrain = match terrain_map.get(key) {
            Some(t) => *t,
            None => continue, // 没注册 = 跳过
        };
        let resource = tile_resources.get(key).copied();
        let prod = tile_production(terrain, resource);
        out.entry(faction.clone())
            .or_insert_with(ResourceProduction::zero)
            .add_assign(&prod);
    }
    out
}

/// 把 tick 产量应用到 FactionResources
pub fn apply_production(
    resources: &mut crate::entity::faction::FactionResources,
    production: &ResourceProduction,
) {
    resources.gold = resources.gold.saturating_add(production.gold);
    resources.food = resources.food.saturating_add(production.food);
    resources.wood = resources.wood.saturating_add(production.wood);
    resources.iron = resources.iron.saturating_add(production.iron);
    resources.stone = resources.stone.saturating_add(production.stone);
}

/// 把 `Building::resource_bonus()` (M8 ResourceBonus) 转 ResourceProduction
pub fn from_building_bonus(bonus: &crate::building::ResourceBonus) -> ResourceProduction {
    ResourceProduction {
        gold: 0,
        food: u64::from(bonus.food),
        wood: u64::from(bonus.wood),
        iron: u64::from(bonus.iron),
        stone: u64::from(bonus.stone),
    }
}

/// M8: 收集某 faction 的所有建筑资源加成, 应用到 FactionResources
///
/// # 参数
/// - `building_manager`: 查所有该 faction 的建筑
/// - `faction_id`: 要查的 faction
/// - `faction_resources`: 目标资源 (直接加)
pub fn apply_building_production(
    building_manager: &crate::building::BuildingManager,
    faction_id: &slg_data::ids::FactionId,
    faction_resources: &mut crate::entity::faction::FactionResources,
) {
    let total = building_manager.total_resource_bonus_for(faction_id);
    let production = from_building_bonus(&total);
    apply_production(faction_resources, &production);
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::grid::HexCoord;

    fn key(q: i32, r: i32) -> TileKey {
        HexCoord::new(q, r).to_tile_key()
    }

    #[test]
    fn test_base_production_all_terrains() {
        assert_eq!(base_production(TerrainType::Plains).food, 5);
        assert_eq!(base_production(TerrainType::Plains).gold, 0);
        assert_eq!(base_production(TerrainType::Forest).wood, 3);
        assert_eq!(base_production(TerrainType::Mountain).iron, 2);
        assert_eq!(base_production(TerrainType::Mountain).stone, 2);
        assert_eq!(base_production(TerrainType::Hills).food, 2);
        assert_eq!(base_production(TerrainType::Hills).wood, 1);
        assert_eq!(base_production(TerrainType::Swamp).food, 1);
        // Water / Desert / Pass 0 产出
        assert_eq!(base_production(TerrainType::Water), ResourceProduction::zero());
        assert_eq!(base_production(TerrainType::Desert), ResourceProduction::zero());
        assert_eq!(base_production(TerrainType::Pass), ResourceProduction::zero());
    }

    #[test]
    fn test_resource_bonus_doubles_one_resource() {
        // Plains food +5, 加 Food resource -> food +10
        let base = base_production(TerrainType::Plains);
        let bonus = resource_bonus(base, ResourceType::Food);
        assert_eq!(bonus.food, 10);
        assert_eq!(bonus.gold, 0);

        // Mountain iron +2 stone +2, 加 Iron resource -> iron +4 stone +2
        let base = base_production(TerrainType::Mountain);
        let bonus = resource_bonus(base, ResourceType::Iron);
        assert_eq!(bonus.iron, 4);
        assert_eq!(bonus.stone, 2);
    }

    #[test]
    fn test_tile_production_no_resource() {
        let p = tile_production(TerrainType::Plains, None);
        assert_eq!(p.food, 5);
    }

    #[test]
    fn test_tile_production_with_resource() {
        let p = tile_production(TerrainType::Plains, Some(ResourceType::Food));
        assert_eq!(p.food, 10);
    }

    #[test]
    fn test_tick_resource_production_basic() {
        // 玩家占 1 个 Plains，1 个 Mountain
        let mut terrain = BTreeMap::new();
        terrain.insert(key(10, 10), TerrainType::Plains);
        terrain.insert(key(11, 10), TerrainType::Mountain);

        let resources: BTreeMap<TileKey, ResourceType> = BTreeMap::new();

        let mut owner = BTreeMap::new();
        owner.insert(key(10, 10), "faction_1".to_string());
        owner.insert(key(11, 10), "faction_1".to_string());

        let out = tick_resource_production(&terrain, &resources, &owner);
        let prod = out.get("faction_1").unwrap();
        assert_eq!(prod.food, 5);  // Plains
        assert_eq!(prod.iron, 2);  // Mountain
        assert_eq!(prod.stone, 2); // Mountain
    }

    #[test]
    fn test_tick_resource_production_multi_faction() {
        let mut terrain = BTreeMap::new();
        terrain.insert(key(0, 0), TerrainType::Plains);
        terrain.insert(key(1, 0), TerrainType::Forest);
        terrain.insert(key(2, 0), TerrainType::Hills);

        let owner: BTreeMap<TileKey, FactionId> = vec![
            (key(0, 0), "faction_1".to_string()),
            (key(1, 0), "faction_2".to_string()),
            (key(2, 0), "faction_2".to_string()),
        ]
        .into_iter()
        .collect();

        let out = tick_resource_production(&terrain, &BTreeMap::new(), &owner);
        assert_eq!(out.get("faction_1").unwrap().food, 5);
        // faction_2: forest wood 3 + hills food 2 wood 1 = wood 4 food 2
        assert_eq!(out.get("faction_2").unwrap().wood, 4);
        assert_eq!(out.get("faction_2").unwrap().food, 2);
    }

    #[test]
    fn test_tick_resource_production_with_resource_tile() {
        // 玩家占 1 个 Plains + 1 个 Plains+Food resource
        let mut terrain = BTreeMap::new();
        terrain.insert(key(0, 0), TerrainType::Plains);
        terrain.insert(key(1, 0), TerrainType::Plains);

        let mut resources = BTreeMap::new();
        resources.insert(key(1, 0), ResourceType::Food);

        let mut owner = BTreeMap::new();
        owner.insert(key(0, 0), "faction_1".to_string());
        owner.insert(key(1, 0), "faction_1".to_string());

        let out = tick_resource_production(&terrain, &resources, &owner);
        // 0,0: food 5; 1,0: food 10 (×2) -> total 15
        assert_eq!(out.get("faction_1").unwrap().food, 15);
    }

    #[test]
    fn test_apply_production() {
        let mut r = crate::entity::faction::FactionResources {
            gold: 100,
            food: 50,
            ..Default::default()
        };
        apply_production(&mut r, &ResourceProduction { gold: 10, food: 5, ..Default::default() });
        assert_eq!(r.gold, 110);
        assert_eq!(r.food, 55);
    }

    #[test]
    fn test_apply_production_saturating() {
        // 避免 overflow
        let mut r = crate::entity::faction::FactionResources {
            gold: u64::MAX,
            ..Default::default()
        };
        apply_production(&mut r, &ResourceProduction { gold: 1, ..Default::default() });
        assert_eq!(r.gold, u64::MAX); // saturating_add
    }

    #[test]
    fn test_tick_resource_production_skip_unregistered_terrain() {
        // 圈地里有 hex 但 terrain_map 没注册 -> 跳过（faction 不出现在 out）
        let terrain: BTreeMap<TileKey, TerrainType> = BTreeMap::new();
        let owner: BTreeMap<TileKey, FactionId> =
            vec![(key(99, 99), "faction_1".to_string())].into_iter().collect();
        let out = tick_resource_production(&terrain, &BTreeMap::new(), &owner);
        assert!(
            !out.contains_key("faction_1"),
            "unregistered terrain 不应产生 faction entry"
        );
    }
}
