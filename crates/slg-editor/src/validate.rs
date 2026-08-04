//! 编辑器校验系统
//!
//! 提供多层级校验：实时校验（笔刷/放置时）与保存前全量校验。
//! 每个校验项可附带修复建议与受影响的格子列表。

use slg_core::map::grid::HexCoord;
use slg_data::ids::TileKey;
use slg_data::map_doc::*;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

// ===========================================================================
// 校验结果类型
// ===========================================================================

/// 校验条目（扩展信息）
#[derive(Debug, Clone)]
pub struct ValidationItem {
    /// 错误/警告描述
    pub message: String,
    /// 修复建议（可选）
    pub fix_suggestion: Option<String>,
    /// 受影响的格子坐标
    pub affected_tiles: Vec<TileKey>,
}

/// 校验结果（扩展）
#[derive(Debug)]
pub struct ValidationResult {
    pub errors: Vec<ValidationItem>,
    pub warnings: Vec<ValidationItem>,
}

impl ValidationResult {
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }

    /// 添加一个错误
    pub fn add_error(&mut self, message: String, fix: Option<String>, tiles: Vec<TileKey>) {
        self.errors.push(ValidationItem {
            message,
            fix_suggestion: fix,
            affected_tiles: tiles,
        });
    }

    /// 添加一个警告
    pub fn add_warning(&mut self, message: String, fix: Option<String>, tiles: Vec<TileKey>) {
        self.warnings.push(ValidationItem {
            message,
            fix_suggestion: fix,
            affected_tiles: tiles,
        });
    }

    /// 收集所有消息（兼容旧接口）
    pub fn all_messages(&self) -> Vec<String> {
        let mut msgs = Vec::new();
        for e in &self.errors {
            msgs.push(format!("[ERROR] {}", e.message));
        }
        for w in &self.warnings {
            msgs.push(format!("[WARN] {}", w.message));
        }
        msgs
    }
}

// ===========================================================================
// 地形解码辅助
// =========================================================================/// 解码 RLE 地形为 (TileKey -> terrain_u8) 映射
///
/// terrain_id 映射与 TerrainType::to_u8 对齐：
///   0=Plains, 1=Mountain, 2=Water, 3=Forest, 4=Desert, 5=Swamp, 6=Hills, 7=Pass
fn decode_terrain_map(doc: &MapDocument) -> BTreeMap<TileKey, u8> {
    let width = doc.meta.width as i32;
    let height = doc.meta.height as i32;

    // 展开 RLE
    let mut flat = Vec::with_capacity((width * height) as usize);
    for (terrain_id, count) in &doc.terrain.rle_data {
        let byte = terrain_id_to_u8(terrain_id);
        for _ in 0..*count {
            flat.push(byte);
        }
    }
    // 不足部分填充平原
    let total = (width * height) as usize;
    while flat.len() < total {
        flat.push(0);
    }

    let mut map = BTreeMap::new();
    for (idx, _) in flat.iter().enumerate().take(total) {
        let q = (idx as i32) % width;
        let r = (idx as i32) / width;
        let key = HexCoord::new(q, r).to_tile_key();
        map.insert(key, flat[idx]);
    }
    map
}

/// terrain_type_id 字符串 -> u8 编码
fn terrain_id_to_u8(id: &str) -> u8 {
    match id {
        "terrain_plains" => 0,
        "terrain_mountain" => 1,
        "terrain_water" => 2,
        "terrain_forest" => 3,
        "terrain_desert" => 4,
        "terrain_swamp" => 5,
        "terrain_hills" => 6,
        "terrain_pass" => 7,
        _ => 0, // 未知地形默认为平原
    }
}

// ===========================================================================
// 校验函数
// ===========================================================================

/// 实体重叠检测（旧接口，返回纯字符串列表）
pub fn check_entity_overlap(doc: &MapDocument) -> Vec<String> {
    let items = validate_entity_overlap_ext(doc);
    items.into_iter().map(|i| i.message).collect()
}

/// 实体重叠检测（扩展版，带修复建议与受影响格子）
///
/// 检查同一 TileKey 是否放置了多个实体。
/// 由于 BTreeMap 的 key 唯一性，实际检测的是同一位置重复插入后
/// 最终只保留一个的情况——通过扫描 placements 发现 key 冲突。
///
/// 注意：BTreeMap 本身不允许重复 key，所以真正的"重叠"发生在
/// 编辑器命令执行时。这里做的是检测 EntityLayer 内部一致性，
/// 以及与 RiverLayer 等其他层的冲突。
pub fn validate_entity_overlap_ext(doc: &MapDocument) -> Vec<ValidationItem> {
    let mut issues = Vec::new();

    // 检查实体层内部：由于 BTreeMap key 唯一，这里实际上不会检测到重复。
    // 但可以检测实体是否占据了河流格或水域格。
    let terrain_map = decode_terrain_map(doc);

    for (&key, placement) in &doc.entities.placements {
        // 检查实体是否放在水域上
        if let Some(&terrain) = terrain_map.get(&key) {
            if terrain == 2 {
                // Water
                let coord = HexCoord::from_tile_key(key);
                issues.push(ValidationItem {
                    message: format!(
                        "实体 '{}' 放置在水域格 {:?} 上",
                        placement.entity_type, coord
                    ),
                    fix_suggestion: Some("将实体移至陆地格，或先将该格改为陆地地形".to_string()),
                    affected_tiles: vec![key],
                });
            }
        }

        // 检查实体是否与河流段重叠
        if doc.rivers.segments.contains_key(&key) {
            let coord = HexCoord::from_tile_key(key);
            issues.push(ValidationItem {
                message: format!(
                    "实体 '{}' 与河流在格 {:?} 处重叠",
                    placement.entity_type, coord
                ),
                fix_suggestion: Some("将实体或河流移开，避免重叠".to_string()),
                affected_tiles: vec![key],
            });
        }
    }

    issues
}

