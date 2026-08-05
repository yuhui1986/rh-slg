//! 经济与资源系统
//!
//! 包含资源产出/消耗、建造队列推进、征兵逻辑。
//! 纯函数设计，不依赖任何引擎类型。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use slg_data::config::*;
use slg_data::ids::*;
use tracing::warn;

use crate::entity::city::*;
use crate::entity::faction::*;
use crate::map::tile::ResourceType;

// ---------------------------------------------------------------------------
// ResourceCost
// ---------------------------------------------------------------------------

/// 资源消耗
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceCost {
    pub gold: u64,
    pub food: u64,
    pub wood: u64,
    pub iron: u64,
    pub stone: u64,
}

impl ResourceCost {
    /// 零消耗
    pub fn zero() -> Self {
        Self {
            gold: 0,
            food: 0,
            wood: 0,
            iron: 0,
            stone: 0,
        }
    }

    /// 按倍率缩放（向上取整）
    pub fn scaled(&self, multiplier: f64) -> Self {
        let multiplier = multiplier.max(0.0);
        Self {
            gold: (self.gold as f64 * multiplier).ceil() as u64,
            food: (self.food as f64 * multiplier).ceil() as u64,
            wood: (self.wood as f64 * multiplier).ceil() as u64,
            iron: (self.iron as f64 * multiplier).ceil() as u64,
            stone: (self.stone as f64 * multiplier).ceil() as u64,
        }
    }
}

// ---------------------------------------------------------------------------
// 资源检查与扣除
// ---------------------------------------------------------------------------

/// 检查势力是否能承担消耗
pub fn can_afford(resources: &FactionResources, cost: &ResourceCost) -> bool {
    resources.gold >= cost.gold
        && resources.food >= cost.food
        && resources.wood >= cost.wood
        && resources.iron >= cost.iron
        && resources.stone >= cost.stone
}

/// 扣除资源，返回是否成功
///
/// 资源不足时不扣除，返回 false。
pub fn spend_resources(resources: &mut FactionResources, cost: &ResourceCost) -> bool {
    if !can_afford(resources, cost) {
        return false;
    }
    resources.gold -= cost.gold;
    resources.food -= cost.food;
    resources.wood -= cost.wood;
    resources.iron -= cost.iron;
    resources.stone -= cost.stone;
    true
}

// ---------------------------------------------------------------------------
// 资源产出
// ---------------------------------------------------------------------------

/// 每 tick 资源产出
///
/// 根据领地等级计算产出，应用全局参数倍率。
/// 产出公式：base × level × global_params.economy.resource_multiplier
/// 资源点加成：有资源点的格子额外产出对应资源。
pub fn tick_resources(
    faction: &mut FactionState,
    tile_owners: &BTreeMap<TileKey, FactionId>,
    tile_levels: &BTreeMap<TileKey, u8>,
    tile_resources: &BTreeMap<TileKey, ResourceType>,
    faction_id: &FactionId,
    global_params: &GlobalParams,
) {
    let mut total_gold = 0u64;
    let mut total_food = 0u64;
    let mut total_wood = 0u64;
    let mut total_iron = 0u64;
    let mut total_stone = 0u64;

    for (key, owner) in tile_owners {
        if owner != faction_id {
            continue;
        }

        let level = tile_levels.get(key).copied().unwrap_or(1) as u64;

        // 基础产出 × 等级倍率
        let base = level * 10;
        total_gold += base;
        total_food += base;

        // 资源点加成
        if let Some(res) = tile_resources.get(key) {
            match res {
                ResourceType::Gold => total_gold += level * 5,
                ResourceType::Food => total_food += level * 5,
                ResourceType::Wood => total_wood += level * 5,
                ResourceType::Iron => total_iron += level * 5,
                ResourceType::Stone => total_stone += level * 5,
            }
        }
    }

    // 应用全局倍率
    let multiplier = global_params.economy.resource_multiplier;
    faction.resources.gold += (total_gold as f64 * multiplier) as u64;
    faction.resources.food += (total_food as f64 * multiplier) as u64;
    faction.resources.wood += (total_wood as f64 * multiplier) as u64;
    faction.resources.iron += (total_iron as f64 * multiplier) as u64;
    faction.resources.stone += (total_stone as f64 * multiplier) as u64;
}

// ---------------------------------------------------------------------------
// 军队维护
// ---------------------------------------------------------------------------

