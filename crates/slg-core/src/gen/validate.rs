//! 连通性与铺路可达性校验
//!
//! 验证地图是否满足基本可玩性要求：
//! 1. 陆地连通（无孤立飞地）
//! 2. 所有出生点两两可达
//! 3. 陆地占比 > 60%

use crate::gen::spawn::SpawnPoint;
use crate::map::grid::HexCoord;
use crate::map::tile::TerrainType;
use std::collections::{HashSet, VecDeque};

/// 校验结果
#[derive(Debug)]
pub struct ValidationResult {
    pub is_valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

/// 全量校验
pub fn validate_map(
    width: u32,
    height: u32,
    terrain: &[TerrainType],
    spawns: &[SpawnPoint],
) -> ValidationResult {
    let mut result = ValidationResult {
        is_valid: true,
        errors: Vec::new(),
        warnings: Vec::new(),
    };

    // 1. 陆地占比
    let land_count = terrain.iter().filter(|t| **t != TerrainType::Water).count();
    let total = (width * height) as usize;
    let land_ratio = land_count as f64 / total as f64;
    if land_ratio < 0.60 {
        result.warnings.push(format!(
            "陆地占比 {:.1}% < 60%（{} / {}）",
            land_ratio * 100.0,
            land_count,
            total
        ));
    }

    // 2. 陆地连通性
    if !check_land_connectivity(width, height, terrain) {
        result.errors.push("地图存在不连通的陆地飞地".to_string());
        result.is_valid = false;
    }

    // 3. 出生点可达性
    // 优化：从第一个出生点做单次 BFS，检查所有其他出生点是否可达（替代 O(n^2) 次 BFS）
    if spawns.len() >= 2 {
        let visited = bfs_flood(width, height, terrain, spawns[0].coord);
        for s in &spawns[1..] {
            if !visited.contains(&s.coord) {
                result.errors.push(format!(
                    "出生点 {:?}(faction {}) 与 {:?}(faction {}) 不可达",
                    spawns[0].coord, spawns[0].faction_index, s.coord, s.faction_index,
                ));
                result.is_valid = false;
            }
        }
    }

    // 4. 出生点数量
    if spawns.is_empty() {
        result.errors.push("没有生成任何出生点".to_string());
        result.is_valid = false;
    }

    result
}

/// 检查陆地连通性（BFS 洪泛）
///
/// 从第一个陆地格子出发 BFS，检查所有陆地是否都被访问到。
fn check_land_connectivity(width: u32, height: u32, terrain: &[TerrainType]) -> bool {
    // 找到第一个陆地格子
    let start = terrain.iter().position(|t| *t != TerrainType::Water);
    let start_idx = match start {
        Some(idx) => idx,
        None => return true, // 无陆地，视为有效
    };

    let start_x = (start_idx as u32) % width;
    let start_y = (start_idx as u32) / width;
    let start_coord = HexCoord::new(start_x as i32, start_y as i32);

    let visited = bfs_flood(width, height, terrain, start_coord);
    let land_count = terrain.iter().filter(|t| **t != TerrainType::Water).count();

    visited.len() == land_count
}

/// BFS 洪泛：从 start 出发，返回所有可达的陆地坐标集合
fn bfs_flood(
    width: u32,
    height: u32,
    terrain: &[TerrainType],
    start: HexCoord,
) -> HashSet<HexCoord> {
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();

    // start 可能本身是水域（不应发生，但做防御）
    let start_idx = (start.r as u32 * width + start.q as u32) as usize;
    if start_idx >= terrain.len() || terrain[start_idx] == TerrainType::Water {
        return visited;
    }

    visited.insert(start);
    queue.push_back(start);

    while let Some(current) = queue.pop_front() {
        for neighbor in current.neighbors() {
            if visited.contains(&neighbor) {
                continue;
            }
            let nx = neighbor.q;
            let ny = neighbor.r;
            if nx < 0 || ny < 0 || nx as u32 >= width || ny as u32 >= height {
                continue;
            }
            let nidx = (ny as u32 * width + nx as u32) as usize;
            if nidx >= terrain.len() {
                continue;
            }
            if terrain[nidx] != TerrainType::Water {
                visited.insert(neighbor);
                queue.push_back(neighbor);
            }
        }
    }

    visited
}

/// 检查两点间是否可达（BFS）
#[allow(dead_code)]
fn is_reachable(
    width: u32,
    height: u32,
    terrain: &[TerrainType],
    from: HexCoord,
    to: HexCoord,
) -> bool {
    // 快速检查：起终点都必须是陆地
    let from_idx = (from.r as u32 * width + from.q as u32) as usize;
    let to_idx = (to.r as u32 * width + to.q as u32) as usize;
    if from_idx >= terrain.len()
        || to_idx >= terrain.len()
        || terrain[from_idx] == TerrainType::Water
        || terrain[to_idx] == TerrainType::Water
    {
        return false;
    }

    let visited = bfs_flood(width, height, terrain, from);
    visited.contains(&to)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_all_water() {
        let terrain = vec![TerrainType::Water; 64 * 64];
        let result = validate_map(64, 64, &terrain, &[]);
        // 无陆地 => 连通性通过，但出生点为空
        assert!(!result.is_valid); // 没有出生点
    }

    #[test]
    fn test_validate_single_island() {
        let mut terrain = vec![TerrainType::Water; 32 * 32];
        // 中间 10x10 陆地
        for y in 11..21 {
            for x in 11..21 {
                terrain[y * 32 + x] = TerrainType::Plains;
            }
        }
        let spawns = vec![SpawnPoint {
            coord: HexCoord::new(15, 15),
            faction_index: 0,
        }];
        let result = validate_map(32, 32, &terrain, &spawns);
        assert!(result.is_valid, "errors: {:?}", result.errors);
    }

    #[test]
    fn test_validate_disconnected_land() {
        let mut terrain = vec![TerrainType::Water; 32 * 32];
        // 两块不相连的陆地
        for y in 5..10 {
            for x in 5..10 {
                terrain[y * 32 + x] = TerrainType::Plains;
            }
        }
        for y in 22..27 {
            for x in 22..27 {
                terrain[y * 32 + x] = TerrainType::Plains;
            }
        }
        let result = validate_map(32, 32, &terrain, &[]);
        assert!(!result.is_valid);
        assert!(result.errors.iter().any(|e| e.contains("不连通")));
    }

    #[test]
    fn test_validate_unreachable_spawns() {
        let mut terrain = vec![TerrainType::Water; 32 * 32];
        // 两块不相连的陆地，各放一个出生点
        for y in 5..10 {
            for x in 5..10 {
                terrain[y * 32 + x] = TerrainType::Plains;
            }
        }
        for y in 22..27 {
            for x in 22..27 {
                terrain[y * 32 + x] = TerrainType::Plains;
            }
        }
        let spawns = vec![
            SpawnPoint {
                coord: HexCoord::new(7, 7),
                faction_index: 0,
            },
            SpawnPoint {
                coord: HexCoord::new(24, 24),
                faction_index: 1,
            },
        ];
        let result = validate_map(32, 32, &terrain, &spawns);
        assert!(!result.is_valid);
        assert!(result.errors.iter().any(|e| e.contains("不可达")));
    }
}