/// 连通性校验
///
/// 使用 BFS 检测地图上非水域格的连通分量。
/// 如果存在多个不相连的陆地区域，则产生警告。
pub fn validate_connectivity(doc: &MapDocument) -> Vec<ValidationItem> {
    let mut issues = Vec::new();

    let terrain_map = decode_terrain_map(doc);
    let width = doc.meta.width as i32;
    let height = doc.meta.height as i32;

    if width == 0 || height == 0 {
        return issues;
    }

    // 收集所有非水域格
    let land_tiles: BTreeSet<TileKey> = terrain_map
        .iter()
        .filter(|(_, &t)| t != 2) // 非水域
        .map(|(&k, _)| k)
        .collect();

    if land_tiles.is_empty() {
        issues.push(ValidationItem {
            message: "地图上没有陆地格，全部为水域".to_string(),
            fix_suggestion: Some("添加一些陆地格以供游戏使用".to_string()),
            affected_tiles: vec![],
        });
        return issues;
    }

    // BFS 找连通分量
    let mut visited: BTreeSet<TileKey> = BTreeSet::new();
    let mut components: Vec<Vec<TileKey>> = Vec::new();

    for &start_key in &land_tiles {
        if visited.contains(&start_key) {
            continue;
        }

        // BFS
        let mut component = Vec::new();
        let mut queue = VecDeque::new();
        queue.push_back(start_key);
        visited.insert(start_key);

        while let Some(current) = queue.pop_front() {
            component.push(current);

            let coord = HexCoord::from_tile_key(current);
            for neighbor in &coord.neighbors() {
                let nkey = neighbor.to_tile_key();
                if !visited.contains(&nkey) && land_tiles.contains(&nkey) {
                    visited.insert(nkey);
                    queue.push_back(nkey);
                }
            }
        }

        components.push(component);
    }

    // 如果有多个连通分量，说明存在孤立陆地
    if components.len() > 1 {
        // 按大小排序，最大的作为"主大陆"
        components.sort_by_key(|c| std::cmp::Reverse(c.len()));

        for (i, component) in components.iter().enumerate().skip(1) {
            let size = component.len();
            let sample = component.first().copied().unwrap_or(0);
            let coord = HexCoord::from_tile_key(sample);
            issues.push(ValidationItem {
                message: format!(
                    "发现孤立陆地区域 #{}（{} 格），起始于 {:?}，与主大陆不连通",
                    i, size, coord
                ),
                fix_suggestion: Some(
                    "添加桥梁或渡口连接该区域与主大陆，或删除孤立区域".to_string(),
                ),
                affected_tiles: component.clone(),
            });
        }
    }

    issues
}

