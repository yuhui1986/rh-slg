//! 河流编辑工具：绘制、擦除、渡口标记

use crate::command::*;
use slg_core::map::grid::HexCoord;
use slg_data::map_doc::*;
use std::sync::Mutex;

// ---------------------------------------------------------------------------
// RiverPaint 命令：在指定坐标绘制河流
// ---------------------------------------------------------------------------

/// 绘制河流命令
///
/// 在给定坐标处创建或替换河流段，支持撤销。
pub struct RiverPaint {
    pub coord: HexCoord,
    pub width: u8,
    pub old_segment: Mutex<Option<RiverSegment>>,
}

impl RiverPaint {
    pub fn new(coord: HexCoord, width: u8) -> Self {
        Self {
            coord,
            width,
            old_segment: Mutex::new(None),
        }
    }
}

impl EditorCommand for RiverPaint {
    fn execute(&self, doc: &mut MapDocument) -> Result<(), String> {
        if self.width == 0 || self.width > 3 {
            return Err("河流宽度必须为 1-3".to_string());
        }

        let key = self.coord.to_tile_key();

        // 保存旧数据（用于撤销）
        let old = doc.rivers.segments.get(&key).cloned();
        *self.old_segment.lock().unwrap() = old;

        doc.rivers.segments.insert(
            key,
            RiverSegment {
                width: self.width,
                is_ford: false,
                direction: None,
            },
        );
        Ok(())
    }

    fn undo(&self, doc: &mut MapDocument) -> Result<(), String> {
        let key = self.coord.to_tile_key();
        if let Some(ref old) = *self.old_segment.lock().unwrap() {
            doc.rivers.segments.insert(key, old.clone());
        } else {
            doc.rivers.segments.remove(&key);
        }
        Ok(())
    }

    fn merge_hint(&self) -> Option<MergeHint> {
        Some(MergeHint {
            tool_type: "river_paint".to_string(),
            position: self.coord,
        })
    }
}

// ---------------------------------------------------------------------------
// RiverErase 命令：擦除指定坐标的河流
// ---------------------------------------------------------------------------

/// 擦除河流命令
pub struct RiverErase {
    pub coord: HexCoord,
    pub removed: Mutex<Option<RiverSegment>>,
}

impl RiverErase {
    pub fn new(coord: HexCoord) -> Self {
        Self {
            coord,
            removed: Mutex::new(None),
        }
    }
}

impl EditorCommand for RiverErase {
    fn execute(&self, doc: &mut MapDocument) -> Result<(), String> {
        let key = self.coord.to_tile_key();
        let removed = doc.rivers.segments.remove(&key);
        *self.removed.lock().unwrap() = removed;
        Ok(())
    }

