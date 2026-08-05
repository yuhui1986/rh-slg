//! 建筑系统 (M8)
//!
//! 5 种建筑 × 3 级，每种建筑每级有不同效果：
//! - Farm (农田)       : +food
//! - LumberMill (伐木) : +wood
//! - Mine (矿场)       : +iron +stone
//! - Barracks (兵营)   : 派兵上限 +1 (M8 简化: 1 队就行, 不严格 enforce)
//! - CityWall (城防)   : 驻防值加成 (战斗时 defender 兵力 × (1 + bonus))
//!
//! 升级规则：
//! - L1 → L2: 消耗 100 gold + 50 food
//! - L2 → L3: 消耗 200 gold + 100 food + 50 wood
//!
//! 占用：1 个 hex 只能建 1 个建筑（M0 简化：不同建筑不共存）

use serde::{Deserialize, Serialize};
use slg_data::ids::FactionId;

// ---------------------------------------------------------------------------
// BuildingType
// ---------------------------------------------------------------------------

/// 建筑类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BuildingType {
    /// 农田 — 产 food
    Farm,
    /// 伐木场 — 产 wood
    LumberMill,
    /// 矿场 — 产 iron + stone
    Mine,
    /// 兵营 — 派兵上限 +1 (M8 暂不严格 enforce, 占位)
    Barracks,
    /// 城防 — 战斗时 defender 兵力加成
    CityWall,
}

impl BuildingType {
    /// 所有建筑类型（顺序稳定, UI 用）
    pub const ALL: [BuildingType; 5] = [
        BuildingType::Farm,
        BuildingType::LumberMill,
        BuildingType::Mine,
        BuildingType::Barracks,
        BuildingType::CityWall,
    ];

    /// 中文显示名
    pub fn display_name(&self) -> &'static str {
        match self {
            BuildingType::Farm => "农田",
            BuildingType::LumberMill => "伐木场",
            BuildingType::Mine => "矿场",
            BuildingType::Barracks => "兵营",
            BuildingType::CityWall => "城防",
        }
    }
}

// ---------------------------------------------------------------------------
// 效果
// ---------------------------------------------------------------------------

/// 资源产出加成 (per tick, per building, per level)
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceBonus {
    pub food: u32,
    pub wood: u32,
    pub iron: u32,
    pub stone: u32,
}

/// 战斗加成
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct CombatBonus {
    /// defender 兵力倍率 (1.0 = 不加, 1.5 = +50%)
    /// 用于 CityWall: 0.3/level
    pub defender_troop_multiplier: f64,
    /// 派兵上限加成 (Barracks 用, M8 暂不严格 enforce)
    pub march_capacity_bonus: u8,
}

/// 建筑当前等级效果
pub fn effect_at(btype: BuildingType, level: u8) -> (ResourceBonus, CombatBonus) {
    let resource = match btype {
        BuildingType::Farm => ResourceBonus {
            food: match level {
                1 => 2,
                2 => 5,
                _ => 10, // L3+
            },
            ..Default::default()
        },
        BuildingType::LumberMill => ResourceBonus {
            wood: match level {
                1 => 2,
                2 => 5,
                _ => 10,
            },
            ..Default::default()
        },
        BuildingType::Mine => ResourceBonus {
            iron: match level {
                1 => 2,
                2 => 4,
                _ => 7,
            },
            stone: match level {
                1 => 1,
                2 => 2,
                _ => 4,
            },
            ..Default::default()
        },
        BuildingType::Barracks => ResourceBonus::default(),
        BuildingType::CityWall => ResourceBonus::default(),
    };

    let combat = match btype {
        BuildingType::Barracks => CombatBonus {
            defender_troop_multiplier: 1.0,
            march_capacity_bonus: 1,
        },
        BuildingType::CityWall => CombatBonus {
            defender_troop_multiplier: 1.0 + 0.3 * level as f64,
            march_capacity_bonus: 0,
        },
        _ => CombatBonus::default(),
    };

    (resource, combat)
}

// ---------------------------------------------------------------------------
// Building
// ---------------------------------------------------------------------------

/// 建筑
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Building {
    pub btype: BuildingType,
    /// 1-3, M8 简化上限
    pub level: u8,
    pub owner: FactionId,
}

impl Building {
    pub fn new(btype: BuildingType, level: u8, owner: FactionId) -> Self {
        debug_assert!((1..=3).contains(&level), "level 必须在 1..=3");
        Self { btype, level, owner }
    }