/// 资源平衡校验
///
/// 检查：
/// 1. 地图上是否存在资源点
/// 2. 各势力的初始资源分布是否均衡
pub fn validate_resource_balance(doc: &MapDocument) -> Vec<ValidationItem> {
    let mut issues = Vec::new();

    // 检查是否有资源点
    if doc.resources.entries.is_empty() {
        issues.push(ValidationItem {
            message: "地图上没有任何资源点".to_string(),
            fix_suggestion: Some("使用资源笔刷添加金币、粮食、木材等资源点".to_string()),
            affected_tiles: vec![],
        });
        return issues;
    }

    // 统计各势力拥有的资源数量
    let mut faction_resources: BTreeMap<Option<String>, Vec<TileKey>> = BTreeMap::new();
    for (&key, placement) in &doc.entities.placements {
        if placement.entity_type == "city" {
            faction_resources
                .entry(placement.faction_id.clone())
                .or_default()
                .push(key);
        }
    }

    // 检查是否有势力没有城池
    let factions_with_cities: BTreeSet<Option<String>> =
        faction_resources.keys().cloned().collect();
    let all_factions: BTreeSet<String> = doc
        .entities
        .placements
        .values()
        .filter_map(|p| p.faction_id.clone())
        .collect();

    for faction in &all_factions {
        if !factions_with_cities.contains(&Some(faction.clone())) {
            issues.push(ValidationItem {
                message: format!("势力 '{}' 没有城池", faction),
                fix_suggestion: Some(format!("为势力 '{}' 至少放置一个城池", faction)),
                affected_tiles: vec![],
            });
        }
    }

    // 检查各势力城池数量是否均衡
    let city_counts: Vec<(Option<String>, usize)> = faction_resources
        .iter()
        .map(|(k, v)| (k.clone(), v.len()))
        .collect();

    if city_counts.len() > 1 {
        let max_cities = city_counts.iter().map(|(_, c)| *c).max().unwrap_or(0);
        let min_cities = city_counts.iter().map(|(_, c)| *c).min().unwrap_or(0);

        if max_cities > 0 && min_cities == 0 {
            // 有些势力没有城池
            for (faction, count) in &city_counts {
                if *count == 0 {
                    let name = faction.as_deref().unwrap_or("(无势力)");
                    issues.push(ValidationItem {
                        message: format!("势力 '{}' 没有城池，可能影响游戏平衡", name),
                        fix_suggestion: Some("为该势力添加至少一个城池".to_string()),
                        affected_tiles: vec![],
                    });
                }
            }
        } else if max_cities > min_cities * 2 && min_cities > 0 {
            // 城池数量差距过大（超过 2 倍）
            issues.push(ValidationItem {
                message: format!(
                    "各势力城池数量不均衡：最多 {} 个，最少 {} 个",
                    max_cities, min_cities
                ),
                fix_suggestion: Some("调整各势力城池数量，使差距不超过 2 倍".to_string()),
                affected_tiles: vec![],
            });
        }
    }

    // 检查资源点密度
    let total_tiles = (doc.meta.width * doc.meta.height) as usize;
    let resource_count = doc.resources.entries.len();
    let density = if total_tiles > 0 {
        resource_count as f64 / total_tiles as f64
    } else {
        0.0
    };

    if density < 0.01 {
        issues.push(ValidationItem {
            message: format!(
                "资源点过少（{} 个/{} 格，密度 {:.1}%）",
                resource_count,
                total_tiles,
                density * 100.0
            ),
            fix_suggestion: Some("建议资源密度在 2%~5% 之间，使用资源笔刷添加更多资源".to_string()),
            affected_tiles: vec![],
        });
    } else if density > 0.30 {
        issues.push(ValidationItem {
            message: format!(
                "资源点过多（{} 个/{} 格，密度 {:.1}%）",
                resource_count,
                total_tiles,
                density * 100.0
            ),
            fix_suggestion: Some("建议资源密度在 2%~5% 之间，考虑减少部分资源点".to_string()),
            affected_tiles: vec![],
        });
    }

    issues
}

/// 河流连续性校验（扩展版，带修复建议）
///
/// 检查：
/// 1. 是否存在孤立河流段（无相邻河流格）
/// 2. 河流是否形成环路
/// 3. 渡口位置是否合理
pub fn validate_river_continuity_ext(doc: &MapDocument) -> Vec<ValidationItem> {
    let mut issues = Vec::new();
    let river_layer = &doc.rivers;

    if river_layer.segments.is_empty() {
        return issues;
    }

    // 1. 检查孤立河流段
    for &key in river_layer.segments.keys() {
        let coord = HexCoord::from_tile_key(key);
        let has_adjacent_river = coord
            .neighbors()
            .iter()
            .any(|n| river_layer.segments.contains_key(&n.to_tile_key()));

        if !has_adjacent_river {
            issues.push(ValidationItem {
                message: format!("河流格 {:?} 孤立，无相邻河流格", coord),
                fix_suggestion: Some("删除孤立河流格，或添加相邻河流格使其连通".to_string()),
                affected_tiles: vec![key],
            });
        }
    }

    // 2. 检查河流环路
    if let Some(cycle_coord) = detect_river_cycle(river_layer) {
        issues.push(ValidationItem {
            message: format!("河流形成环路，经过坐标: {:?}", cycle_coord),
            fix_suggestion: Some("移除环路上的一段河流以打断环路".to_string()),
            affected_tiles: vec![cycle_coord.to_tile_key()],
        });
    }

    // 3. 检查渡口位置合理性
    for (&key, segment) in &river_layer.segments {
        if !segment.is_ford {
            continue;
        }
        let coord = HexCoord::from_tile_key(key);

        // 渡口应该位于河流段上
        // 检查渡口是否在河流的端点（不合理）还是在中间段
        let adjacent_river_count = coord
            .neighbors()
            .iter()
            .filter(|n| river_layer.segments.contains_key(&n.to_tile_key()))
            .count();

        if adjacent_river_count == 0 {
            issues.push(ValidationItem {
                message: format!("渡口 {:?} 位于孤立河流格上，无法起到渡河作用", coord),
                fix_suggestion: Some("将渡口移至河流连续段上，或先修复河流连通性".to_string()),
                affected_tiles: vec![key],
            });
        }
    }

    issues
}