    fn undo(&self, doc: &mut MapDocument) -> Result<(), String> {
        if let Some(ref segment) = *self.removed.lock().unwrap() {
            let key = self.coord.to_tile_key();
            doc.rivers.segments.insert(key, segment.clone());
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// FordMark 命令：标记/取消渡口
// ---------------------------------------------------------------------------

/// 标记渡口命令
///
/// 将已有河流段标记为渡口，或取消渡口标记。
pub struct FordMark {
    pub coord: HexCoord,
    pub set_ford: bool,
    pub was_ford: Mutex<bool>,
}

impl FordMark {
    pub fn new(coord: HexCoord, set_ford: bool) -> Self {
        Self {
            coord,
            set_ford,
            was_ford: Mutex::new(false),
        }
    }
}

impl EditorCommand for FordMark {
    fn execute(&self, doc: &mut MapDocument) -> Result<(), String> {
        let key = self.coord.to_tile_key();
        let segment = doc
            .rivers
            .segments
            .get_mut(&key)
            .ok_or_else(|| format!("位置 {:?} 没有河流，无法标记渡口", self.coord))?;

        // 保存旧值
        *self.was_ford.lock().unwrap() = segment.is_ford;

        segment.is_ford = self.set_ford;
        Ok(())
    }

    fn undo(&self, doc: &mut MapDocument) -> Result<(), String> {
        let key = self.coord.to_tile_key();
        if let Some(segment) = doc.rivers.segments.get_mut(&key) {
            segment.is_ford = *self.was_ford.lock().unwrap();
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// RiverPaintLine 命令：沿线绘制河流（从 A 到 B）
// ---------------------------------------------------------------------------

/// 沿线绘制河流命令（使用六边形直线算法）
pub struct RiverPaintLine {
    pub from: HexCoord,
    pub to: HexCoord,
    pub width: u8,
    pub painted: Mutex<Vec<(HexCoord, Option<RiverSegment>)>>,
}

impl RiverPaintLine {
    pub fn new(from: HexCoord, to: HexCoord, width: u8) -> Self {
        Self {
            from,
            to,
            width,
            painted: Mutex::new(Vec::new()),
        }
    }
}

impl EditorCommand for RiverPaintLine {
    fn execute(&self, doc: &mut MapDocument) -> Result<(), String> {
        if self.width == 0 || self.width > 3 {
            return Err("河流宽度必须为 1-3".to_string());
        }

        let line = HexCoord::line(self.from, self.to);
        self.painted.lock().unwrap().clear();

        for coord in &line {
            let key = coord.to_tile_key();
            let old = doc.rivers.segments.get(&key).cloned();
            self.painted.lock().unwrap().push((*coord, old));
            doc.rivers.segments.insert(
                key,
                RiverSegment {
                    width: self.width,
                    is_ford: false,
                    direction: None,
                },
            );
        }
        Ok(())
    }

    fn undo(&self, doc: &mut MapDocument) -> Result<(), String> {
        for (coord, old) in self.painted.lock().unwrap().iter() {
            let key = coord.to_tile_key();
            if let Some(segment) = old {
                doc.rivers.segments.insert(key, segment.clone());
            } else {
                doc.rivers.segments.remove(&key);
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// 辅助函数
// ---------------------------------------------------------------------------

/// 获取指定坐标处的河流段（只读）
pub fn get_river_at(doc: &MapDocument, coord: HexCoord) -> Option<&RiverSegment> {
    doc.rivers.segments.get(&coord.to_tile_key())
}

/// 检查指定坐标是否有河流
pub fn has_river(doc: &MapDocument, coord: HexCoord) -> bool {
    doc.rivers.segments.contains_key(&coord.to_tile_key())
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

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
    fn test_river_paint_basic() {
        let mut doc = create_test_doc();
        let coord = HexCoord::new(5, 5);
        let cmd = RiverPaint::new(coord, 1);

        cmd.execute(&mut doc).unwrap();

        let segment = get_river_at(&doc, coord).unwrap();
        assert_eq!(segment.width, 1);
        assert!(!segment.is_ford);
        assert!(segment.direction.is_none());
    }

    #[test]
    fn test_river_paint_width_validation() {
        let mut doc = create_test_doc();
        let cmd = RiverPaint::new(HexCoord::new(5, 5), 0);
        assert!(cmd.execute(&mut doc).is_err());

        let cmd = RiverPaint::new(HexCoord::new(5, 5), 4);
        assert!(cmd.execute(&mut doc).is_err());
    }

    #[test]
    fn test_river_paint_undo() {
        let mut doc = create_test_doc();
        let coord = HexCoord::new(5, 5);
        let cmd = RiverPaint::new(coord, 2);

        cmd.execute(&mut doc).unwrap();
        assert!(has_river(&doc, coord));

        cmd.undo(&mut doc).unwrap();
        assert!(!has_river(&doc, coord));
    }

    #[test]
    fn test_river_paint_replace() {
        let mut doc = create_test_doc();
        let coord = HexCoord::new(5, 5);

        // 先绘制宽度 1
        let cmd1 = RiverPaint::new(coord, 1);
        cmd1.execute(&mut doc).unwrap();
        assert_eq!(get_river_at(&doc, coord).unwrap().width, 1);

        // 替换为宽度 3
        let cmd2 = RiverPaint::new(coord, 3);
        cmd2.execute(&mut doc).unwrap();
        assert_eq!(get_river_at(&doc, coord).unwrap().width, 3);

        // 撤销应回到宽度 1
        cmd2.undo(&mut doc).unwrap();
        assert_eq!(get_river_at(&doc, coord).unwrap().width, 1);
    }

    #[test]
    fn test_river_erase() {
        let mut doc = create_test_doc();
        let coord = HexCoord::new(5, 5);

        // 先绘制
        let paint = RiverPaint::new(coord, 1);
        paint.execute(&mut doc).unwrap();
        assert!(has_river(&doc, coord));

        // 擦除
        let erase = RiverErase::new(coord);
        erase.execute(&mut doc).unwrap();
        assert!(!has_river(&doc, coord));

        // 撤销擦除
        erase.undo(&mut doc).unwrap();
        assert!(has_river(&doc, coord));
    }

    #[test]
    fn test_ford_mark() {
        let mut doc = create_test_doc();
        let coord = HexCoord::new(5, 5);

        // 先绘制河流
        let paint = RiverPaint::new(coord, 2);
        paint.execute(&mut doc).unwrap();
        assert!(!get_river_at(&doc, coord).unwrap().is_ford);

        // 标记渡口
        let ford = FordMark::new(coord, true);
        ford.execute(&mut doc).unwrap();
        assert!(get_river_at(&doc, coord).unwrap().is_ford);

        // 撤销渡口
        ford.undo(&mut doc).unwrap();
        assert!(!get_river_at(&doc, coord).unwrap().is_ford);
    }

    #[test]
    fn test_ford_mark_no_river_error() {
        let mut doc = create_test_doc();
        let coord = HexCoord::new(5, 5);

        // 没有河流时标记渡口应报错
        let ford = FordMark::new(coord, true);
        assert!(ford.execute(&mut doc).is_err());
    }

    #[test]
    fn test_river_paint_line() {
        let mut doc = create_test_doc();
        let from = HexCoord::new(0, 0);
        let to = HexCoord::new(3, 0);

        let cmd = RiverPaintLine::new(from, to, 1);
        cmd.execute(&mut doc).unwrap();

        // 检查线段上所有格子都有河流
        let line = HexCoord::line(from, to);
        for coord in &line {
            assert!(has_river(&doc, *coord), "坐标 {:?} 应该有河流", coord);
        }

        // 撤销
        cmd.undo(&mut doc).unwrap();
        for coord in &line {
            assert!(
                !has_river(&doc, *coord),
                "撤销后坐标 {:?} 不应有河流",
                coord
            );
        }
    }

    #[test]
    fn test_command_history_with_river() {
        use crate::command::CommandHistory;

        let mut doc = create_test_doc();
        let mut history = CommandHistory::new(200);
        let coord = HexCoord::new(5, 5);

        // 绘制河流
        let cmd = RiverPaint::new(coord, 1);
        history.execute(Box::new(cmd), &mut doc).unwrap();
        assert!(has_river(&doc, coord));

        // 撤销
        history.undo(&mut doc).unwrap();
        assert!(!has_river(&doc, coord));

        // 重做
        history.redo(&mut doc).unwrap();
        assert!(has_river(&doc, coord));
    }

    #[test]
    fn test_river_data_serde_roundtrip() {
        let segment = RiverSegment {
            width: 2,
            is_ford: true,
            direction: Some(FlowDirection::East),
        };

        let json = serde_json::to_string(&segment).unwrap();
        let back: RiverSegment = serde_json::from_str(&json).unwrap();
        assert_eq!(segment, back);
    }
}
