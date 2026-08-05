//! 分城系统 (M8)
//!
//! 玩家主城外, 满足条件时可建"分城" (二级城池)。
//! 分城有独立 `buildings` 列表, 仍归属同一 faction。
//!
//! 限制：
//! - 每个 faction 最多 N 个分城 (M8 简化: 2 个)
//! - 建分城要求：周围 1 hex 邻接有 ≥6 个己方格 (充分控制)
//! - 消耗资源：500 gold + 200 food + 100 wood
//!
//! 分城 vs 主城：
//! - 主城: 唯一, 玩家出生点, 不能被拆除
//! - 分城: 可建可拆, 提供额外建筑槽

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use slg_data::ids::FactionId;

use crate::building::Building;
use crate::map::grid::HexCoord;

// ---------------------------------------------------------------------------
// 常量
// ---------------------------------------------------------------------------

/// 每个 faction 最多分城数 (M8 简化)
pub const MAX_SUBCITIES_PER_FACTION: u8 = 2;

/// 建分城要求：周围 1 hex 邻接有 ≥`REQUIRED_NEIGHBOR_TILES` 个己方格
pub const REQUIRED_NEIGHBOR_TILES: usize = 6;

/// 建分城消耗
pub fn establish_subcity_cost() -> crate::entity::faction::FactionResources {
    crate::entity::faction::FactionResources {
        gold: 500,
        food: 200,
        wood: 100,
        iron: 0,
        stone: 0,
        troops: 0,
    }
}

// ---------------------------------------------------------------------------
// City
// ---------------------------------------------------------------------------

/// 城池 (主城 + 分城)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct City {
    /// 城池坐标
    pub coord: HexCoord,
    /// 归属
    pub owner: FactionId,
    /// 是否主城 (主城不可拆除)
    pub is_main: bool,
    /// 城池内建筑列表 (M8 简化: 直接 Vec, 不分 slot)
    pub buildings: Vec<Building>,
}

impl City {
    pub fn new_main(coord: HexCoord, owner: FactionId) -> Self {
        Self {
            coord,
            owner,
            is_main: true,
            buildings: Vec::new(),
        }
    }

    pub fn new_subcity(coord: HexCoord, owner: FactionId) -> Self {
        Self {
            coord,
            owner,
            is_main: false,
            buildings: Vec::new(),
        }
    }

    /// 加一个建筑
    pub fn add_building(&mut self, b: Building) {
        self.buildings.push(b);
    }

    /// 移除建筑 (按 index)
    pub fn remove_building(&mut self, idx: usize) -> Option<Building> {
        if idx < self.buildings.len() {
            Some(self.buildings.remove(idx))
        } else {
            None
        }
    }
}

// ---------------------------------------------------------------------------
// CityManager
// ---------------------------------------------------------------------------

/// 城池管理器：按 coord 索引所有城池 (主城 + 分城)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CityManager {
    pub cities: BTreeMap<HexCoord, City>,
}

