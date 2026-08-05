//! 编辑器工具：Paint / FloodFill / PlaceEntity / River / Select / Stamp

pub mod river;
pub mod select;
pub mod stamp;

use crate::command::*;
use slg_core::map::grid::HexCoord;
use slg_data::ids::*;
use slg_data::map_doc::*;
use std::sync::Mutex;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// PaintBrush 命令：修改单格地形类型
pub struct PaintBrush {
    pub coord: HexCoord,
    pub new_terrain: TerrainTypeId,
    pub old_terrain: Mutex<Option<TerrainTypeId>>,
}

impl PaintBrush {
    pub fn new(coord: HexCoord, new_terrain: TerrainTypeId) -> Self {
        Self {
            coord,
            new_terrain,
            old_terrain: Mutex::new(None),
        }
    }
}

impl EditorCommand for PaintBrush {
    fn execute(&self, doc: &mut MapDocument) -> Result<(), String> {
        // M9.2: 真改 RLE 数据中指定位置
        let idx = coord_to_rle_idx(self.coord, doc.meta.width);
        if idx >= doc.terrain.total_tiles as usize {
            return Err(format!("坐标越界: idx={}", idx));
        }
        let old = get_terrain_at(doc, idx);
        if old == self.new_terrain {
            // 已经是目标地形, 不入 history
            return Ok(());
        }
        *self.old_terrain.lock().unwrap() = Some(old);
        set_terrain_at(doc, idx, &self.new_terrain);
        Ok(())
    }

    fn undo(&self, doc: &mut MapDocument) -> Result<(), String> {
        // 恢复旧地形
        let guard = self.old_terrain.lock().unwrap();
        if let Some(ref old) = *guard {
            let idx = coord_to_rle_idx(self.coord, doc.meta.width);
            set_terrain_at(doc, idx, old);
        }
        Ok(())
    }

    fn merge_hint(&self) -> Option<MergeHint> {
        Some(MergeHint {
            tool_type: "paint".to_string(),
            position: self.coord,
        })
    }
}

/// 把 HexCoord 转为 RLE local 索引
///
/// RLE 是按 `total_tiles` 顺序压缩 (0..total_tiles), 用 `q + r*width` 映射
/// M9.2 修: 之前用 `to_tile_key() as usize` 是错的 (那是全局 packed 编码)
pub fn coord_to_rle_idx(coord: HexCoord, width: u32) -> usize {
    (coord.r as usize) * (width as usize) + (coord.q as usize)
}

/// 从 RLE 数据中获取指定索引处的地形类型（简化实现）
///
/// M9.1: `pub` 让 editor_state.rs 的 FloodFill dispatch 可以复用
pub fn get_terrain_at(doc: &MapDocument, idx: usize) -> TerrainTypeId {
    let mut pos = 0;
    for (terrain_id, count) in &doc.terrain.rle_data {
        pos += *count as usize;
        if idx < pos {
            return terrain_id.clone();
        }
    }
    // 默认返回平原
    "terrain_plains".to_string()
}

