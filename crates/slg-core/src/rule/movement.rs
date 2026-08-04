//! 行军推进系统
//!
//! 请求行军：A* 计算路径 → 写入 LRU 缓存 → 预计算 arrive_tick。
//! 每 tick 推进：按速度前进 path_index，到达时产生事件。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use slg_data::config::TerrainTypeDef;
use slg_data::ids::*;

use crate::entity::army::{ArmyMarch, ArmyPosition};
use crate::map::grid::HexCoord;
use crate::map::pathfinding::{find_path, PathResult};
use crate::map::tile::TerrainType;
use crate::resource::PathCache;

// ---------------------------------------------------------------------------
// 行军请求（数据结构，已存在；保留原字段）
// ---------------------------------------------------------------------------

/// 行军请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarchRequest {
    pub army_entity_id: u64,
    pub destination: HexCoord,
    pub priority: u8,
}

// ---------------------------------------------------------------------------
// 行军到达事件
// ---------------------------------------------------------------------------

/// 行军到达事件：部队抵达目的地时产生
#[derive(Debug, Clone)]
pub struct MarchArrived {
    pub army_entity_id: u64,
    pub destination: HexCoord,
}

// ---------------------------------------------------------------------------
// 请求行军
// ---------------------------------------------------------------------------

/// 请求行军
///
/// 1. 查询 LRU 缓存（key = (起点, 终点, 通行掩码)）
/// 2. 缓存未命中时调用 A* 寻路
/// 3. 写入缓存
/// 4. 计算 arrive_tick = current_tick + ceil(path_len / speed)
///
/// 返回 `true` 表示行军已设置，`false` 表示起点即终点或无路径。
#[allow(clippy::too_many_arguments)]
pub fn request_march(
    army: &mut ArmyMarch,
    army_pos: &ArmyPosition,
    destination: HexCoord,
    current_tick: u64,
    speed: u16,
    path_cache: &mut PathCache,
    terrain: &BTreeMap<TileKey, TerrainType>,
    terrain_defs: &BTreeMap<TerrainTypeId, TerrainTypeDef>,
    tile_owners: &BTreeMap<TileKey, FactionId>,
    faction: &FactionId,
) -> bool {
    if army_pos.coord == destination {
        return false;
    }

    let start_key = army_pos.coord.to_tile_key();
    let goal_key = destination.to_tile_key();
    // 通行掩码：0 = 默认（全部地形类型可通行检查由 terrain_defs 控制）
    let cache_key = (start_key, goal_key, 0u32);

    // 尝试缓存命中
    let path = if let Some(cached) = path_cache.entries.get(&cache_key) {
        cached.clone()
    } else {
        let result: PathResult = find_path(
            army_pos.coord,
            destination,
            terrain,
            terrain_defs,
            tile_owners,
            faction,
            1000,
        );

        if !result.found {
            return false;
        }

        // 写入 LRU 缓存
        path_cache.entries.put(cache_key, result.path.clone());
        result.path
    };

    if path.is_empty() {
        return false;
    }

    // 设置行军状态
    let speed = speed.max(1) as u64;
    let steps = path.len() as u64;
    army.arrive_tick = current_tick + steps.div_ceil(speed);
    army.path = path;
    army.path_index = 0;

    true
}

// ---------------------------------------------------------------------------
// 每 tick 推进行军
// ---------------------------------------------------------------------------

