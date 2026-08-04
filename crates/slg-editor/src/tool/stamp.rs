//! Stamp 模板库
//!
//! 提供从选区创建模板、保存/加载模板文件、放置模板到地图的功能。

use crate::command::*;
use crate::tool::select::*;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use slg_core::map::grid::HexCoord;
use slg_data::map_doc::*;
use std::collections::BTreeMap;
use std::path::Path;

/// 模板数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StampTemplate {
    pub name: String,
    pub description: String,
    pub width: u32,
    pub height: u32,
    pub tiles: BTreeMap<(i32, i32), TileData>,
    pub entities: BTreeMap<(i32, i32), EntityPlacement>,
}

/// 模板中的格子数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TileData {
    pub terrain: String,
    pub level: u8,
    pub resource: Option<String>,
}

impl StampTemplate {
    /// 从选区创建模板
    pub fn from_selection(
        name: String,
        doc: &MapDocument,
        selection: &SelectionRegion,
        origin: HexCoord,
    ) -> Self {
        let mut tiles = BTreeMap::new();
        let mut entities = BTreeMap::new();

        for &key in &selection.tiles {
            let coord = HexCoord::from_tile_key(key);
            let offset = (coord.q - origin.q, coord.r - origin.r);

            // 从地形 RLE 数据中读取地形类型（简化实现：使用默认地形）
            let idx = key as usize;
            let terrain = if idx < doc.terrain.total_tiles as usize {
                get_terrain_at(&doc.terrain, idx)
            } else {
                "terrain_plains".to_string()
            };

            tiles.insert(
                offset,
                TileData {
                    terrain,
                    level: 1,
                    resource: None,
                },
            );

            // 检查是否有实体
            if let Some(placement) = doc.entities.placements.get(&key) {
                entities.insert(offset, placement.clone());
            }
        }

        let (width, height) = if selection.is_empty() {
            (0, 0)
        } else {
            (
                selection.bounds_max.q.abs_diff(selection.bounds_min.q) + 1,
                selection.bounds_max.r.abs_diff(selection.bounds_min.r) + 1,
            )
        };

        Self {
            name,
            description: format!("{} 格模板", tiles.len()),
            width,
            height,
            tiles,
            entities,
        }
    }

    /// 应用模板到地图
    pub fn apply_to_map(&self, doc: &mut MapDocument, target: HexCoord) {
        for ((dq, dr), tile_data) in &self.tiles {
            let coord = HexCoord::new(target.q + dq, target.r + dr);
            let key = coord.to_tile_key();
            // 简化实现：将地形追加到 RLE 数据末尾
            doc.terrain.rle_data.push((tile_data.terrain.clone(), 1));
            let _ = key;
        }

        for ((dq, dr), placement) in &self.entities {
            let coord = HexCoord::new(target.q + dq, target.r + dr);
            let key = coord.to_tile_key();
            doc.entities.placements.insert(key, placement.clone());
        }
    }
}

/// 从地形层 RLE 数据中获取指定索引处的地形类型（简化实现）
fn get_terrain_at(terrain: &TerrainLayer, idx: usize) -> String {
    let mut pos = 0;
    for (terrain_id, count) in &terrain.rle_data {
        pos += *count as usize;
        if idx < pos {
            return terrain_id.clone();
        }
    }
    "terrain_plains".to_string()
}

/// 模板库
#[derive(Resource, Default)]
pub struct StampLibrary {
    pub templates: Vec<StampTemplate>,
    pub selected_index: Option<usize>,
}

impl StampLibrary {
    /// 保存模板到文件
    pub fn save_template(&self, index: usize, path: &Path) -> Result<(), String> {
        if let Some(template) = self.templates.get(index) {
            let ron = ron::to_string(template).map_err(|e| e.to_string())?;
            std::fs::write(path, ron).map_err(|e| e.to_string())?;
            Ok(())
        } else {
            Err("模板索引无效".to_string())
        }
    }

    /// 从文件加载模板
    pub fn load_template(&mut self, path: &Path) -> Result<(), String> {
        let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let template: StampTemplate = ron::from_str(&content).map_err(|e| e.to_string())?;
        self.templates.push(template);
        Ok(())
    }

    /// 选中模板
    pub fn select(&mut self, index: usize) {
        self.selected_index = Some(index);
    }

    /// 获取选中的模板
    pub fn selected(&self) -> Option<&StampTemplate> {
        self.selected_index.and_then(|i| self.templates.get(i))
    }
}