/// 保存前全量校验（旧接口，返回纯字符串的 ValidationResult）
///
/// 保持向后兼容，内部调用扩展版校验。
pub fn validate_for_save(doc: &MapDocument) -> ValidationResult {
    let full = validate_for_save_full(doc);
    // 转换为旧格式：只保留消息字符串
    ValidationResult {
        errors: full
            .errors
            .into_iter()
            .map(|i| ValidationItem {
                message: i.message,
                fix_suggestion: i.fix_suggestion,
                affected_tiles: i.affected_tiles,
            })
            .collect(),
        warnings: full
            .warnings
            .into_iter()
            .map(|i| ValidationItem {
                message: i.message,
                fix_suggestion: i.fix_suggestion,
                affected_tiles: i.affected_tiles,
            })
            .collect(),
    }
}

/// 保存前全量校验（扩展版）
///
/// 执行所有校验项，返回包含修复建议的完整校验结果。
pub fn validate_for_save_full(doc: &MapDocument) -> ValidationResult {
    let mut result = ValidationResult {
        errors: Vec::new(),
        warnings: Vec::new(),
    };

    // 1. 基本完整性检查
    if doc.meta.width == 0 || doc.meta.height == 0 {
        result.add_error(
            "地图尺寸不能为 0".to_string(),
            Some("设置有效的地图宽度和高度".to_string()),
            vec![],
        );
    }

    if doc.meta.name.is_empty() {
        result.add_warning(
            "地图名称为空".to_string(),
            Some("为地图设置一个有意义的名称".to_string()),
            vec![],
        );
    }

    // 2. 实体重叠 / 冲突校验
    let overlap_issues = validate_entity_overlap_ext(doc);
    for issue in overlap_issues {
        result.errors.push(issue);
    }

    // 3. 连通性校验
    let connectivity_issues = validate_connectivity(doc);
    for issue in connectivity_issues {
        result.warnings.push(issue);
    }

    // 4. 资源平衡校验
    let balance_issues = validate_resource_balance(doc);
    for issue in balance_issues {
        result.warnings.push(issue);
    }

    // 5. 河流连续性校验
    let river_issues = validate_river_continuity_ext(doc);
    for issue in river_issues {
        result.warnings.push(issue);
    }

    result
}

// ===========================================================================
// 河流连续性校验（内部辅助）
// ===========================================================================

/// 河流连续性校验（旧接口）
///
/// 检查河流层中是否存在孤立段或环路。
/// 返回警告信息列表（孤立段、环路等）。
pub fn validate_river_continuity(river_layer: &RiverLayer) -> Vec<String> {
    let mut errors = Vec::new();

    if river_layer.segments.is_empty() {
        return errors;
    }

    // 1. 检查孤立河流段
    for key in river_layer.segments.keys() {
        let coord = HexCoord::from_tile_key(*key);
        let has_adjacent_river = coord
            .neighbors()
            .iter()
            .any(|n| river_layer.segments.contains_key(&n.to_tile_key()));

        if !has_adjacent_river {
            errors.push(format!("河流格 {:?} 孤立，无相邻河流格", coord));
        }
    }

    // 2. 检查河流是否形成环路
    if let Some(cycle_info) = detect_river_cycle(river_layer) {
        errors.push(format!("河流形成环路，经过坐标: {:?}", cycle_info));
    }

    errors
}

/// 使用 BFS 检测河流环路
///
/// 如果检测到环路，返回环路上的一个坐标；否则返回 None。
fn detect_river_cycle(river_layer: &RiverLayer) -> Option<HexCoord> {
    let segments = &river_layer.segments;
    if segments.is_empty() {
        return None;
    }

    let mut visited: BTreeSet<u64> = BTreeSet::new();

    // 从任意未访问的节点开始 BFS
    for &key in segments.keys() {
        if visited.contains(&key) {
            continue;
        }

        // BFS，记录父节点
        let mut queue: VecDeque<(u64, Option<u64>)> = VecDeque::new();
        let mut parent: BTreeMap<u64, u64> = BTreeMap::new();

        queue.push_back((key, None));

        while let Some((current, from)) = queue.pop_front() {
            if visited.contains(&current) {
                // 已访问节点 — 如果不是父节点，则发现环路
                if let Some(f) = from {
                    if current != f {
                        return Some(HexCoord::from_tile_key(current));
                    }
                }
                continue;
            }

            visited.insert(current);
            if let Some(f) = from {
                parent.insert(current, f);
            }

            let coord = HexCoord::from_tile_key(current);
            for neighbor in &coord.neighbors() {
                let nkey = neighbor.to_tile_key();
                if segments.contains_key(&nkey) {
                    // 如果邻居已访问且不是父节点，发现环路
                    if visited.contains(&nkey) {
                        if let Some(f) = from {
                            if nkey != f {
                                return Some(*neighbor);
                            }
                        }
                    } else {
                        queue.push_back((nkey, Some(current)));
                    }
                }
            }
        }
    }

    None
}

/// 获取河流断开位置列表
///
/// 返回所有孤立河流段的坐标。
pub fn find_river_breaks(river_layer: &RiverLayer) -> Vec<HexCoord> {
    let mut breaks = Vec::new();

    for key in river_layer.segments.keys() {
        let coord = HexCoord::from_tile_key(*key);
        let has_adjacent_river = coord
            .neighbors()
            .iter()
            .any(|n| river_layer.segments.contains_key(&n.to_tile_key()));

        if !has_adjacent_river {
            breaks.push(coord);
        }
    }

    breaks
}