impl CityManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// 注册一个主城 (faction 出生时)
    pub fn register_main_city(&mut self, coord: HexCoord, owner: FactionId) {
        self.cities
            .insert(coord, City::new_main(coord, owner));
    }

    /// 获取 coord 上的城池
    pub fn get(&self, coord: HexCoord) -> Option<&City> {
        self.cities.get(&coord)
    }

    /// 获取 coord 上的城池 (mutable)
    pub fn get_mut(&mut self, coord: HexCoord) -> Option<&mut City> {
        self.cities.get_mut(&coord)
    }

    /// 某 faction 拥有的分城数
    pub fn subcity_count_for(&self, owner: &FactionId) -> u8 {
        self.cities
            .values()
            .filter(|c| !c.is_main && &c.owner == owner)
            .count() as u8
    }

    /// 是否可以建分城 (条件检查)
    ///
    /// 条件：
    /// 1. coord 已被 owner 占领 (territory 端)
    /// 2. coord 上没有现存城池
    /// 3. coord 周围 1 hex 邻接有 ≥`REQUIRED_NEIGHBOR_TILES` 个 owner 的格
    /// 4. owner 现有分城数 < `MAX_SUBCITIES_PER_FACTION`
    pub fn can_establish_subcity(
        &self,
        coord: HexCoord,
        owner: &FactionId,
        owner_territory_keys: &std::collections::BTreeSet<u64>,
    ) -> Result<(), EstablishError> {
        // 1. coord 是 owner 的地
        if !owner_territory_keys.contains(&coord.to_tile_key()) {
            return Err(EstablishError::NotOwner);
        }
        // 2. coord 没有现有城池
        if self.cities.contains_key(&coord) {
            return Err(EstablishError::AlreadyCity);
        }
        // 3. 周围 6 邻接有 ≥ 6 个 owner 格
        let neighbor_count = coord
            .neighbors()
            .iter()
            .filter(|n| owner_territory_keys.contains(&n.to_tile_key()))
            .count();
        if neighbor_count < REQUIRED_NEIGHBOR_TILES {
            return Err(EstablishError::InsufficientTerritory);
        }
        // 4. 分城数 < 上限
        if self.subcity_count_for(owner) >= MAX_SUBCITIES_PER_FACTION {
            return Err(EstablishError::SubcityLimitReached);
        }
        Ok(())
    }

    /// 建分城 (扣除资源)
    pub fn establish_subcity(
        &mut self,
        coord: HexCoord,
        owner: FactionId,
        owner_resources: &mut crate::entity::faction::FactionResources,
        owner_territory_keys: &std::collections::BTreeSet<u64>,
    ) -> Result<&City, EstablishError> {
        self.can_establish_subcity(coord, &owner, owner_territory_keys)?;
        let cost = establish_subcity_cost();
        if owner_resources.gold < cost.gold
            || owner_resources.food < cost.food
            || owner_resources.wood < cost.wood
        {
            return Err(EstablishError::InsufficientResources);
        }
        owner_resources.gold -= cost.gold;
        owner_resources.food -= cost.food;
        owner_resources.wood -= cost.wood;
        self.cities
            .insert(coord, City::new_subcity(coord, owner));
        Ok(self.cities.get(&coord).unwrap())
    }

    /// 拆除分城 (主城不能拆)
    ///
    /// 返还 50% 资源
    pub fn demolish_subcity(
        &mut self,
        coord: HexCoord,
        owner_resources: &mut crate::entity::faction::FactionResources,
    ) -> Result<City, DemolishError> {
        let city = self.cities.get(&coord).ok_or(DemolishError::NotACity)?;
        if city.is_main {
            return Err(DemolishError::IsMain);
        }
        let city = self.cities.remove(&coord).unwrap();
        // 返还 50% 建分城成本
        let cost = establish_subcity_cost();
        owner_resources.gold += cost.gold / 2;
        owner_resources.food += cost.food / 2;
        owner_resources.wood += cost.wood / 2;
        Ok(city)
    }
}

// ---------------------------------------------------------------------------
// 错误
// ---------------------------------------------------------------------------

/// 建分城错误
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EstablishError {
    /// coord 不是 owner 占领的
    NotOwner,
    /// coord 已有城池
    AlreadyCity,
    /// 周围邻接己方格 < REQUIRED_NEIGHBOR_TILES
    InsufficientTerritory,
    /// 资源不足
    InsufficientResources,
    /// 已达分城上限
    SubcityLimitReached,
}

