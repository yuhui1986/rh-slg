//! 编辑器工具：Paint / FloodFill / PlaceEntity / River / Select / Stamp

pub mod river;
pub mod select;
pub mod stamp;

use crate::command::*;
use slg_core::map::grid::HexCoord;
use slg_data::ids::*;
use slg_data::map_doc::*;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// PaintBrush 命令：修改单格地形类型
pub struct PaintBrush {
    pub coord: HexCoord,
    pub new_terrain: TerrainTypeId,
    pub old_terrain: RefCell<Option<TerrainTypeId>>,
}

impl PaintBrush {
    pub fn new(coord: HexCoord, new_terrain: TerrainTypeId) -> Self {
        Self {
            coord,
            new_terrain,
            old_terrain: RefCell::new(None),
        }
    }
}

impl EditorCommand for PaintBrush {
    fn execute(&self, doc: &mut MapDocument) -> Result<(), String> {
        // 简化实现：将新地形追加到 RLE 数据末尾
        // 实际应解码 RLE、修改指定位置、重新编码
        let key = self.coord.to_tile_key();
        let idx = key as usize;
        if idx < doc.terrain.total_tiles as usize {
            // 记录旧地形（简化：从 RLE 数据推断）
            let old = get_terrain_at(doc, idx);
            *self.old_terrain.borrow_mut() = Some(old);

            // 简化：追加新地形段
            doc.terrain.rle_data.push((self.new_terrain.clone(), 1));
        }
        Ok(())
    }

    fn undo(&self, doc: &mut MapDocument) -> Result<(), String> {
        // 恢复旧地形（简化：移除末尾追加的段）
        if let Some(old) = self.old_terrain.borrow().as_ref() {
            if let Some(last) = doc.terrain.rle_data.last() {
                if last.0 == *self.new_terrain && last.1 == 1 {
                    doc.terrain.rle_data.pop();
                }
            }
            let _ = old; // 旧地形在完整实现中用于恢复原位置
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

/// 从 RLE 数据中获取指定索引处的地形类型（简化实现）
fn get_terrain_at(doc: &MapDocument, idx: usize) -> TerrainTypeId {
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

/// FloodFill 命令：泛洪填充
pub struct FloodFill {
    pub start: HexCoord,
    pub target_terrain: TerrainTypeId,
    pub fill_terrain: TerrainTypeId,
    pub affected: RefCell<Vec<HexCoord>>,
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
            affected: RefCell::new(Vec::new()),
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

            self.affected.borrow_mut().push(current);

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
        for _coord in self.affected.borrow().iter() {
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
    pub removed: RefCell<Option<EntityPlacement>>,
}

impl RemoveEntity {
    pub fn new(coord: HexCoord) -> Self {
        Self {
            coord,
            removed: RefCell::new(None),
        }
    }
}

impl EditorCommand for RemoveEntity {
    fn execute(&self, doc: &mut MapDocument) -> Result<(), String> {
        let key = self.coord.to_tile_key();
        let removed = doc.entities.placements.remove(&key);
        *self.removed.borrow_mut() = removed;
        Ok(())
    }

    fn undo(&self, doc: &mut MapDocument) -> Result<(), String> {
        if let Some(placement) = self.removed.borrow().as_ref() {
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
        let affected = fill.affected.borrow();
        assert!(!affected.is_empty());
        // 起始点应该在受影响列表中
        assert!(affected.contains(&HexCoord::new(0, 0)));
    }
}