// ===========================================================================
// 测试
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // 测试辅助
    // -----------------------------------------------------------------------

    fn create_test_doc() -> MapDocument {
        MapDocument {
            meta: MapMeta {
                name: "测试地图".to_string(),
                seed: 42,
                width: 10,
                height: 10,
                preset_name: None,
            },
            terrain: TerrainLayer {
                rle_data: vec![("terrain_plains".to_string(), 100)],
                total_tiles: 100,
            },
            resources: ResourceLayer {
                entries: BTreeMap::new(),
            },
            entities: EntityLayer {
                placements: BTreeMap::new(),
            },
            rules: RuleLayer {
                zones: Vec::new(),
                triggers: Vec::new(),
            },
            rivers: RiverLayer::default(),
        }
    }

    fn create_doc_with_terrain(
        terrain_rle: Vec<(String, u32)>,
        width: u32,
        height: u32,
    ) -> MapDocument {
        let total = width * height;
        MapDocument {
            meta: MapMeta {
                name: "测试地图".to_string(),
                seed: 42,
                width,
                height,
                preset_name: None,
            },
            terrain: TerrainLayer {
                rle_data: terrain_rle,
                total_tiles: total,
            },
            resources: ResourceLayer {
                entries: BTreeMap::new(),
            },
            entities: EntityLayer {
                placements: BTreeMap::new(),
            },
            rules: RuleLayer {
                zones: Vec::new(),
                triggers: Vec::new(),
            },
            rivers: RiverLayer::default(),
        }
    }

    fn make_segment(width: u8) -> RiverSegment {
        RiverSegment {
            width,
            is_ford: false,
            direction: None,
        }
    }

    fn make_ford(width: u8) -> RiverSegment {
        RiverSegment {
            width,
            is_ford: true,
            direction: None,
        }
    }

    // -----------------------------------------------------------------------
    // ValidationResult 扩展接口测试
    // -----------------------------------------------------------------------

    #[test]
    fn test_validation_result_add_error() {
        let mut result = ValidationResult {
            errors: Vec::new(),
            warnings: Vec::new(),
        };
        result.add_error("测试错误".to_string(), Some("修复方案".to_string()), vec![]);
        assert!(!result.is_valid());
        assert_eq!(result.errors.len(), 1);
        assert_eq!(result.errors[0].message, "测试错误");
        assert_eq!(
            result.errors[0].fix_suggestion,
            Some("修复方案".to_string())
        );
    }

    #[test]
    fn test_validation_result_add_warning() {
        let mut result = ValidationResult {
            errors: Vec::new(),
            warnings: Vec::new(),
        };
        result.add_warning("测试警告".to_string(), None, vec![]);
        assert!(result.is_valid()); // warnings 不影响 is_valid
        assert_eq!(result.warnings.len(), 1);
    }

    #[test]
    fn test_validation_result_all_messages() {
        let mut result = ValidationResult {
            errors: Vec::new(),
            warnings: Vec::new(),
        };
        result.add_error("错误1".to_string(), None, vec![]);
        result.add_warning("警告1".to_string(), None, vec![]);
        let msgs = result.all_messages();
        assert_eq!(msgs.len(), 2);
        assert!(msgs[0].contains("[ERROR]"));
        assert!(msgs[1].contains("[WARN]"));
    }

    // -----------------------------------------------------------------------
    // 实体重叠校验测试
    // -----------------------------------------------------------------------

    #[test]
    fn test_validate_entity_overlap_no_overlap() {
        let mut doc = create_test_doc();
        doc.entities.placements.insert(
            HexCoord::new(5, 5).to_tile_key(),
            EntityPlacement {
                entity_type: "city".to_string(),
                faction_id: Some("faction_wei".to_string()),
                properties: BTreeMap::new(),
            },
        );
        let issues = validate_entity_overlap_ext(&doc);
        assert!(
            issues.is_empty(),
            "无重叠时不应有错误，但得到: {:?}",
            issues
        );
    }

    #[test]
    fn test_validate_entity_overlap_water() {
        let mut doc = create_doc_with_terrain(
            vec![
                ("terrain_plains".to_string(), 50),
                ("terrain_water".to_string(), 50),
            ],
            10,
            10,
        );
        // 在水域格 (0, 5) 放置实体（索引 50 -> q=0, r=5）
        let water_key = HexCoord::new(0, 5).to_tile_key();
        doc.entities.placements.insert(
            water_key,
            EntityPlacement {
                entity_type: "city".to_string(),
                faction_id: None,
                properties: BTreeMap::new(),
            },
        );

        let issues = validate_entity_overlap_ext(&doc);
        assert!(!issues.is_empty(), "实体放在水域上应产生错误");
        assert!(issues[0].message.contains("水域"));
        assert!(issues[0].fix_suggestion.is_some());
    }

    #[test]
    fn test_validate_entity_overlap_river() {
        let mut doc = create_test_doc();
        let key = HexCoord::new(3, 3).to_tile_key();
        // 放置实体
        doc.entities.placements.insert(
            key,
            EntityPlacement {
                entity_type: "fortress".to_string(),
                faction_id: None,
                properties: BTreeMap::new(),
            },
        );
        // 同一位置有河流
        doc.rivers.segments.insert(key, make_segment(1));

        let issues = validate_entity_overlap_ext(&doc);
        assert!(!issues.is_empty(), "实体与河流重叠应产生错误");
        assert!(issues[0].message.contains("河流"));
    }

    #[test]
    fn test_check_entity_overlap_old_api() {
        let doc = create_test_doc();
        let errors = check_entity_overlap(&doc);
        assert!(errors.is_empty());
    }

    // -----------------------------------------------------------------------
    // 连通性校验测试
    // -----------------------------------------------------------------------

    #[test]
    fn test_validate_connectivity_single_landmass() {
        let doc = create_test_doc(); // 全平原，应该连通
        let issues = validate_connectivity(&doc);
        assert!(
            issues.is_empty(),
            "单一连通陆地不应有警告，但得到: {:?}",
            issues
        );
    }

    #[test]
    fn test_validate_connectivity_isolated_land() {
        // 创建一个被水域隔开的地图
        // 上半部分水域，下半部分陆地，但有一个孤立的陆地格
        let doc = create_doc_with_terrain(
            vec![
                ("terrain_plains".to_string(), 1), // (0,0) 孤立陆地
                ("terrain_water".to_string(), 98), // 中间全是水域
                ("terrain_plains".to_string(), 1), // (9,9) 主大陆的最后一个格
            ],
            10,
            10,
        );

        let issues = validate_connectivity(&doc);
        // (0,0) 和 (9,9) 都是孤立的（中间隔着水域），应该检测到
        assert!(!issues.is_empty(), "孤立陆地应产生警告");
        // 应该有修复建议
        for issue in &issues {
            assert!(issue.fix_suggestion.is_some(), "每个校验项应有修复建议");
        }
    }

    #[test]
    fn test_validate_connectivity_all_water() {
        let doc = create_doc_with_terrain(vec![("terrain_water".to_string(), 100)], 10, 10);
        let issues = validate_connectivity(&doc);
        assert!(!issues.is_empty(), "全水域应产生警告");
        assert!(issues[0].message.contains("没有陆地"));
    }

    // -----------------------------------------------------------------------
    // 资源平衡校验测试
    // -----------------------------------------------------------------------

    #[test]
    fn test_validate_resource_balance_no_resources() {
        let doc = create_test_doc();
        let issues = validate_resource_balance(&doc);
        assert!(!issues.is_empty(), "无资源应产生警告");
        assert!(issues[0].message.contains("没有"));
    }

    #[test]
    fn test_validate_resource_balance_ok() {
        let mut doc = create_test_doc();
        // 添加适量资源
        for i in 0..5 {
            doc.resources.entries.insert(
                HexCoord::new(i, 0).to_tile_key(),
                ResourceEntry {
                    resource_type: "gold".to_string(),
                    level: 1,
                },
            );
        }
        // 添加一个城池
        doc.entities.placements.insert(
            HexCoord::new(5, 5).to_tile_key(),
            EntityPlacement {
                entity_type: "city".to_string(),
                faction_id: Some("faction_wei".to_string()),
                properties: BTreeMap::new(),
            },
        );

        let issues = validate_resource_balance(&doc);
        // 不应有关于"没有资源"的警告
        let no_resource_warning = issues.iter().any(|i| i.message.contains("没有任何资源"));
        assert!(!no_resource_warning, "有资源时不应报'没有资源'");
    }

    #[test]
    fn test_validate_resource_balance_unbalanced_cities() {
        let mut doc = create_test_doc();
        // 添加一些资源
        for i in 0..3 {
            doc.resources.entries.insert(
                HexCoord::new(i, 0).to_tile_key(),
                ResourceEntry {
                    resource_type: "food".to_string(),
                    level: 1,
                },
            );
        }
        // faction_wei 有 3 个城池，faction_shu 有 0 个
        for i in 0..3 {
            doc.entities.placements.insert(
                HexCoord::new(i + 2, 5).to_tile_key(),
                EntityPlacement {
                    entity_type: "city".to_string(),
                    faction_id: Some("faction_wei".to_string()),
                    properties: BTreeMap::new(),
                },
            );
        }
        doc.entities.placements.insert(
            HexCoord::new(8, 8).to_tile_key(),
            EntityPlacement {
                entity_type: "city".to_string(),
                faction_id: Some("faction_shu".to_string()),
                properties: BTreeMap::new(),
            },
        );

        let issues = validate_resource_balance(&doc);
        // 应该检测到不均衡（3 vs 1）
        let has_balance_issue = issues.iter().any(|i| i.message.contains("不均衡"));
        assert!(
            has_balance_issue,
            "应检测到城池数量不均衡，但得到: {:?}",
            issues
        );
    }

    #[test]
    fn test_validate_resource_balance_faction_without_city() {
        let mut doc = create_test_doc();
        // 添加一些资源
        doc.resources.entries.insert(
            HexCoord::new(0, 0).to_tile_key(),
            ResourceEntry {
                resource_type: "gold".to_string(),
                level: 1,
            },
        );
        // 有势力但没有城池
        doc.entities.placements.insert(
            HexCoord::new(5, 5).to_tile_key(),
            EntityPlacement {
                entity_type: "fortress".to_string(),
                faction_id: Some("faction_wu".to_string()),
                properties: BTreeMap::new(),
            },
        );

        let issues = validate_resource_balance(&doc);
        let has_no_city = issues.iter().any(|i| i.message.contains("没有城池"));
        assert!(has_no_city, "应检测到势力没有城池，但得到: {:?}", issues);
    }

    // -----------------------------------------------------------------------
    // 河流连续性校验（扩展版）测试
    // -----------------------------------------------------------------------

    #[test]
    fn test_validate_river_continuity_ext_valid() {
        let mut doc = create_test_doc();
        // 连续的河流
        doc.rivers
            .segments
            .insert(HexCoord::new(5, 5).to_tile_key(), make_segment(1));
        doc.rivers
            .segments
            .insert(HexCoord::new(6, 5).to_tile_key(), make_segment(1));

        let issues = validate_river_continuity_ext(&doc);
        assert!(
            issues.is_empty(),
            "连续河流不应有警告，但得到: {:?}",
            issues
        );
    }

    #[test]
    fn test_validate_river_continuity_ext_isolated() {
        let mut doc = create_test_doc();
        // 孤立河流格
        doc.rivers
            .segments
            .insert(HexCoord::new(5, 5).to_tile_key(), make_segment(1));

        let issues = validate_river_continuity_ext(&doc);
        assert!(!issues.is_empty(), "孤立河流应产生警告");
        assert!(issues[0].message.contains("孤立"));
        assert!(issues[0].fix_suggestion.is_some(), "应有修复建议");
    }

    #[test]
    fn test_validate_river_continuity_ext_cycle() {
        let mut doc = create_test_doc();
        // 构造环路
        doc.rivers
            .segments
            .insert(HexCoord::new(0, 0).to_tile_key(), make_segment(1));
        doc.rivers
            .segments
            .insert(HexCoord::new(1, 0).to_tile_key(), make_segment(1));
        doc.rivers
            .segments
            .insert(HexCoord::new(1, -1).to_tile_key(), make_segment(1));

        let issues = validate_river_continuity_ext(&doc);
        let has_cycle = issues.iter().any(|i| i.message.contains("环路"));
        assert!(has_cycle, "应检测到河流环路，但得到: {:?}", issues);
    }

    #[test]
    fn test_validate_river_continuity_ext_ford_on_isolated() {
        let mut doc = create_test_doc();
        // 渡口在孤立格上
        doc.rivers
            .segments
            .insert(HexCoord::new(5, 5).to_tile_key(), make_ford(1));

        let issues = validate_river_continuity_ext(&doc);
        let has_ford_issue = issues.iter().any(|i| i.message.contains("渡口"));
        assert!(
            has_ford_issue,
            "孤立格上的渡口应产生警告，但得到: {:?}",
            issues
        );
    }

    // -----------------------------------------------------------------------
    // 保存前全量校验测试
    // -----------------------------------------------------------------------

    #[test]
    fn test_validate_for_save_full_basic() {
        let doc = create_test_doc();
        let result = validate_for_save_full(&doc);
        // 空资源会触发警告，但不应有错误
        assert!(result.is_valid(), "基本地图不应有错误");
    }

    #[test]
    fn test_validate_for_save_full_zero_size() {
        let mut doc = create_test_doc();
        doc.meta.width = 0;
        let result = validate_for_save_full(&doc);
        assert!(!result.is_valid(), "尺寸为 0 应产生错误");
        assert!(result.errors[0].fix_suggestion.is_some());
    }

    #[test]
    fn test_validate_for_save_full_empty_name() {
        let mut doc = create_test_doc();
        doc.meta.name = String::new();
        let result = validate_for_save_full(&doc);
        assert!(result.is_valid(), "名称为空不应导致校验失败");
        let has_name_warning = result.warnings.iter().any(|w| w.message.contains("名称"));
        assert!(has_name_warning, "应有名称为空的警告");
    }

    #[test]
    fn test_validate_for_save_old_api() {
        let doc = create_test_doc();
        let result = validate_for_save(&doc);
        assert!(result.is_valid(), "旧 API 也应正常工作");
    }

    // -----------------------------------------------------------------------
    // 旧版河流连续性校验测试（保持不变）
    // -----------------------------------------------------------------------

    #[test]
    fn test_river_continuity_valid() {
        let mut river_layer = RiverLayer::default();

        // 连续的河流：两个相邻格
        river_layer
            .segments
            .insert(HexCoord::new(5, 5).to_tile_key(), make_segment(1));
        river_layer
            .segments
            .insert(HexCoord::new(6, 5).to_tile_key(), make_segment(1));

        let errors = validate_river_continuity(&river_layer);
        assert!(
            errors.is_empty(),
            "连续河流不应有错误，但得到: {:?}",
            errors
        );
    }

    #[test]
    fn test_river_isolated() {
        let mut river_layer = RiverLayer::default();

        // 孤立的河流格
        river_layer
            .segments
            .insert(HexCoord::new(5, 5).to_tile_key(), make_segment(1));

        let errors = validate_river_continuity(&river_layer);
        assert!(!errors.is_empty(), "孤立河流应产生错误");
        assert!(errors[0].contains("孤立"));
    }

    #[test]
    fn test_river_empty_layer() {
        let river_layer = RiverLayer::default();
        let errors = validate_river_continuity(&river_layer);
        assert!(errors.is_empty(), "空河流层不应有错误");
    }

    #[test]
    fn test_river_long_chain() {
        let mut river_layer = RiverLayer::default();

        // 一条长链：(0,0) -> (1,0) -> (2,0) -> (3,0)
        for q in 0..=3 {
            river_layer
                .segments
                .insert(HexCoord::new(q, 0).to_tile_key(), make_segment(1));
        }

        let errors = validate_river_continuity(&river_layer);
        assert!(
            errors.is_empty(),
            "连续河流链不应有错误，但得到: {:?}",
            errors
        );
    }

    #[test]
    fn test_river_chain_with_break() {
        let mut river_layer = RiverLayer::default();

        // (0,0) -> (1,0) ... (3,0) -> (4,0)
        river_layer
            .segments
            .insert(HexCoord::new(0, 0).to_tile_key(), make_segment(1));
        river_layer
            .segments
            .insert(HexCoord::new(1, 0).to_tile_key(), make_segment(1));
        river_layer
            .segments
            .insert(HexCoord::new(3, 0).to_tile_key(), make_segment(1));
        river_layer
            .segments
            .insert(HexCoord::new(4, 0).to_tile_key(), make_segment(1));

        let errors = validate_river_continuity(&river_layer);
        assert!(
            errors.is_empty(),
            "两段各自连续的河流不应报孤立错误，但得到: {:?}",
            errors
        );
    }

    #[test]
    fn test_river_cycle_detection() {
        let mut river_layer = RiverLayer::default();

        // 构造一个三角形环路
        river_layer
            .segments
            .insert(HexCoord::new(0, 0).to_tile_key(), make_segment(1));
        river_layer
            .segments
            .insert(HexCoord::new(1, 0).to_tile_key(), make_segment(1));
        river_layer
            .segments
            .insert(HexCoord::new(1, -1).to_tile_key(), make_segment(1));

        let errors = validate_river_continuity(&river_layer);
        let has_cycle_error = errors.iter().any(|e| e.contains("环路"));
        assert!(has_cycle_error, "应检测到河流环路，但得到: {:?}", errors);
    }

    #[test]
    fn test_find_river_breaks_empty() {
        let river_layer = RiverLayer::default();
        let breaks = find_river_breaks(&river_layer);
        assert!(breaks.is_empty());
    }

    #[test]
    fn test_find_river_breaks_single() {
        let mut river_layer = RiverLayer::default();
        river_layer
            .segments
            .insert(HexCoord::new(10, 10).to_tile_key(), make_segment(1));

        let breaks = find_river_breaks(&river_layer);
        assert_eq!(breaks.len(), 1);
        assert_eq!(breaks[0], HexCoord::new(10, 10));
    }

    #[test]
    fn test_find_river_breaks_none() {
        let mut river_layer = RiverLayer::default();
        river_layer
            .segments
            .insert(HexCoord::new(5, 5).to_tile_key(), make_segment(1));
        river_layer
            .segments
            .insert(HexCoord::new(6, 5).to_tile_key(), make_segment(1));

        let breaks = find_river_breaks(&river_layer);
        assert!(
            breaks.is_empty(),
            "连续河流不应有断开，但得到: {:?}",
            breaks
        );
    }

    // -----------------------------------------------------------------------
    // 地形解码测试
    // -----------------------------------------------------------------------

    #[test]
    fn test_decode_terrain_map() {
        let doc = create_doc_with_terrain(
            vec![
                ("terrain_plains".to_string(), 5),
                ("terrain_water".to_string(), 5),
            ],
            10,
            1, // 1 行
        );
        let map = decode_terrain_map(&doc);
        // 前 5 个是平原 (0)
        for q in 0..5 {
            let key = HexCoord::new(q, 0).to_tile_key();
            assert_eq!(map.get(&key), Some(&0), "格 ({},0) 应为平原", q);
        }
        // 后 5 个是水域 (2)（但只有 10 格，rle 只有 10）
        // 由于总格数是 10，rle 数据正好 10 格
        // (5,0)~(9,0) 应该没有对应 rle（只有 10 格，5 平原 + 5 水域 = 10）
        // 但 width=10, height=1，所以总共 10 格，rle 正好填满
        // idx 5~9 -> q=5~9, r=0
        for q in 5..10 {
            let key = HexCoord::new(q, 0).to_tile_key();
            assert_eq!(map.get(&key), Some(&2), "格 ({},0) 应为水域", q);
        }
    }
}