/// 修改 RLE 数据中指定索引处的地形类型
///
/// 行为:
/// - idx 所在 run 的 terrain == new → 不变
/// - run.count == 1 → 替换 terrain 字段
/// - run.count > 1 且 idx 在 run 起始 → split: (new,1) + (old,count-1)
/// - run.count > 1 且 idx 在 run 末尾 → split: (old,count-1) + (new,1)
/// - run.count > 1 且 idx 在 run 中间 → split: (old,k) + (new,1) + (old,count-1-k)
///
/// M9.2: 替换之前的"简化"实现 (只追加 rle_data)
/// 语义: "delete + insert"
/// 1. 拆 old run, 留下 idx 位置给 new (前后段保留, 仍是同地形)
/// 2. 在原 idx 位置插 new run
/// 3. 合并相邻同地形 run
pub fn set_terrain_at(doc: &mut MapDocument, idx: usize, new: &TerrainTypeId) {
    if idx >= doc.terrain.total_tiles as usize {
        return; // 越界
    }
    let mut pos = 0;
    let rle_clone = doc.terrain.rle_data.clone();
    for (run_i, (terrain_id, count)) in rle_clone.into_iter().enumerate() {
        let start = pos;
        let end = pos + count as usize;
        if idx >= start && idx < end {
            if terrain_id == *new {
                return; // 已是目标地形
            }
            let run_idx = idx - start;
            let count = count as usize;

            // 1. 拆 old run: 拆出 idx 位置, 留前后段
            let mut replace_with: Vec<(TerrainTypeId, u32)> = Vec::new();
            if run_idx > 0 {
                replace_with.push((terrain_id.clone(), run_idx as u32));
            }
            if run_idx + 1 < count {
                replace_with.push((terrain_id, (count - run_idx - 1) as u32));
            }
            doc.terrain.rle_data.splice(run_i..run_i + 1, replace_with);

            // 2. 插 new run
            let insert_pos = if run_idx > 0 { run_i + 1 } else { run_i };
            doc.terrain.rle_data.insert(insert_pos, (new.clone(), 1));

            // 3. 合并相邻同地形 run
            merge_adjacent_runs(&mut doc.terrain.rle_data);
            return;
        }
        pos = end;
    }
    // 越界 / 找不到, 啥也不做
}

/// 合并相邻同地形 run
fn merge_adjacent_runs(rle: &mut Vec<(TerrainTypeId, u32)>) {
    let mut merged: Vec<(TerrainTypeId, u32)> = Vec::with_capacity(rle.len());
    for (terrain, count) in rle.drain(..) {
        if let Some(last) = merged.last_mut() {
            if last.0 == terrain {
                last.1 += count;
                continue;
            }
        }
        merged.push((terrain, count));
    }
    *rle = merged;
}

/// FloodFill 命令：泛洪填充
pub struct FloodFill {
    pub start: HexCoord,
    pub target_terrain: TerrainTypeId,
    pub fill_terrain: TerrainTypeId,
    pub affected: Mutex<Vec<HexCoord>>,
}

impl FloodFill {
    pub fn new(
        start: HexCoord,
        target_terrain: TerrainTypeId,
        fill_terrain: TerrainTypeId,
    ) -> Self {
        Self {
            start,
            target_terrain,
            fill_terrain,
            affected: Mutex::new(Vec::new()),
        }
    }

    /// 计算泛洪填充范围
    pub fn compute_fill(&self, doc: &MapDocument, max_width: u32, max_height: u32) {
        // BFS 从 start 开始，填充相同地形的连通区域
        let mut visited = BTreeSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(self.start);
        visited.insert(self.start.to_tile_key());

        while let Some(current) = queue.pop_front() {
            // 边界检查
            if current.q < 0
                || current.r < 0
                || current.q >= max_width as i32
                || current.r >= max_height as i32
            {
                continue;
            }

            let key = current.to_tile_key();
            let idx = key as usize;
            let terrain = get_terrain_at(doc, idx);

            // 只填充与目标地形相同的格子
            if terrain != self.target_terrain {
                continue;
            }

            self.affected.lock().unwrap().push(current);

            for neighbor in current.neighbors() {
                let nkey = neighbor.to_tile_key();
                if !visited.contains(&nkey) {
                    visited.insert(nkey);
                    queue.push_back(neighbor);
                }
            }
        }
    }
}

impl EditorCommand for FloodFill {
    fn execute(&self, _doc: &mut MapDocument) -> Result<(), String> {
        for _coord in self.affected.lock().unwrap().iter() {
            // 修改每个格子的地形（简化实现：实际应解码 RLE、修改、重新编码）
        }
        Ok(())
    }

    fn undo(&self, _doc: &mut MapDocument) -> Result<(), String> {
        // 恢复所有受影响格子的地形（简化实现）
        Ok(())
    }
}

/// PlaceEntity 命令：放置实体
pub struct PlaceEntity {
    pub coord: HexCoord,
    pub entity_type: String,
    pub properties: BTreeMap<String, String>,
}

