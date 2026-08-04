//! Hex A* 寻路系统
//!
//! cube 坐标距离启发式，地形移动代价数据表驱动。
//! 支持 LRU 路径缓存，通行性判断含地形、敌方领地惩罚。

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};

use slg_data::config::TerrainTypeDef;
use slg_data::ids::*;

use crate::map::grid::HexCoord;
use crate::map::tile::TerrainType;

// ---------------------------------------------------------------------------
// 公共结果类型
// ---------------------------------------------------------------------------

/// 路径搜索结果
#[derive(Debug, Clone)]
pub struct PathResult {
    pub path: Vec<HexCoord>,
    pub cost: f64,
    pub found: bool,
}

// ---------------------------------------------------------------------------
// A* 内部节点
// ---------------------------------------------------------------------------

/// A* 搜索节点（BinaryHeap 元素）
#[derive(Debug, Clone)]
struct AStarNode {
    coord: HexCoord,
    /// g + h（启发式估计总代价），用于堆排序
    f_cost: f64,
}

impl PartialEq for AStarNode {
    fn eq(&self, other: &Self) -> bool {
        self.f_cost == other.f_cost
    }
}

impl Eq for AStarNode {}

impl PartialOrd for AStarNode {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for AStarNode {
    /// 反转比较：BinaryHeap 是最大堆，我们需要最小 f_cost 优先
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .f_cost
            .partial_cmp(&self.f_cost)
            .unwrap_or(Ordering::Equal)
    }
}

// ---------------------------------------------------------------------------
// Hex A* 寻路
// ---------------------------------------------------------------------------

/// Hex A* 寻路
///
/// - cube 坐标距离启发式（admissible on hex grids）
/// - 地形移动代价数据表驱动
/// - 敌方领地通过惩罚（2x 代价）
/// - 水域不可通行
/// - `max_iterations` 防止无限搜索
pub fn find_path(
    start: HexCoord,
    goal: HexCoord,
    terrain: &BTreeMap<TileKey, TerrainType>,
    terrain_defs: &BTreeMap<TerrainTypeId, TerrainTypeDef>,
    tile_owners: &BTreeMap<TileKey, FactionId>,
    faction: &FactionId,
    max_iterations: usize,
) -> PathResult {
    if start == goal {
        return PathResult {
            path: vec![start],
            cost: 0.0,
            found: true,
        };
    }

    let start_key = start.to_tile_key();
    let goal_key = goal.to_tile_key();

    let mut open_set = BinaryHeap::new();
    let mut came_from: BTreeMap<TileKey, TileKey> = BTreeMap::new();
    let mut g_score: BTreeMap<TileKey, f64> = BTreeMap::new();
    let mut closed_set: BTreeSet<TileKey> = BTreeSet::new();

    g_score.insert(start_key, 0.0);
    open_set.push(AStarNode {
        coord: start,
        f_cost: heuristic(start, goal),
    });

    let mut iterations = 0usize;

    while let Some(current) = open_set.pop() {
        iterations += 1;
        if iterations > max_iterations {
            return PathResult {
                path: Vec::new(),
                cost: f64::INFINITY,
                found: false,
            };
        }

        let current_key = current.coord.to_tile_key();

        // 到达目标，重建路径
        if current_key == goal_key {
            let path = reconstruct_path(&came_from, start_key, goal_key);
            let cost = g_score[&goal_key];
            return PathResult {
                path,
                cost,
                found: true,
            };
        }

        if closed_set.contains(&current_key) {
            continue;
        }
        closed_set.insert(current_key);

        for neighbor in current.coord.neighbors() {
            let neighbor_key = neighbor.to_tile_key();

            if closed_set.contains(&neighbor_key) {
                continue;
            }

            // 检查通行性：水域不可通行
            let terrain_type = terrain
                .get(&neighbor_key)
                .copied()
                .unwrap_or(TerrainType::Water);
            if !is_passable(terrain_type, terrain_defs) {
                continue;
            }

            // 敌方领地惩罚
            let is_enemy = tile_owners
                .get(&neighbor_key)
                .is_some_and(|owner| owner != faction);

            let base_cost = get_movement_cost(terrain_type, terrain_defs);
            let penalty = if is_enemy { 2.0 } else { 1.0 };
            let move_cost = base_cost * penalty;

            let tentative_g = g_score[&current_key] + move_cost;

            let existing_g = g_score.get(&neighbor_key).copied().unwrap_or(f64::INFINITY);

            if tentative_g < existing_g {
                came_from.insert(neighbor_key, current_key);
                g_score.insert(neighbor_key, tentative_g);
                let f_cost = tentative_g + heuristic(neighbor, goal);
                open_set.push(AStarNode {
                    coord: neighbor,
                    f_cost,
                });
            }
        }
    }

    // 开集耗尽，无路径
    PathResult {
        path: Vec::new(),
        cost: f64::INFINITY,
        found: false,
    }
}