    /// 该建筑当前 tick 的资源加成
    pub fn resource_bonus(&self) -> ResourceBonus {
        effect_at(self.btype, self.level).0
    }

    /// 该建筑当前等级的战斗加成
    pub fn combat_bonus(&self) -> CombatBonus {
        effect_at(self.btype, self.level).1
    }
}

// ---------------------------------------------------------------------------
// 升级 + 建 L1 成本
// ---------------------------------------------------------------------------

/// 建 L1 建筑成本 (每种建筑)
pub fn build_cost(_btype: BuildingType) -> crate::entity::faction::FactionResources {
    crate::entity::faction::FactionResources {
        gold: 50,
        food: 0,
        wood: 0,
        iron: 0,
        stone: 0,
        troops: 0,
    }
}

/// 升级所需资源
///
/// L1 → L2: 100 gold + 50 food
/// L2 → L3: 200 gold + 100 food + 50 wood
pub fn upgrade_cost(current_level: u8) -> Option<crate::entity::faction::FactionResources> {
    match current_level {
        1 => Some(crate::entity::faction::FactionResources {
            gold: 100,
            food: 50,
            wood: 0,
            iron: 0,
            stone: 0,
            troops: 0,
        }),
        2 => Some(crate::entity::faction::FactionResources {
            gold: 200,
            food: 100,
            wood: 50,
            iron: 0,
            stone: 0,
            troops: 0,
        }),
        _ => None, // L3 已满
    }
}

/// 升级：扣除资源 + level +1
///
/// 如果资源不足或已满级, 返回 `Err`。
pub fn try_upgrade(
    building: &mut Building,
    faction_resources: &mut crate::entity::faction::FactionResources,
) -> Result<u8, UpgradeError> {
    let cost = upgrade_cost(building.level).ok_or(UpgradeError::MaxLevel)?;
    if faction_resources.gold < cost.gold
        || faction_resources.food < cost.food
        || faction_resources.wood < cost.wood
    {
        return Err(UpgradeError::InsufficientResources);
    }
    faction_resources.gold -= cost.gold;
    faction_resources.food -= cost.food;
    faction_resources.wood -= cost.wood;
    building.level += 1;
    Ok(building.level)
}

/// 升级错误
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpgradeError {
    /// 已满级 (L3)
    MaxLevel,
    /// 资源不足
    InsufficientResources,
}

// ---------------------------------------------------------------------------
// BuildingManager
// ---------------------------------------------------------------------------

use std::collections::BTreeMap;

use crate::map::grid::HexCoord;

/// 建筑管理器：按 hex 坐标索引所有建筑
///
/// 1 个 hex 最多 1 个建筑（不同 type 不共存）
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BuildingManager {
    /// coord -> building
    pub buildings: BTreeMap<HexCoord, Building>,
}

impl BuildingManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// 在 coord 上建 1 个 L1 建筑 (扣资源)
    ///
    /// 失败条件：coord 已有建筑 / 资源不足
    pub fn build(
        &mut self,
        coord: HexCoord,
        btype: BuildingType,
        owner: FactionId,
        owner_resources: &mut crate::entity::faction::FactionResources,
    ) -> Result<&Building, BuildError> {
        if self.buildings.contains_key(&coord) {
            return Err(BuildError::AlreadyBuilt);
        }
        let cost = build_cost(btype);
        if owner_resources.gold < cost.gold {
            return Err(BuildError::InsufficientResources);
        }
        owner_resources.gold -= cost.gold;
        self.buildings
            .insert(coord, Building::new(btype, 1, owner));
        Ok(self.buildings.get(&coord).unwrap())
    }

    /// 获取 coord 上的建筑
    pub fn get(&self, coord: HexCoord) -> Option<&Building> {
        self.buildings.get(&coord)
    }

    /// 获取 coord 上的建筑 (mutable)
    pub fn get_mut(&mut self, coord: HexCoord) -> Option<&mut Building> {
        self.buildings.get_mut(&coord)
    }

    /// 拆除建筑
    pub fn demolish(&mut self, coord: HexCoord) -> Option<Building> {
        self.buildings.remove(&coord)
    }

    /// 升级 coord 上的建筑, 扣除 owner 的资源
    ///
    /// `owner_resources` 是该建筑 owner 的资源 (调用方负责查找)
    pub fn upgrade(
        &mut self,
        coord: HexCoord,
        owner_resources: &mut crate::entity::faction::FactionResources,
    ) -> Result<u8, UpgradeError> {
        let building = self.get_mut(coord).ok_or(UpgradeError::MaxLevel)?;
        try_upgrade(building, owner_resources)
    }

    /// 计算某势力的总资源加成 (per tick)
    ///
    /// 遍历所有该 owner 的建筑, 累加 resource_bonus
    pub fn total_resource_bonus_for(&self, owner: &FactionId) -> ResourceBonus {
        let mut total = ResourceBonus::default();
        for b in self.buildings.values() {
            if &b.owner == owner {
                let bonus = b.resource_bonus();
                total.food += bonus.food;
                total.wood += bonus.wood;
                total.iron += bonus.iron;
                total.stone += bonus.stone;
            }
        }
        total
    }

    /// 计算某势力在某 hex 的战斗加成 (城防 / 兵营)
    ///
    /// 主要用于 handle_combat 的 defender 兵力加成
    pub fn combat_bonus_at(&self, coord: HexCoord) -> CombatBonus {
        // 起始 1.0 (defender_troop_multiplier 默认 = 1.0)
        let mut total = CombatBonus {
            defender_troop_multiplier: 1.0,
            march_capacity_bonus: 0,
        };
        if let Some(b) = self.buildings.get(&coord) {
            let bonus = b.combat_bonus();
            // bonus.defender_troop_multiplier = 1.0 + 加成
            // total = 1.0 + (bonus - 1.0) = bonus
            total.defender_troop_multiplier =
                1.0 + (bonus.defender_troop_multiplier - 1.0);
            total.march_capacity_bonus += bonus.march_capacity_bonus;
        }
        total
    }
}