impl EditorCommand for PlaceEntity {
    fn execute(&self, doc: &mut MapDocument) -> Result<(), String> {
        let key = self.coord.to_tile_key();
        doc.entities.placements.insert(
            key,
            EntityPlacement {
                entity_type: self.entity_type.clone(),
                faction_id: None,
                properties: self.properties.clone(),
            },
        );
        Ok(())
    }

    fn undo(&self, doc: &mut MapDocument) -> Result<(), String> {
        let key = self.coord.to_tile_key();
        doc.entities.placements.remove(&key);
        Ok(())
    }
}

/// RemoveEntity 命令：移除实体
pub struct RemoveEntity {
    pub coord: HexCoord,
    pub removed: Mutex<Option<EntityPlacement>>,
}

impl RemoveEntity {
    pub fn new(coord: HexCoord) -> Self {
        Self {
            coord,
            removed: Mutex::new(None),
        }
    }
}

impl EditorCommand for RemoveEntity {
    fn execute(&self, doc: &mut MapDocument) -> Result<(), String> {
        let key = self.coord.to_tile_key();
        let removed = doc.entities.placements.remove(&key);
        *self.removed.lock().unwrap() = removed;
        Ok(())
    }

    fn undo(&self, doc: &mut MapDocument) -> Result<(), String> {
        if let Some(placement) = self.removed.lock().unwrap().as_ref() {
            let key = self.coord.to_tile_key();
            doc.entities.placements.insert(key, placement.clone());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validate::check_entity_overlap;

    fn create_test_doc() -> MapDocument {
        MapDocument {
            meta: MapMeta {
                name: "测试".to_string(),
                seed: 42,
                width: 32,
                height: 32,
                preset_name: None,
            },
            terrain: TerrainLayer {
                rle_data: vec![("terrain_plains".to_string(), 1024)],
                total_tiles: 1024,
            },
            resources: ResourceLayer {
                entries: BTreeMap::new(),
            },
            entities: EntityLayer {
                placements: BTreeMap::new(),
            },
            rules: RuleLayer {
                zones: vec![],
                triggers: vec![],
            },
            rivers: Default::default(),
        }
    }

    #[test]
    fn test_place_entity() {
        let mut doc = create_test_doc();
        let cmd = PlaceEntity {
            coord: HexCoord::new(5, 5),
            entity_type: "city".to_string(),
            properties: BTreeMap::new(),
        };

        cmd.execute(&mut doc).unwrap();
        assert!(doc
            .entities
            .placements
            .contains_key(&HexCoord::new(5, 5).to_tile_key()));

        cmd.undo(&mut doc).unwrap();
        assert!(!doc
            .entities
            .placements
            .contains_key(&HexCoord::new(5, 5).to_tile_key()));
    }

    #[test]
    fn test_remove_entity() {
        let mut doc = create_test_doc();
        let key = HexCoord::new(3, 3).to_tile_key();
        doc.entities.placements.insert(
            key,
            EntityPlacement {
                entity_type: "fortress".to_string(),
                faction_id: None,
                properties: BTreeMap::new(),
            },
        );

        let cmd = RemoveEntity::new(HexCoord::new(3, 3));
        cmd.execute(&mut doc).unwrap();
        assert!(!doc.entities.placements.contains_key(&key));

        cmd.undo(&mut doc).unwrap();
        assert!(doc.entities.placements.contains_key(&key));
    }

    #[test]
    fn test_command_history_undo_redo() {
        let mut doc = create_test_doc();
        let mut history = CommandHistory::new(200);

        let cmd = PlaceEntity {
            coord: HexCoord::new(5, 5),
            entity_type: "city".to_string(),
            properties: BTreeMap::new(),
        };

        history.execute(Box::new(cmd), &mut doc).unwrap();
        assert!(doc
            .entities
            .placements
            .contains_key(&HexCoord::new(5, 5).to_tile_key()));

        history.undo(&mut doc).unwrap();
        assert!(!doc
            .entities
            .placements
            .contains_key(&HexCoord::new(5, 5).to_tile_key()));

        history.redo(&mut doc).unwrap();
        assert!(doc
            .entities
            .placements
            .contains_key(&HexCoord::new(5, 5).to_tile_key()));
    }

    #[test]
    fn test_validate_no_overlap() {
        let doc = create_test_doc();
        let errors = check_entity_overlap(&doc);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_validate_for_save() {
        use crate::validate::validate_for_save;
        let doc = create_test_doc();
        let result = validate_for_save(&doc);
        assert!(result.is_valid());
    }

    #[test]
    fn test_flood_fill_compute() {
        let doc = create_test_doc();
        let fill = FloodFill::new(
            HexCoord::new(0, 0),
            "terrain_plains".to_string(),
            "terrain_forest".to_string(),
        );
        // 全部是平原，应该填充大量格子（受边界限制）
        fill.compute_fill(&doc, 32, 32);
        let affected = fill.affected.lock().unwrap();
        assert!(!affected.is_empty());
        // 起始点应该在受影响列表中
        assert!(affected.contains(&HexCoord::new(0, 0)));
    }

    // M9.2: PaintBrush 真改 RLE 数据 (替换之前的"只追加"实现)

    fn make_rle_doc() -> MapDocument {
        MapDocument {
            meta: MapMeta {
                name: "Test".to_string(),
                seed: 1,
                width: 32,
                height: 32,
                preset_name: None,
            },
            terrain: TerrainLayer {
                rle_data: vec![("terrain_plains".to_string(), 1024)],
                total_tiles: 1024,
            },
            resources: ResourceLayer { entries: BTreeMap::new() },
            entities: EntityLayer { placements: BTreeMap::new() },
            rules: RuleLayer { zones: vec![], triggers: vec![] },
            rivers: Default::default(),
        }
    }

    #[test]
    fn test_set_terrain_at_creates_run() {
        // 全平原 -> 第 5 格改森林
        // 应该拆成: (plains, 5) (forest, 1) (plains, 1018)
        let mut doc = make_rle_doc();
        set_terrain_at(&mut doc, 5, &"terrain_forest".to_string());
        assert_eq!(doc.terrain.rle_data.len(), 3);
        assert_eq!(doc.terrain.rle_data[0], ("terrain_plains".to_string(), 5));
        assert_eq!(doc.terrain.rle_data[1], ("terrain_forest".to_string(), 1));
        assert_eq!(doc.terrain.rle_data[2], ("terrain_plains".to_string(), 1018));
    }

    #[test]
    fn test_set_terrain_at_run_start() {
        // idx == run start (idx 0)
        // 全 plains -> (forest, 1) (plains, 1023)
        let mut doc = make_rle_doc();
        set_terrain_at(&mut doc, 0, &"terrain_forest".to_string());
        assert_eq!(doc.terrain.rle_data.len(), 2);
        assert_eq!(doc.terrain.rle_data[0], ("terrain_forest".to_string(), 1));
        assert_eq!(doc.terrain.rle_data[1], ("terrain_plains".to_string(), 1023));
    }

    #[test]
    fn test_set_terrain_at_run_end() {
        // idx == run end (idx 1023, 最后一个)
        // 全 plains -> (plains, 1023) (forest, 1)
        let mut doc = make_rle_doc();
        set_terrain_at(&mut doc, 1023, &"terrain_forest".to_string());
        assert_eq!(doc.terrain.rle_data.len(), 2);
        assert_eq!(doc.terrain.rle_data[0], ("terrain_plains".to_string(), 1023));
        assert_eq!(doc.terrain.rle_data[1], ("terrain_forest".to_string(), 1));
    }

    #[test]
    fn test_set_terrain_at_middle() {
        // idx = 500 (全 1024 平原中间)
        // (plains, 500) (forest, 1) (plains, 523)
        let mut doc = make_rle_doc();
        set_terrain_at(&mut doc, 500, &"terrain_forest".to_string());
        assert_eq!(doc.terrain.rle_data.len(), 3);
        assert_eq!(doc.terrain.rle_data[0], ("terrain_plains".to_string(), 500));
        assert_eq!(doc.terrain.rle_data[1], ("terrain_forest".to_string(), 1));
        assert_eq!(doc.terrain.rle_data[2], ("terrain_plains".to_string(), 523));
    }

    #[test]
    fn test_set_terrain_at_same_terrain_noop() {
        // idx 是 plains, 改成 plains → 不变
        let mut doc = make_rle_doc();
        let rle_before = doc.terrain.rle_data.clone();
        set_terrain_at(&mut doc, 5, &"terrain_plains".to_string());
        assert_eq!(doc.terrain.rle_data, rle_before);
    }

    #[test]
    fn test_set_terrain_at_multi_run() {
        // 3 个 run: (plains, 100) (forest, 50) (plains, 100)
        // idx 125 (forest run 中, run_idx=25) -> 改成 mountain
        // 拆 forest run 前后段, 中间塞 mountain, 前后 forest 不相邻不合并
        // (plains, 100) (forest, 25) (mountain, 1) (forest, 24) (plains, 100) = 5 runs
        let mut doc = make_rle_doc();
        doc.terrain.rle_data = vec![
            ("terrain_plains".to_string(), 100),
            ("terrain_forest".to_string(), 50),
            ("terrain_plains".to_string(), 100),
        ];
        set_terrain_at(&mut doc, 125, &"terrain_mountain".to_string());
        assert_eq!(doc.terrain.rle_data.len(), 5);
        assert_eq!(doc.terrain.rle_data[0], ("terrain_plains".to_string(), 100));
        assert_eq!(doc.terrain.rle_data[1], ("terrain_forest".to_string(), 25));
        assert_eq!(doc.terrain.rle_data[2], ("terrain_mountain".to_string(), 1));
        assert_eq!(doc.terrain.rle_data[3], ("terrain_forest".to_string(), 24));
        assert_eq!(doc.terrain.rle_data[4], ("terrain_plains".to_string(), 100));
    }

    #[test]
    fn test_paint_brush_execute_undo_roundtrip() {
        // 第 5 格 plains -> forest, 然后 undo 回去
        let mut doc = make_rle_doc();
        let cmd = PaintBrush::new(HexCoord::new(5, 0), "terrain_forest".to_string());
        cmd.execute(&mut doc).unwrap();
        assert_eq!(get_terrain_at(&doc, 5), "terrain_forest");
        // 周围 4 格仍 plains
        assert_eq!(get_terrain_at(&doc, 4), "terrain_plains");
        assert_eq!(get_terrain_at(&doc, 6), "terrain_plains");
        // undo 回去
        cmd.undo(&mut doc).unwrap();
        assert_eq!(get_terrain_at(&doc, 5), "terrain_plains");
        // rle 应该还原 (1 个 run)
        assert_eq!(doc.terrain.rle_data.len(), 1);
        assert_eq!(doc.terrain.rle_data[0], ("terrain_plains".to_string(), 1024));
    }

    #[test]
    fn test_paint_brush_noop_when_same_terrain() {
        let mut doc = make_rle_doc();
        let cmd = PaintBrush::new(HexCoord::new(5, 0), "terrain_plains".to_string());
        cmd.execute(&mut doc).unwrap();
        // 没变化, history 不应 push
        assert!(cmd.old_terrain.lock().unwrap().is_none());
        assert_eq!(doc.terrain.rle_data.len(), 1);
    }

    #[test]
    fn test_coord_to_rle_idx() {
        // 32x32, (5, 0) -> 5, (0, 1) -> 32, (5, 1) -> 37
        assert_eq!(coord_to_rle_idx(HexCoord::new(5, 0), 32), 5);
        assert_eq!(coord_to_rle_idx(HexCoord::new(0, 1), 32), 32);
        assert_eq!(coord_to_rle_idx(HexCoord::new(5, 1), 32), 37);
    }
}
