//! 编辑器命令模式：EditorCommand trait + CommandHistory 撤销重做

use slg_core::map::grid::HexCoord;
use slg_data::map_doc::*;

/// 编辑器命令 trait
pub trait EditorCommand {
    /// 执行命令
    fn execute(&self, doc: &mut MapDocument) -> Result<(), String>;
    /// 撤销命令
    fn undo(&self, doc: &mut MapDocument) -> Result<(), String>;
    /// 合并提示（连续笔刷合并为单次 stroke）
    fn merge_hint(&self) -> Option<MergeHint> {
        None
    }
}

/// 合并提示
#[derive(Debug, Clone)]
pub struct MergeHint {
    pub tool_type: String,
    pub position: HexCoord,
}

/// 命令历史
pub struct CommandHistory {
    pub undo_stack: Vec<Box<dyn EditorCommand>>,
    pub redo_stack: Vec<Box<dyn EditorCommand>>,
    pub max_depth: usize,
}

impl std::fmt::Debug for CommandHistory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CommandHistory")
            .field("undo_depth", &self.undo_stack.len())
            .field("redo_depth", &self.redo_stack.len())
            .field("max_depth", &self.max_depth)
            .finish()
    }
}

impl CommandHistory {
    pub fn new(max_depth: usize) -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            max_depth,
        }
    }

    /// 执行命令并压入 undo 栈
    pub fn execute(
        &mut self,
        cmd: Box<dyn EditorCommand>,
        doc: &mut MapDocument,
    ) -> Result<(), String> {
        cmd.execute(doc)?;

        // 检查是否可以合并
        if let (Some(_last), Some(_hint)) = (self.undo_stack.last(), cmd.merge_hint()) {
            // 如果是同一工具同一位置，合并
            // 简化实现：直接压入
        }

        self.undo_stack.push(cmd);
        self.redo_stack.clear(); // 新操作清空 redo 栈

        // 限制深度
        if self.undo_stack.len() > self.max_depth {
            self.undo_stack.remove(0);
        }

        Ok(())
    }

    /// 撤销
    pub fn undo(&mut self, doc: &mut MapDocument) -> Result<(), String> {
        if let Some(cmd) = self.undo_stack.pop() {
            cmd.undo(doc)?;
            self.redo_stack.push(cmd);
            Ok(())
        } else {
            Err("没有可撤销的操作".to_string())
        }
    }

    /// 重做
    pub fn redo(&mut self, doc: &mut MapDocument) -> Result<(), String> {
        if let Some(cmd) = self.redo_stack.pop() {
            cmd.execute(doc)?;
            self.undo_stack.push(cmd);
            Ok(())
        } else {
            Err("没有可重做的操作".to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool::PlaceEntity;
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
}