/// 建建筑错误
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildError {
    /// 该 hex 已有建筑
    AlreadyBuilt,
    /// 资源不足
    InsufficientResources,
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;

    #[test]
    fn test_building_type_display() {
        assert_eq!(BuildingType::Farm.display_name(), "农田");
        assert_eq!(BuildingType::LumberMill.display_name(), "伐木场");
        assert_eq!(BuildingType::Mine.display_name(), "矿场");
    }

    #[test]
    fn test_effect_farm_per_level() {
        let (l1, _) = effect_at(BuildingType::Farm, 1);
        let (l2, _) = effect_at(BuildingType::Farm, 2);
        let (l3, _) = effect_at(BuildingType::Farm, 3);
        assert_eq!(l1.food, 2);
        assert_eq!(l2.food, 5);
        assert_eq!(l3.food, 10);
    }

    #[test]
    fn test_effect_city_wall_defender_multiplier() {
        let (_, l1) = effect_at(BuildingType::CityWall, 1);
        let (_, l2) = effect_at(BuildingType::CityWall, 2);
        let (_, l3) = effect_at(BuildingType::CityWall, 3);
        assert!((l1.defender_troop_multiplier - 1.3).abs() < 1e-9);
        assert!((l2.defender_troop_multiplier - 1.6).abs() < 1e-9);
        assert!((l3.defender_troop_multiplier - 1.9).abs() < 1e-9);
    }

    #[test]
    fn test_upgrade_cost_progression() {
        let c1 = upgrade_cost(1).unwrap();
        assert_eq!(c1.gold, 100);
        assert_eq!(c1.food, 50);
        let c2 = upgrade_cost(2).unwrap();
        assert_eq!(c2.gold, 200);
        assert_eq!(c2.food, 100);
        assert_eq!(c2.wood, 50);
        // L3 已满
        assert!(upgrade_cost(3).is_none());
    }

    #[test]
    fn test_try_upgrade_success() {
        let mut building = Building::new(BuildingType::Farm, 1, "faction_1".to_string());
        let mut res = crate::entity::faction::FactionResources {
            gold: 200,
            food: 100,
            ..Default::default()
        };
        let new_level = try_upgrade(&mut building, &mut res).unwrap();
        assert_eq!(new_level, 2);
        assert_eq!(building.level, 2);
        assert_eq!(res.gold, 100); // 200 - 100
        assert_eq!(res.food, 50); // 100 - 50
    }

    #[test]
    fn test_try_upgrade_insufficient_resources() {
        let mut building = Building::new(BuildingType::Farm, 1, "faction_1".to_string());
        let mut res = crate::entity::faction::FactionResources {
            gold: 50, // 不足
            food: 100,
            ..Default::default()
        };
        let result = try_upgrade(&mut building, &mut res);
        assert_eq!(result, Err(UpgradeError::InsufficientResources));
        assert_eq!(building.level, 1, "level 不变");
    }

    #[test]
    fn test_try_upgrade_max_level() {
        let mut building = Building::new(BuildingType::Farm, 3, "faction_1".to_string());
        let mut res = crate::entity::faction::FactionResources::default();
        let result = try_upgrade(&mut building, &mut res);
        assert_eq!(result, Err(UpgradeError::MaxLevel));
    }

    #[test]
    fn test_building_manager_build_and_get() {
        let mut mgr = BuildingManager::new();
        let coord = HexCoord::new(5, 5);
        let mut res = crate::entity::faction::FactionResources::default();
        res.gold = 100;
        mgr.build(coord, BuildingType::Farm, "faction_1".to_string(), &mut res)
            .unwrap();
        let b = mgr.get(coord).unwrap();
        assert_eq!(b.btype, BuildingType::Farm);
        assert_eq!(b.level, 1);
    }

    #[test]
    fn test_building_manager_already_built() {
        let mut mgr = BuildingManager::new();
        let coord = HexCoord::new(5, 5);
        let mut res = crate::entity::faction::FactionResources::default();
        res.gold = 100;
        mgr.build(coord, BuildingType::Farm, "faction_1".to_string(), &mut res)
            .unwrap();
        let result = mgr.build(
            coord,
            BuildingType::Mine,
            "faction_1".to_string(),
            &mut res,
        );
        assert_eq!(result, Err(BuildError::AlreadyBuilt));
    }

    #[test]
    fn test_building_manager_demolish() {
        let mut mgr = BuildingManager::new();
        let coord = HexCoord::new(5, 5);
        let mut res = crate::entity::faction::FactionResources::default();
        res.gold = 100;
        mgr.build(coord, BuildingType::Farm, "faction_1".to_string(), &mut res)
            .unwrap();
        let b = mgr.demolish(coord).unwrap();
        assert_eq!(b.btype, BuildingType::Farm);
        assert!(mgr.get(coord).is_none());
    }

    #[test]
    fn test_total_resource_bonus_for_owner() {
        let mut mgr = BuildingManager::new();
        let mut res = crate::entity::faction::FactionResources::default();
        res.gold = 1000;
        // faction_1: 1 农田 L1 + 1 伐木场 L2
        mgr.build(
            HexCoord::new(0, 0),
            BuildingType::Farm,
            "faction_1".to_string(),
            &mut res,
        )
        .unwrap();
        mgr.build(
            HexCoord::new(1, 0),
            BuildingType::LumberMill,
            "faction_1".to_string(),
            &mut res,
        )
        .unwrap();
        // 升级伐木场到 L2
        mgr.upgrade(
            HexCoord::new(1, 0),
            &mut crate::entity::faction::FactionResources {
                gold: 1000,
                food: 1000,
                ..Default::default()
            },
        )
        .unwrap();
        // faction_2: 1 矿场 L1 (不算)
        mgr.build(
            HexCoord::new(10, 10),
            BuildingType::Mine,
            "faction_2".to_string(),
            &mut res,
        )
        .unwrap();

        let bonus = mgr.total_resource_bonus_for(&"faction_1".to_string());
        assert_eq!(bonus.food, 2, "1 农田 L1");
        assert_eq!(bonus.wood, 5, "1 伐木场 L2");
        assert_eq!(bonus.iron, 0);
    }

    #[test]
    fn test_combat_bonus_at_with_city_wall_l2() {
        let mut mgr = BuildingManager::new();
        let coord = HexCoord::new(0, 0);
        let mut res = crate::entity::faction::FactionResources::default();
        res.gold = 100;
        mgr.build(coord, BuildingType::CityWall, "faction_1".to_string(), &mut res)
            .unwrap();
        mgr.upgrade(
            coord,
            &mut crate::entity::faction::FactionResources {
                gold: 1000,
                food: 1000,
                ..Default::default()
            },
        )
        .unwrap();
        let bonus = mgr.combat_bonus_at(coord);
        assert!(
            (bonus.defender_troop_multiplier - 1.6).abs() < 1e-9,
            "L2 城防 defender 兵力 × 1.6"
        );
    }

    #[test]
    fn test_build_insufficient_resources() {
        let mut mgr = BuildingManager::new();
        let mut res = crate::entity::faction::FactionResources::default();
        // gold = 0 < 50 (build_cost)
        let result = mgr.build(
            HexCoord::new(0, 0),
            BuildingType::Farm,
            "faction_1".to_string(),
            &mut res,
        );
        assert_eq!(result, Err(BuildError::InsufficientResources));
    }
}