/// 拆分城错误
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DemolishError {
    /// coord 不是城池
    NotACity,
    /// 主城不可拆
    IsMain,
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_resources(gold: u64, food: u64, wood: u64) -> crate::entity::faction::FactionResources {
        crate::entity::faction::FactionResources {
            gold,
            food,
            wood,
            ..Default::default()
        }
    }

    #[test]
    fn test_register_main_city() {
        let mut mgr = CityManager::new();
        let coord = HexCoord::new(10, 10);
        mgr.register_main_city(coord, "faction_1".to_string());
        let c = mgr.get(coord).unwrap();
        assert!(c.is_main);
        assert_eq!(c.owner, "faction_1");
    }

    #[test]
    fn test_can_establish_subcity_requires_owner() {
        let mgr = CityManager::new();
        let keys = std::collections::BTreeSet::new();
        let result = mgr.can_establish_subcity(
            HexCoord::new(5, 5),
            &"faction_1".to_string(),
            &keys,
        );
        assert_eq!(result, Err(EstablishError::NotOwner));
    }

    #[test]
    fn test_can_establish_subcity_requires_neighbors() {
        let mgr = CityManager::new();
        let coord = HexCoord::new(5, 5);
        // 只有 coord 是 owner, 没邻接
        let mut keys = std::collections::BTreeSet::new();
        keys.insert(coord.to_tile_key());
        let result =
            mgr.can_establish_subcity(coord, &"faction_1".to_string(), &keys);
        assert_eq!(result, Err(EstablishError::InsufficientTerritory));
    }

    #[test]
    fn test_can_establish_subcity_success() {
        let mgr = CityManager::new();
        let coord = HexCoord::new(5, 5);
        // owner 拥有 coord + 全部 6 邻接
        let mut keys = std::collections::BTreeSet::new();
        keys.insert(coord.to_tile_key());
        for n in coord.neighbors() {
            keys.insert(n.to_tile_key());
        }
        let result =
            mgr.can_establish_subcity(coord, &"faction_1".to_string(), &keys);
        assert_eq!(result, Ok(()));
    }

    #[test]
    fn test_can_establish_subcity_limit() {
        let mut mgr = CityManager::new();
        // 注册 2 个分城 (主城不算)
        for i in 0..MAX_SUBCITIES_PER_FACTION {
            let c = HexCoord::new(10 + i as i32, 10);
            mgr.cities
                .insert(c, City::new_subcity(c, "faction_1".to_string()));
        }
        let coord = HexCoord::new(20, 20);
        let mut keys = std::collections::BTreeSet::new();
        keys.insert(coord.to_tile_key());
        for n in coord.neighbors() {
            keys.insert(n.to_tile_key());
        }
        let result =
            mgr.can_establish_subcity(coord, &"faction_1".to_string(), &keys);
        assert_eq!(result, Err(EstablishError::SubcityLimitReached));
    }

    #[test]
    fn test_establish_subcity_deducts_resources() {
        let mut mgr = CityManager::new();
        let coord = HexCoord::new(5, 5);
        let mut keys = std::collections::BTreeSet::new();
        keys.insert(coord.to_tile_key());
        for n in coord.neighbors() {
            keys.insert(n.to_tile_key());
        }
        let mut res = make_resources(1000, 1000, 1000);
        let city = mgr
            .establish_subcity(coord, "faction_1".to_string(), &mut res, &keys)
            .unwrap();
        assert!(!city.is_main, "应是分城");
        assert_eq!(res.gold, 1000 - 500);
        assert_eq!(res.food, 1000 - 200);
        assert_eq!(res.wood, 1000 - 100);
    }

    #[test]
    fn test_establish_subcity_insufficient_resources() {
        let mut mgr = CityManager::new();
        let coord = HexCoord::new(5, 5);
        let mut keys = std::collections::BTreeSet::new();
        keys.insert(coord.to_tile_key());
        for n in coord.neighbors() {
            keys.insert(n.to_tile_key());
        }
        let mut res = make_resources(100, 100, 100); // 不足
        let result = mgr.establish_subcity(coord, "faction_1".to_string(), &mut res, &keys);
        assert_eq!(result, Err(EstablishError::InsufficientResources));
    }

    #[test]
    fn test_demolish_subcity() {
        let mut mgr = CityManager::new();
        let coord = HexCoord::new(5, 5);
        let mut keys = std::collections::BTreeSet::new();
        keys.insert(coord.to_tile_key());
        for n in coord.neighbors() {
            keys.insert(n.to_tile_key());
        }
        let mut res = make_resources(1000, 1000, 1000);
        mgr.establish_subcity(coord, "faction_1".to_string(), &mut res, &keys)
            .unwrap();
        let gold_before = res.gold;
        let removed = mgr.demolish_subcity(coord, &mut res).unwrap();
        assert!(!removed.is_main);
        assert_eq!(res.gold, gold_before + 500 / 2); // 返还 50%
    }

    #[test]
    fn test_demolish_main_city_rejected() {
        let mut mgr = CityManager::new();
        let coord = HexCoord::new(5, 5);
        mgr.register_main_city(coord, "faction_1".to_string());
        let mut res = make_resources(1000, 1000, 1000);
        let result = mgr.demolish_subcity(coord, &mut res);
        assert_eq!(result, Err(DemolishError::IsMain));
    }

    #[test]
    fn test_subcity_count_for() {
        let mut mgr = CityManager::new();
        mgr.register_main_city(HexCoord::new(0, 0), "faction_1".to_string());
        mgr.cities.insert(
            HexCoord::new(10, 10),
            City::new_subcity(HexCoord::new(10, 10), "faction_1".to_string()),
        );
        mgr.cities.insert(
            HexCoord::new(20, 20),
            City::new_subcity(HexCoord::new(20, 20), "faction_1".to_string()),
        );
        // 主城不算分城
        assert_eq!(mgr.subcity_count_for(&"faction_1".to_string()), 2);
        // 其它 faction = 0
        assert_eq!(mgr.subcity_count_for(&"faction_2".to_string()), 0);
    }
}