/// 每 tick 推进行军
///
/// 遍历所有行军中的部队，按 `speed` 推进 `path_index`，
/// 更新 `ArmyPosition`，到达终点时产生 `MarchArrived` 事件。
///
/// 返回本 tick 到达终点的部队列表。
pub fn tick_march(
    armies: &mut [(u64, ArmyMarch, ArmyPosition)],
    _current_tick: u64,
    speed: u16,
) -> Vec<MarchArrived> {
    let mut arrived = Vec::new();
    let steps_per_tick = speed.max(1) as usize;

    for (entity_id, march, pos) in armies.iter_mut() {
        // 无路径或已到达
        if march.path.is_empty() || march.path_index >= march.path.len() {
            continue;
        }

        // 按速度推进
        march.path_index = (march.path_index + steps_per_tick).min(march.path.len());

        // 更新当前位置为最后到达的节点
        if march.path_index > 0 {
            pos.coord = march.path[march.path_index - 1];
        }

        // 检查是否到达终点
        if march.path_index >= march.path.len() {
            arrived.push(MarchArrived {
                army_entity_id: *entity_id,
                destination: pos.coord,
            });
        }
    }

    arrived
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    use lru::LruCache;
    use std::num::NonZeroUsize;

    fn make_cache() -> PathCache {
        PathCache {
            entries: LruCache::new(NonZeroUsize::new(4096).unwrap()),
        }
    }

    fn create_terrain_map() -> BTreeMap<TileKey, TerrainType> {
        let mut terrain = BTreeMap::new();
        for r in 0..20i32 {
            for q in 0..20i32 {
                terrain.insert(HexCoord::new(q, r).to_tile_key(), TerrainType::Plains);
            }
        }
        terrain
    }

    #[test]
    fn test_request_march_sets_path() {
        let terrain = create_terrain_map();
        let terrain_defs = BTreeMap::new();
        let owners = BTreeMap::new();
        let faction = "faction_1".to_string();
        let mut cache = make_cache();

        let mut march = ArmyMarch {
            path: Vec::new(),
            path_index: 0,
            arrive_tick: 0,
        };
        let pos = ArmyPosition {
            coord: HexCoord::new(0, 0),
        };

        let ok = request_march(
            &mut march,
            &pos,
            HexCoord::new(5, 0),
            0,
            1,
            &mut cache,
            &terrain,
            &terrain_defs,
            &owners,
            &faction,
        );

        assert!(ok, "request_march should succeed");
        assert!(!march.path.is_empty(), "path should be populated");
        assert_eq!(march.path.first(), Some(&HexCoord::new(0, 0)));
        assert_eq!(march.path.last(), Some(&HexCoord::new(5, 0)));
        assert!(march.arrive_tick > 0, "arrive_tick should be set");
    }

    #[test]
    fn test_request_march_same_position() {
        let terrain = create_terrain_map();
        let terrain_defs = BTreeMap::new();
        let owners = BTreeMap::new();
        let faction = "faction_1".to_string();
        let mut cache = make_cache();

        let mut march = ArmyMarch {
            path: Vec::new(),
            path_index: 0,
            arrive_tick: 0,
        };
        let pos = ArmyPosition {
            coord: HexCoord::new(3, 3),
        };

        let ok = request_march(
            &mut march,
            &pos,
            HexCoord::new(3, 3),
            0,
            1,
            &mut cache,
            &terrain,
            &terrain_defs,
            &owners,
            &faction,
        );

        assert!(!ok, "request_march to same position should return false");
    }

    #[test]
    fn test_tick_march_advances_position() {
        let mut march = ArmyMarch {
            path: vec![
                HexCoord::new(0, 0),
                HexCoord::new(1, 0),
                HexCoord::new(2, 0),
                HexCoord::new(3, 0),
            ],
            path_index: 0,
            arrive_tick: 4,
        };
        let mut pos = ArmyPosition {
            coord: HexCoord::new(0, 0),
        };

        // Tick 1: speed=1 → path_index 1
        let arrived = tick_march(&mut [(99u64, march.clone(), pos.clone())], 1, 1);
        assert!(arrived.is_empty());
        // 模拟推进
        march.path_index = 1;
        pos.coord = march.path[0];
        assert_eq!(pos.coord, HexCoord::new(0, 0));

        // 直接用 tick_march 验证完整推进
        let mut armies: Vec<(u64, ArmyMarch, ArmyPosition)> = vec![(
            42,
            ArmyMarch {
                path: vec![
                    HexCoord::new(0, 0),
                    HexCoord::new(1, 0),
                    HexCoord::new(2, 0),
                    HexCoord::new(3, 0),
                ],
                path_index: 0,
                arrive_tick: 2,
            },
            ArmyPosition {
                coord: HexCoord::new(0, 0),
            },
        )];

        // speed=2 → 每 tick 走 2 步
        let arrived = tick_march(&mut armies, 1, 2);
        assert!(
            arrived.is_empty(),
            "should not arrive after 1 tick with speed=2 on 4-step path"
        );
        assert_eq!(armies[0].1.path_index, 2);
        assert_eq!(armies[0].2.coord, HexCoord::new(1, 0));

        // Tick 2: 再走 2 步 → 到达
        let arrived = tick_march(&mut armies, 2, 2);
        assert_eq!(arrived.len(), 1);
        assert_eq!(arrived[0].army_entity_id, 42);
        assert_eq!(arrived[0].destination, HexCoord::new(3, 0));
        assert_eq!(armies[0].1.path_index, 4);
    }

    #[test]
    fn test_tick_march_no_march() {
        let mut armies: Vec<(u64, ArmyMarch, ArmyPosition)> = vec![(
            1,
            ArmyMarch {
                path: Vec::new(),
                path_index: 0,
                arrive_tick: 0,
            },
            ArmyPosition {
                coord: HexCoord::new(0, 0),
            },
        )];

        let arrived = tick_march(&mut armies, 1, 1);
        assert!(arrived.is_empty());
    }

    #[test]
    fn test_path_cache_hit() {
        let terrain = create_terrain_map();
        let terrain_defs = BTreeMap::new();
        let owners = BTreeMap::new();
        let faction = "faction_1".to_string();
        let mut cache = make_cache();

        let pos = ArmyPosition {
            coord: HexCoord::new(0, 0),
        };

        // 第一次请求
        let mut march1 = ArmyMarch {
            path: Vec::new(),
            path_index: 0,
            arrive_tick: 0,
        };
        let ok1 = request_march(
            &mut march1,
            &pos,
            HexCoord::new(5, 0),
            0,
            1,
            &mut cache,
            &terrain,
            &terrain_defs,
            &owners,
            &faction,
        );
        assert!(ok1);

        // 第二次请求相同起终点 → 应命中缓存
        let mut march2 = ArmyMarch {
            path: Vec::new(),
            path_index: 0,
            arrive_tick: 0,
        };
        let ok2 = request_march(
            &mut march2,
            &pos,
            HexCoord::new(5, 0),
            0,
            1,
            &mut cache,
            &terrain,
            &terrain_defs,
            &owners,
            &faction,
        );
        assert!(ok2);
        assert_eq!(
            march1.path, march2.path,
            "cached path should match original"
        );
    }
}