/// Stamp 放置命令
pub struct StampPlace {
    pub template: StampTemplate,
    pub target: HexCoord,
}

impl EditorCommand for StampPlace {
    fn execute(&self, doc: &mut MapDocument) -> Result<(), String> {
        self.template.apply_to_map(doc, self.target);
        Ok(())
    }

    fn undo(&self, _doc: &mut MapDocument) -> Result<(), String> {
        // 简化实现：不支持撤销
        // 完整实现需要记录被覆盖的原始数据
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_doc() -> MapDocument {
        MapDocument {
            meta: MapMeta {
                name: "test".to_string(),
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
    fn test_stamp_template_creation() {
        let doc = create_test_doc();
        let mut selection = SelectionRegion::new();
        selection.insert(HexCoord::new(5, 5));
        selection.insert(HexCoord::new(6, 5));

        let template = StampTemplate::from_selection(
            "测试模板".to_string(),
            &doc,
            &selection,
            HexCoord::new(5, 5),
        );

        assert_eq!(template.name, "测试模板");
        assert_eq!(template.tiles.len(), 2);
        assert_eq!(template.description, "2 格模板");
    }

    #[test]
    fn test_stamp_template_with_entities() {
        let mut doc = create_test_doc();
        // 在选区中放置一个实体
        doc.entities.placements.insert(
            HexCoord::new(5, 5).to_tile_key(),
            EntityPlacement {
                entity_type: "city".to_string(),
                faction_id: None,
                properties: BTreeMap::new(),
            },
        );

        let mut selection = SelectionRegion::new();
        selection.insert(HexCoord::new(5, 5));
        selection.insert(HexCoord::new(6, 5));

        let template = StampTemplate::from_selection(
            "带实体模板".to_string(),
            &doc,
            &selection,
            HexCoord::new(5, 5),
        );

        assert_eq!(template.tiles.len(), 2);
        assert_eq!(template.entities.len(), 1);
        assert!(template.entities.contains_key(&(0, 0)));
    }

    #[test]
    fn test_stamp_template_apply() {
        let doc = create_test_doc();
        let mut selection = SelectionRegion::new();
        selection.insert(HexCoord::new(5, 5));
        selection.insert(HexCoord::new(6, 5));

        let template = StampTemplate::from_selection(
            "测试模板".to_string(),
            &doc,
            &selection,
            HexCoord::new(5, 5),
        );

        let mut target_doc = create_test_doc();
        template.apply_to_map(&mut target_doc, HexCoord::new(10, 10));

        // 验证实体被放置到目标位置
        // 原始选区中没有实体，所以目标文档中也不应该有
        assert!(target_doc.entities.placements.is_empty());
    }

    #[test]
    fn test_stamp_template_apply_with_entities() {
        let mut doc = create_test_doc();
        doc.entities.placements.insert(
            HexCoord::new(5, 5).to_tile_key(),
            EntityPlacement {
                entity_type: "city".to_string(),
                faction_id: Some("faction_wei".to_string()),
                properties: BTreeMap::new(),
            },
        );

        let mut selection = SelectionRegion::new();
        selection.insert(HexCoord::new(5, 5));

        let template = StampTemplate::from_selection(
            "城池模板".to_string(),
            &doc,
            &selection,
            HexCoord::new(5, 5),
        );

        let mut target_doc = create_test_doc();
        template.apply_to_map(&mut target_doc, HexCoord::new(20, 20));

        // 验证实体被放置到目标位置 (20, 20)
        let key = HexCoord::new(20, 20).to_tile_key();
        assert!(target_doc.entities.placements.contains_key(&key));
        let placement = target_doc.entities.placements.get(&key).unwrap();
        assert_eq!(placement.entity_type, "city");
        assert_eq!(placement.faction_id, Some("faction_wei".to_string()));
    }

    #[test]
    fn test_stamp_library() {
        let mut library = StampLibrary::default();
        assert!(library.selected().is_none());

        library.templates.push(StampTemplate {
            name: "测试".to_string(),
            description: "".to_string(),
            width: 2,
            height: 1,
            tiles: BTreeMap::new(),
            entities: BTreeMap::new(),
        });

        library.select(0);
        assert!(library.selected().is_some());
        assert_eq!(library.selected().unwrap().name, "测试");
    }

    #[test]
    fn test_stamp_library_invalid_index() {
        let mut library = StampLibrary::default();
        library.select(99);
        assert!(library.selected().is_none());
    }

    #[test]
    fn test_stamp_template_ron_roundtrip() {
        let mut tiles = BTreeMap::new();
        tiles.insert(
            (0, 0),
            TileData {
                terrain: "terrain_forest".to_string(),
                level: 2,
                resource: Some("resource_wood".to_string()),
            },
        );
        tiles.insert(
            (1, 0),
            TileData {
                terrain: "terrain_plains".to_string(),
                level: 1,
                resource: None,
            },
        );

        let template = StampTemplate {
            name: "序列化测试".to_string(),
            description: "测试 RON 序列化".to_string(),
            width: 2,
            height: 1,
            tiles,
            entities: BTreeMap::new(),
        };

        // 序列化
        let ron_str = ron::to_string(&template).unwrap();
        assert!(!ron_str.is_empty());

        // 反序列化
        let deserialized: StampTemplate = ron::from_str(&ron_str).unwrap();
        assert_eq!(deserialized.name, "序列化测试");
        assert_eq!(deserialized.tiles.len(), 2);
        assert_eq!(
            deserialized.tiles.get(&(0, 0)).unwrap().terrain,
            "terrain_forest"
        );
        assert_eq!(
            deserialized.tiles.get(&(1, 0)).unwrap().terrain,
            "terrain_plains"
        );
    }

    #[test]
    fn test_stamp_library_save_load_roundtrip() {
        let mut library = StampLibrary::default();
        library.templates.push(StampTemplate {
            name: "保存测试".to_string(),
            description: "测试文件保存加载".to_string(),
            width: 1,
            height: 1,
            tiles: BTreeMap::new(),
            entities: BTreeMap::new(),
        });

        let path = std::env::temp_dir().join("test_stamp_template.ron");
        library.save_template(0, &path).unwrap();

        let mut library2 = StampLibrary::default();
        library2.load_template(&path).unwrap();

        assert_eq!(library2.templates.len(), 1);
        assert_eq!(library2.templates[0].name, "保存测试");

        // 清理
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_stamp_place_command() {
        let mut doc = create_test_doc();
        let mut tiles = BTreeMap::new();
        tiles.insert(
            (0, 0),
            TileData {
                terrain: "terrain_forest".to_string(),
                level: 1,
                resource: None,
            },
        );

        let template = StampTemplate {
            name: "命令测试".to_string(),
            description: "".to_string(),
            width: 1,
            height: 1,
            tiles,
            entities: BTreeMap::new(),
        };

        let cmd = StampPlace {
            template,
            target: HexCoord::new(15, 15),
        };

        // 执行不应 panic
        cmd.execute(&mut doc).unwrap();

        // 撤销不应 panic（简化实现）
        cmd.undo(&mut doc).unwrap();
    }

    #[test]
    fn test_stamp_place_with_command_history() {
        let mut doc = create_test_doc();
        let mut history = CommandHistory::new(200);

        let mut tiles = BTreeMap::new();
        tiles.insert(
            (0, 0),
            TileData {
                terrain: "terrain_forest".to_string(),
                level: 1,
                resource: None,
            },
        );

        let template = StampTemplate {
            name: "历史测试".to_string(),
            description: "".to_string(),
            width: 1,
            height: 1,
            tiles,
            entities: BTreeMap::new(),
        };

        let cmd = StampPlace {
            template,
            target: HexCoord::new(10, 10),
        };

        // 通过 CommandHistory 执行
        history.execute(Box::new(cmd), &mut doc).unwrap();

        // 撤销
        history.undo(&mut doc).unwrap();

        // 重做
        history.redo(&mut doc).unwrap();
    }

    #[test]
    fn test_stamp_template_dimensions() {
        let doc = create_test_doc();
        let mut selection = SelectionRegion::new();
        selection.insert(HexCoord::new(3, 7));
        selection.insert(HexCoord::new(5, 10));

        let template = StampTemplate::from_selection(
            "尺寸测试".to_string(),
            &doc,
            &selection,
            HexCoord::new(3, 7),
        );

        // bounds_min = (3, 7), bounds_max = (5, 10)
        // width = 5 - 3 + 1 = 3, height = 10 - 7 + 1 = 4
        assert_eq!(template.width, 3);
        assert_eq!(template.height, 4);
    }

    #[test]
    fn test_stamp_template_empty_selection() {
        let doc = create_test_doc();
        let selection = SelectionRegion::new();

        let template = StampTemplate::from_selection(
            "空选区".to_string(),
            &doc,
            &selection,
            HexCoord::new(0, 0),
        );

        assert_eq!(template.tiles.len(), 0);
        assert_eq!(template.entities.len(), 0);
    }
}