/// 每 tick 军队维护消耗
///
/// 每 100 兵消耗 1 粮食/tick，使用 saturating_sub 避免下溢。
pub fn tick_maintenance(faction: &mut FactionState, total_troops: u32) {
    let food_cost = (total_troops as u64) / 100;
    faction.resources.food = faction.resources.food.saturating_sub(food_cost);
}

// ---------------------------------------------------------------------------
// 建筑队列
// ---------------------------------------------------------------------------

/// 推进建造队列
///
/// 每 tick 检查是否完成，完成时从队列移除。
/// 返回本次完成的建筑条目列表，调用方可据此应用效果。
pub fn tick_build_queue(build_queue: &mut CityBuildQueue, current_tick: u64) -> Vec<BuildEntry> {
    let mut completed = Vec::new();
    let mut completed_indices = Vec::new();

    for (i, entry) in build_queue.queue.iter().enumerate() {
        if current_tick >= entry.end_tick {
            completed_indices.push(i);
        }
    }

    // 从后往前移除已完成的（避免索引偏移）
    for &i in completed_indices.iter().rev() {
        let entry = build_queue.queue.remove(i);
        completed.push(entry);
    }

    completed
}

/// 检查是否可以开始建造
///
/// 建造条件：队列未满（最多 3 条）。
pub fn can_start_build(build_queue: &CityBuildQueue) -> bool {
    build_queue.queue.len() < 3
}

/// 添加建造条目到队列
///
/// 根据建筑定义和目标等级计算建造时间，创建 BuildEntry。
/// `target_level` 从 1 开始。
pub fn enqueue_build(
    build_queue: &mut CityBuildQueue,
    building_id: BuildingId,
    target_level: u8,
    building_defs: &BTreeMap<BuildingId, BuildingDef>,
    start_tick: u64,
) -> bool {
    if build_queue.queue.len() >= 3 {
        return false;
    }

    if target_level == 0 {
        return false;
    }

    let def = match building_defs.get(&building_id) {
        Some(d) => d,
        None => return false,
    };

    let level_idx = (target_level - 1) as usize;
    let level_data = match def.levels.get(level_idx) {
        Some(ld) => ld,
        None => return false,
    };

    let entry = BuildEntry {
        building_id,
        start_tick,
        end_tick: start_tick + level_data.build_time_ticks as u64,
    };

    build_queue.queue.push(entry);
    true
}

/// 解析建筑效果字符串并应用到势力资源
///
/// 格式："food_production:10" / "gold_production:15"
/// 目前仅处理资源产出类效果，其他效果（如 recruit_speed、city_defense）由调用方处理。
pub fn apply_building_effect(faction: &mut FactionState, effect: &str) {
    if let Some((key, value)) = parse_building_effect(effect) {
        let amount = value as u64;
        match key.as_str() {
            "food_production" => faction.resources.food += amount,
            "gold_production" => faction.resources.gold += amount,
            "wood_production" => faction.resources.wood += amount,
            "iron_production" => faction.resources.iron += amount,
            "stone_production" => faction.resources.stone += amount,
            _ => warn!(key = %key, "unrecognized building effect key"),
        }
    }
}

// ---------------------------------------------------------------------------
// 征兵
// ---------------------------------------------------------------------------

/// 征兵
///
/// 消耗资源，增加守军。受 recruit_cost_multiplier 影响。
/// 资源不足时返回 false，不征兵。
pub fn recruit_troops(
    faction: &mut FactionState,
    garrison: &mut CityGarrison,
    unit_type: &UnitTypeId,
    count: u32,
    unit_defs: &BTreeMap<UnitTypeId, UnitTypeDef>,
    global_params: &GlobalParams,
) -> bool {
    let unit_def = match unit_defs.get(unit_type) {
        Some(def) => def,
        None => return false,
    };

    let cost_per_unit =
        (unit_def.recruit_cost as f64 * global_params.economy.recruit_cost_multiplier) as u64;
    let total_cost = cost_per_unit * count as u64;

    let cost = ResourceCost {
        gold: total_cost,
        food: 0,
        wood: 0,
        iron: 0,
        stone: 0,
    };

    if !spend_resources(&mut faction.resources, &cost) {
        return false;
    }

    // 增加守军
    garrison.troops.push((unit_type.clone(), count));
    true
}