// ---------------------------------------------------------------------------
// 内部辅助函数
// ---------------------------------------------------------------------------

/// 启发式函数：cube 坐标距离（admissible for hex grids）
fn heuristic(a: HexCoord, b: HexCoord) -> f64 {
    a.distance(b) as f64
}

/// 获取地形移动代价
///
/// 优先使用 `terrain_defs` 数据表中的 `movement_cost`；若表中无对应条目则使用内置默认值。
fn get_movement_cost(
    terrain: TerrainType,
    terrain_defs: &BTreeMap<TerrainTypeId, TerrainTypeDef>,
) -> f64 {
    // 尝试从数据表查找
    let key = terrain_type_key(terrain);
    if let Some(def) = terrain_defs.get(key) {
        return def.movement_cost;
    }
    // 内置默认值
    match terrain {
        TerrainType::Plains => 1.0,
        TerrainType::Mountain => 3.0,
        TerrainType::Forest => 1.5,
        TerrainType::Hills => 2.0,
        TerrainType::Pass => 2.0,
        TerrainType::Desert => 1.5,
        TerrainType::Swamp => 2.5,
        TerrainType::Water => f64::INFINITY,
    }
}

/// 判断地形是否可通行
///
/// 水域永远不可通行；其余地形检查 `terrain_defs` 中的 `passable` 字段，
/// 若数据表中无条目则默认可通行。
fn is_passable(
    terrain: TerrainType,
    terrain_defs: &BTreeMap<TerrainTypeId, TerrainTypeDef>,
) -> bool {
    if terrain == TerrainType::Water {
        return false;
    }
    let key = terrain_type_key(terrain);
    if let Some(def) = terrain_defs.get(key) {
        return def.passable;
    }
    true
}

/// 地形类型 → 数据表 key（与 RON 配置表约定一致）
const fn terrain_type_key(t: TerrainType) -> &'static str {
    match t {
        TerrainType::Plains => "terrain_plains",
        TerrainType::Mountain => "terrain_mountain",
        TerrainType::Water => "terrain_water",
        TerrainType::Forest => "terrain_forest",
        TerrainType::Desert => "terrain_desert",
        TerrainType::Swamp => "terrain_swamp",
        TerrainType::Hills => "terrain_hills",
        TerrainType::Pass => "terrain_pass",
    }
}