// ---------------------------------------------------------------------------
// 建筑效果解析
// ---------------------------------------------------------------------------

/// 解析建筑效果字符串
///
/// 格式："food_production:10" / "recruit_speed:1.5" / "resource_cap:1000"
pub fn parse_building_effect(effect: &str) -> Option<(String, f64)> {
    let parts: Vec<&str> = effect.split(':').collect();
    if parts.len() == 2 {
        if let Ok(value) = parts[1].parse::<f64>() {
            return Some((parts[0].to_string(), value));
        }
    }
    None
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::grid::HexCoord;

    fn create_faction() -> FactionState {
        FactionState {
            resources: FactionResources {
                gold: 1000,
                food: 500,
                wood: 300,
                iron: 200,
                stone: 100,
                troops: 0,
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

    fn create_global_params() -> GlobalParams {
        GlobalParams {
            economy: EconomyParams {
                resource_multiplier: 1.0,
                build_cost_multiplier: 1.0,
                recruit_cost_multiplier: 1.0,
            },
            military: MilitaryParams {
                combat_damage_multiplier: 1.0,
                march_speed_multiplier: 1.0,
                exp_gain_multiplier: 1.0,
            },
            map: MapParams {
                tile_level_range: (1, 9),
                resource_density: 0.3,
            },
            diplomacy: DiplomacyParams {
                relation_decay_per_tick: 0.1,
                alliance_threshold: 50,
            },
        }
    }

    fn create_unit_defs() -> BTreeMap<UnitTypeId, UnitTypeDef> {
        let mut defs = BTreeMap::new();
        defs.insert(
            "unit_infantry".to_string(),
            UnitTypeDef {
                id: "unit_infantry".to_string(),
                name: "步兵".to_string(),
                attack: 10,
                defense: 8,
                hp: 100,
                speed: 3,
                recruit_cost: 50,
                counter_target: "unit_cavalry".to_string(),
                terrain_adaptation: vec![],
            },
        );
        defs
    }

    fn create_building_defs() -> BTreeMap<BuildingId, BuildingDef> {
        let mut defs = BTreeMap::new();
        defs.insert(
            "building_farm".to_string(),
            BuildingDef {
                id: "building_farm".to_string(),
                name: "农田".to_string(),
                category: "economy".to_string(),
                levels: vec![
                    BuildingLevel {
                        cost_resources: 500,
                        build_time_ticks: 100,
                        effect: "food_production:10".to_string(),
                    },
                    BuildingLevel {
                        cost_resources: 1200,
                        build_time_ticks: 250,
                        effect: "food_production:25".to_string(),
                    },
                    BuildingLevel {
                        cost_resources: 3000,
                        build_time_ticks: 500,
                        effect: "food_production:50".to_string(),
                    },
                ],
                terrain_req: vec!["terrain_plains".to_string()],
            },
        );
        defs
    }

    // -----------------------------------------------------------------------
    // ResourceCost
    // -----------------------------------------------------------------------

    #[test]
    fn test_resource_cost_zero() {
        let cost = ResourceCost::zero();
        assert_eq!(cost.gold, 0);
        assert_eq!(cost.food, 0);
        assert_eq!(cost.wood, 0);
        assert_eq!(cost.iron, 0);
        assert_eq!(cost.stone, 0);
    }

    #[test]
    fn test_resource_cost_scaled() {
        let cost = ResourceCost {
            gold: 100,
            food: 200,
            wood: 300,
            iron: 400,
            stone: 500,
        };
        let scaled = cost.scaled(1.5);
        assert_eq!(scaled.gold, 150);
        assert_eq!(scaled.food, 300);
        assert_eq!(scaled.wood, 450);
        assert_eq!(scaled.iron, 600);
        assert_eq!(scaled.stone, 750);
    }

    // -----------------------------------------------------------------------
    // can_afford / spend_resources
    // -----------------------------------------------------------------------

    #[test]
    fn test_can_afford() {
        let faction = create_faction();
        let cost = ResourceCost {
            gold: 500,
            food: 200,
            wood: 100,
            iron: 50,
            stone: 25,
        };
        assert!(can_afford(&faction.resources, &cost));
    }

    #[test]
    fn test_cannot_afford() {
        let faction = create_faction();
        let cost = ResourceCost {
            gold: 2000,
            food: 0,
            wood: 0,
            iron: 0,
            stone: 0,
        };
        assert!(!can_afford(&faction.resources, &cost));
    }

    #[test]
    fn test_spend_resources() {
        let mut faction = create_faction();
        let cost = ResourceCost {
            gold: 500,
            food: 200,
            wood: 100,
            iron: 50,
            stone: 25,
        };
        assert!(spend_resources(&mut faction.resources, &cost));
        assert_eq!(faction.resources.gold, 500);
        assert_eq!(faction.resources.food, 300);
        assert_eq!(faction.resources.wood, 200);
        assert_eq!(faction.resources.iron, 150);
        assert_eq!(faction.resources.stone, 75);
    }

    #[test]
    fn test_spend_insufficient() {
        let mut faction = create_faction();
        let cost = ResourceCost {
            gold: 2000,
            food: 0,
            wood: 0,
            iron: 0,
            stone: 0,
        };
        assert!(!spend_resources(&mut faction.resources, &cost));
        // 资源不应变化
        assert_eq!(faction.resources.gold, 1000);
        assert_eq!(faction.resources.food, 500);
    }

    // -----------------------------------------------------------------------
    // tick_resources
    // -----------------------------------------------------------------------

    #[test]
    fn test_tick_resources_basic() {
        let mut faction = create_faction();
        let mut tile_owners = BTreeMap::new();
        let mut tile_levels = BTreeMap::new();
        let tile_resources = BTreeMap::new();
        let faction_id = "faction_1".to_string();
        let global_params = create_global_params();

        // 占领 1 格 lv5 平原
        let key = tile_key(5, 5);
        tile_owners.insert(key, faction_id.clone());
        tile_levels.insert(key, 5);

        let initial_gold = faction.resources.gold;
        let initial_food = faction.resources.food;
        tick_resources(
            &mut faction,
            &tile_owners,
            &tile_levels,
            &tile_resources,
            &faction_id,
            &global_params,
        );

        // lv5: base = 5 * 10 = 50
        assert_eq!(faction.resources.gold, initial_gold + 50);
        assert_eq!(faction.resources.food, initial_food + 50);
    }

    #[test]
    fn test_tick_resources_empty_territory() {
        let mut faction = create_faction();
        let tile_owners = BTreeMap::new();
        let tile_levels = BTreeMap::new();
        let tile_resources = BTreeMap::new();
        let faction_id = "faction_1".to_string();
        let global_params = create_global_params();

        let initial_gold = faction.resources.gold;
        tick_resources(
            &mut faction,
            &tile_owners,
            &tile_levels,
            &tile_resources,
            &faction_id,
            &global_params,
        );

        // 空领地无产出
        assert_eq!(faction.resources.gold, initial_gold);
    }

    #[test]
    fn test_tick_resources_higher_level_more_output() {
        let mut faction_low = create_faction();
        let mut faction_high = create_faction();
        let mut tile_owners = BTreeMap::new();
        let mut tile_levels = BTreeMap::new();
        let tile_resources = BTreeMap::new();
        let global_params = create_global_params();

        let key_low = tile_key(1, 1);
        let key_high = tile_key(2, 2);
        let f1 = "faction_1".to_string();
        let f2 = "faction_2".to_string();

        tile_owners.insert(key_low, f1.clone());
        tile_levels.insert(key_low, 1);
        tile_owners.insert(key_high, f2.clone());
        tile_levels.insert(key_high, 9);

        tick_resources(
            &mut faction_low,
            &tile_owners,
            &tile_levels,
            &tile_resources,
            &f1,
            &global_params,
        );
        tick_resources(
            &mut faction_high,
            &tile_owners,
            &tile_levels,
            &tile_resources,
            &f2,
            &global_params,
        );

        // lv9 产出 > lv1 产出
        let gain_low = faction_low.resources.gold - 1000;
        let gain_high = faction_high.resources.gold - 1000;
        assert!(gain_high > gain_low);
    }

    #[test]
    fn test_tick_resources_with_resource_bonus() {
        let mut faction = create_faction();
        let mut tile_owners = BTreeMap::new();
        let mut tile_levels = BTreeMap::new();
        let mut tile_resources = BTreeMap::new();
        let faction_id = "faction_1".to_string();
        let global_params = create_global_params();

        let key = tile_key(5, 5);
        tile_owners.insert(key, faction_id.clone());
        tile_levels.insert(key, 3);
        tile_resources.insert(key, ResourceType::Iron);

        let initial_iron = faction.resources.iron;
        tick_resources(
            &mut faction,
            &tile_owners,
            &tile_levels,
            &tile_resources,
            &faction_id,
            &global_params,
        );

        // lv3 铁矿点：base iron = 0, bonus = 3 * 5 = 15
        assert_eq!(faction.resources.iron, initial_iron + 15);
    }

    #[test]
    fn test_tick_resources_with_multiplier() {
        let mut faction = create_faction();
        let mut tile_owners = BTreeMap::new();
        let mut tile_levels = BTreeMap::new();
        let tile_resources = BTreeMap::new();
        let faction_id = "faction_1".to_string();
        let mut global_params = create_global_params();
        global_params.economy.resource_multiplier = 2.0;

        let key = tile_key(5, 5);
        tile_owners.insert(key, faction_id.clone());
        tile_levels.insert(key, 5);

        let initial_gold = faction.resources.gold;
        tick_resources(
            &mut faction,
            &tile_owners,
            &tile_levels,
            &tile_resources,
            &faction_id,
            &global_params,
        );

        // lv5 base = 50, multiplier = 2.0 → 100
        assert_eq!(faction.resources.gold, initial_gold + 100);
    }

    // -----------------------------------------------------------------------
    // tick_maintenance
    // -----------------------------------------------------------------------

    #[test]
    fn test_tick_maintenance() {
        let mut faction = create_faction();
        faction.resources.food = 100;

        tick_maintenance(&mut faction, 500);
        // 500 兵 / 100 = 5 粮食/tick
        assert_eq!(faction.resources.food, 95);
    }

    #[test]
    fn test_tick_maintenance_zero_troops() {
        let mut faction = create_faction();
        faction.resources.food = 100;

        tick_maintenance(&mut faction, 0);
        assert_eq!(faction.resources.food, 100);
    }

    #[test]
    fn test_tick_maintenance_saturating() {
        let mut faction = create_faction();
        faction.resources.food = 3;

        tick_maintenance(&mut faction, 500);
        // 500 兵 = 5 粮食，但只有 3 粮食 → saturating_sub → 0
        assert_eq!(faction.resources.food, 0);
    }

    // -----------------------------------------------------------------------
    // tick_build_queue
    // -----------------------------------------------------------------------

    #[test]
    fn test_tick_build_queue_no_completion() {
        let mut queue = CityBuildQueue {
            queue: vec![BuildEntry {
                building_id: "building_farm".to_string(),
                start_tick: 0,
                end_tick: 100,
            }],
        };

        let completed = tick_build_queue(&mut queue, 50);
        assert!(completed.is_empty());
        assert_eq!(queue.queue.len(), 1);
    }

    #[test]
    fn test_tick_build_queue_completion() {
        let mut queue = CityBuildQueue {
            queue: vec![BuildEntry {
                building_id: "building_farm".to_string(),
                start_tick: 0,
                end_tick: 100,
            }],
        };

        let completed = tick_build_queue(&mut queue, 100);
        assert_eq!(completed.len(), 1);
        assert_eq!(completed[0].building_id, "building_farm");
        assert!(queue.queue.is_empty());
    }

    #[test]
    fn test_tick_build_queue_multiple() {
        let mut queue = CityBuildQueue {
            queue: vec![
                BuildEntry {
                    building_id: "building_farm".to_string(),
                    start_tick: 0,
                    end_tick: 50,
                },
                BuildEntry {
                    building_id: "building_barracks".to_string(),
                    start_tick: 10,
                    end_tick: 100,
                },
                BuildEntry {
                    building_id: "building_wall".to_string(),
                    start_tick: 20,
                    end_tick: 200,
                },
            ],
        };

        // tick=100: farm (end=50) 和 barracks (end=100) 完成，wall 未完成
        let completed = tick_build_queue(&mut queue, 100);
        assert_eq!(completed.len(), 2);
        assert_eq!(queue.queue.len(), 1);
        assert_eq!(queue.queue[0].building_id, "building_wall");
    }

    #[test]
    fn test_enqueue_build() {
        let mut queue = CityBuildQueue { queue: vec![] };
        let defs = create_building_defs();

        assert!(enqueue_build(
            &mut queue,
            "building_farm".to_string(),
            1,
            &defs,
            0,
        ));
        assert_eq!(queue.queue.len(), 1);
        assert_eq!(queue.queue[0].end_tick, 100); // build_time_ticks = 100
    }

    #[test]
    fn test_enqueue_build_queue_full() {
        let mut queue = CityBuildQueue {
            queue: vec![
                BuildEntry {
                    building_id: "a".to_string(),
                    start_tick: 0,
                    end_tick: 10,
                },
                BuildEntry {
                    building_id: "b".to_string(),
                    start_tick: 0,
                    end_tick: 10,
                },
                BuildEntry {
                    building_id: "c".to_string(),
                    start_tick: 0,
                    end_tick: 10,
                },
            ],
        };
        let defs = create_building_defs();

        assert!(!enqueue_build(
            &mut queue,
            "building_farm".to_string(),
            1,
            &defs,
            0,
        ));
    }

    #[test]
    fn test_apply_building_effect() {
        let mut faction = create_faction();
        let initial_food = faction.resources.food;

        apply_building_effect(&mut faction, "food_production:10");
        assert_eq!(faction.resources.food, initial_food + 10);

        let initial_gold = faction.resources.gold;
        apply_building_effect(&mut faction, "gold_production:15");
        assert_eq!(faction.resources.gold, initial_gold + 15);
    }

    // -----------------------------------------------------------------------
    // recruit_troops
    // -----------------------------------------------------------------------

    #[test]
    fn test_recruit_troops_success() {
        let mut faction = create_faction();
        let mut garrison = CityGarrison { troops: vec![] };
        let unit_defs = create_unit_defs();
        let global_params = create_global_params();

        let unit_type = "unit_infantry".to_string();
        assert!(recruit_troops(
            &mut faction,
            &mut garrison,
            &unit_type,
            10,
            &unit_defs,
            &global_params,
        ));

        // 10 兵 × 50 金 = 500 金
        assert_eq!(faction.resources.gold, 500);
        assert_eq!(garrison.troops.len(), 1);
        assert_eq!(garrison.troops[0], (unit_type, 10));
    }

    #[test]
    fn test_recruit_troops_insufficient() {
        let mut faction = create_faction();
        faction.resources.gold = 100; // 不够征兵
        let mut garrison = CityGarrison { troops: vec![] };
        let unit_defs = create_unit_defs();
        let global_params = create_global_params();

        let unit_type = "unit_infantry".to_string();
        assert!(!recruit_troops(
            &mut faction,
            &mut garrison,
            &unit_type,
            10,
            &unit_defs,
            &global_params,
        ));

        // 资源不应变化
        assert_eq!(faction.resources.gold, 100);
        assert!(garrison.troops.is_empty());
    }

    #[test]
    fn test_recruit_troops_unknown_unit() {
        let mut faction = create_faction();
        let mut garrison = CityGarrison { troops: vec![] };
        let unit_defs = create_unit_defs();
        let global_params = create_global_params();

        let unit_type = "unit_unknown".to_string();
        assert!(!recruit_troops(
            &mut faction,
            &mut garrison,
            &unit_type,
            10,
            &unit_defs,
            &global_params,
        ));
    }

    #[test]
    fn test_recruit_troops_with_multiplier() {
        let mut faction = create_faction();
        let mut garrison = CityGarrison { troops: vec![] };
        let unit_defs = create_unit_defs();
        let mut global_params = create_global_params();
        global_params.economy.recruit_cost_multiplier = 2.0;

        let unit_type = "unit_infantry".to_string();
        assert!(recruit_troops(
            &mut faction,
            &mut garrison,
            &unit_type,
            10,
            &unit_defs,
            &global_params,
        ));

        // 10 兵 × 50 金 × 2.0 = 1000 金
        assert_eq!(faction.resources.gold, 0);
    }

    // -----------------------------------------------------------------------
    // parse_building_effect
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_building_effect() {
        let (key, value) = parse_building_effect("food_production:10").unwrap();
        assert_eq!(key, "food_production");
        assert_eq!(value, 10.0);
    }

    #[test]
    fn test_parse_building_effect_float() {
        let (key, value) = parse_building_effect("recruit_speed:1.5").unwrap();
        assert_eq!(key, "recruit_speed");
        assert_eq!(value, 1.5);
    }

    #[test]
    fn test_parse_building_effect_invalid() {
        assert!(parse_building_effect("invalid").is_none());
        assert!(parse_building_effect("key:not_a_number").is_none());
    }
}