/// 重建路径：从 came_from 映射中回溯出完整坐标序列
fn reconstruct_path(
    came_from: &BTreeMap<TileKey, TileKey>,
    start_key: TileKey,
    end_key: TileKey,
) -> Vec<HexCoord> {
    let mut path = Vec::new();
    let mut current = end_key;

    path.push(HexCoord::from_tile_key(current));

    while let Some(&prev) = came_from.get(&current) {
        path.push(HexCoord::from_tile_key(prev));
        current = prev;
        if current == start_key {
            break;
        }
    }

    path.reverse();
    path
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// 创建指定尺寸的全平原地形图（q, r 均从 0 开始）
    fn create_terrain_map(width: i32, height: i32) -> BTreeMap<TileKey, TerrainType> {
        create_terrain_rect(0, 0, width, height)
    }

    /// 创建全平原地形图，覆盖 [q_min, q_max) x [r_min, r_max)
    fn create_terrain_rect(
        q_min: i32,
        r_min: i32,
        q_max: i32,
        r_max: i32,
    ) -> BTreeMap<TileKey, TerrainType> {
        let mut terrain = BTreeMap::new();
        for r in r_min..r_max {
            for q in q_min..q_max {
                terrain.insert(HexCoord::new(q, r).to_tile_key(), TerrainType::Plains);
            }
        }
        terrain
    }

    #[test]
    fn test_same_start_goal() {
        let terrain = create_terrain_map(10, 10);
        let terrain_defs = BTreeMap::new();
        let owners = BTreeMap::new();
        let faction = "faction_1".to_string();

        let result = find_path(
            HexCoord::new(5, 5),
            HexCoord::new(5, 5),
            &terrain,
            &terrain_defs,
            &owners,
            &faction,
            1000,
        );

        assert!(result.found);
        assert_eq!(result.path.len(), 1);
        assert_eq!(result.path[0], HexCoord::new(5, 5));
        assert_eq!(result.cost, 0.0);
    }

    #[test]
    fn test_straight_path() {
        let terrain = create_terrain_map(10, 10);
        let terrain_defs = BTreeMap::new();
        let owners = BTreeMap::new();
        let faction = "faction_1".to_string();

        let start = HexCoord::new(0, 0);
        let goal = HexCoord::new(5, 0);

        let result = find_path(
            start,
            goal,
            &terrain,
            &terrain_defs,
            &owners,
            &faction,
            1000,
        );

        assert!(result.found, "should find a straight path on plains");
        assert_eq!(result.path.first(), Some(&start));
        assert_eq!(result.path.last(), Some(&goal));
        // 全平原，代价应等于 hex 距离
        assert!(
            (result.cost - 5.0).abs() < 1e-9,
            "cost should be 5.0, got {}",
            result.cost
        );
    }

    #[test]
    fn test_path_around_water() {
        // 创建足够大的地形图
        let mut terrain = create_terrain_rect(-10, -10, 20, 20);
        // 在 q=3 列放置部分水域墙（仅 r=-2..5），两端留有缺口可绕行
        for r in -2..5 {
            terrain.insert(HexCoord::new(3, r).to_tile_key(), TerrainType::Water);
        }
        let terrain_defs = BTreeMap::new();
        let owners = BTreeMap::new();
        let faction = "faction_1".to_string();

        let result = find_path(
            HexCoord::new(0, 0),
            HexCoord::new(5, 0),
            &terrain,
            &terrain_defs,
            &owners,
            &faction,
            10000,
        );

        assert!(result.found, "should find path around partial water wall");
        // 路径不应包含水域格
        for coord in &result.path {
            let t = terrain
                .get(&coord.to_tile_key())
                .copied()
                .unwrap_or(TerrainType::Water);
            assert_ne!(
                t,
                TerrainType::Water,
                "path should not go through water at {:?}",
                coord
            );
        }
        // 代价应大于直走距离 5（绕路）
        assert!(
            result.cost > 5.0,
            "detour cost should exceed 5.0, got {}",
            result.cost
        );
    }

    #[test]
    fn test_no_path_fully_blocked() {
        let mut terrain = create_terrain_rect(-10, -10, 20, 20);
        // 完全包围目标 (5,0) 的所有 6 邻域
        let blocked = [
            HexCoord::new(5, 0),
            HexCoord::new(6, 0),
            HexCoord::new(5, 1),
            HexCoord::new(4, 1),
            HexCoord::new(4, 0),
            HexCoord::new(5, -1),
            HexCoord::new(6, -1),
        ];
        for coord in &blocked {
            terrain.insert(coord.to_tile_key(), TerrainType::Water);
        }
        let terrain_defs = BTreeMap::new();
        let owners = BTreeMap::new();
        let faction = "faction_1".to_string();

        let result = find_path(
            HexCoord::new(0, 0),
            HexCoord::new(5, 0),
            &terrain,
            &terrain_defs,
            &owners,
            &faction,
            5000,
        );

        assert!(
            !result.found,
            "should not find path to a fully blocked goal"
        );
    }

    #[test]
    fn test_mountain_higher_cost() {
        let mut terrain = create_terrain_map(10, 10);
        // 直线路径上有山地
        terrain.insert(HexCoord::new(2, 0).to_tile_key(), TerrainType::Mountain);

        let terrain_defs = BTreeMap::new();
        let owners = BTreeMap::new();
        let faction = "faction_1".to_string();

        let result = find_path(
            HexCoord::new(0, 0),
            HexCoord::new(4, 0),
            &terrain,
            &terrain_defs,
            &owners,
            &faction,
            1000,
        );

        assert!(result.found, "should find path through or around mountain");
        // 纯平原代价 = 4.0；含山地应更高
        assert!(
            result.cost > 4.0,
            "cost with mountain should exceed 4.0, got {}",
            result.cost
        );
    }

    #[test]
    fn test_enemy_territory_penalty() {
        let terrain = create_terrain_map(10, 10);
        let terrain_defs = BTreeMap::new();
        let mut owners = BTreeMap::new();
        let faction = "faction_1".to_string();

        // 标记 (1,0) 为敌方领地
        owners.insert(HexCoord::new(1, 0).to_tile_key(), "faction_2".to_string());

        let result = find_path(
            HexCoord::new(0, 0),
            HexCoord::new(3, 0),
            &terrain,
            &terrain_defs,
            &owners,
            &faction,
            1000,
        );

        assert!(result.found);
        // 纯平原代价 = 3.0；含敌方领地应更高（至少 1 格被惩罚）
        assert!(
            result.cost > 3.0,
            "cost with enemy tile should exceed 3.0, got {}",
            result.cost
        );
    }

    #[test]
    fn test_path_endpoints_correct() {
        let terrain = create_terrain_map(20, 20);
        let terrain_defs = BTreeMap::new();
        let owners = BTreeMap::new();
        let faction = "faction_1".to_string();

        let start = HexCoord::new(1, 1);
        let goal = HexCoord::new(15, 10);

        let result = find_path(
            start,
            goal,
            &terrain,
            &terrain_defs,
            &owners,
            &faction,
            5000,
        );

        assert!(result.found);
        assert_eq!(result.path.first(), Some(&start));
        assert_eq!(result.path.last(), Some(&goal));
    }
}
