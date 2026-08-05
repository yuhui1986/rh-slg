//! M9.1: 编辑器运行时状态 (MapDocument + CommandHistory + 当前 tool)
//!
//! `EditorState` 是个 Bevy Resource, 存:
//! - 当前编辑的 MapDocument (内存)
//! - CommandHistory (撤销重做栈)
//! - 当前选中的 tool
//! - 工具参数 (brush_terrain / entity_type / save_path)
//!
//! M9.1 P0+P1: 真正让编辑器能 work
//! - dispatch_editor_tool 读 HexClickEvent + current_tool 构造 EditorCommand
//! - handle_undo / handle_redo 调 CommandHistory
//! - save_editor validate + 写 .ron

use std::collections::BTreeMap;
use std::path::PathBuf;

use bevy::prelude::*;
use slg_data::ids::TerrainTypeId;
use slg_data::map_doc::{MapDocument, MapMeta, ResourceLayer, RuleLayer, TerrainLayer, EntityLayer};

use crate::command::CommandHistory;

// ---------------------------------------------------------------------------
// 当前选中的 tool
// ---------------------------------------------------------------------------

/// 编辑器工具
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorTool {
    /// 不选 tool（点 hex 无效果）
    None,
    /// 笔刷改地形
    Paint,
    /// 泛洪填充
    FloodFill,
    /// 放实体（主城/要塞/关隘）
    PlaceEntity,
    /// 删实体
    RemoveEntity,
}

impl EditorTool {
    pub fn display_name(&self) -> &'static str {
        match self {
            EditorTool::None => "—",
            EditorTool::Paint => "笔刷",
            EditorTool::FloodFill => "填充",
            EditorTool::PlaceEntity => "放实体",
            EditorTool::RemoveEntity => "删实体",
        }
    }
}

// ---------------------------------------------------------------------------
// EditorState
// ---------------------------------------------------------------------------

/// 编辑器运行时状态 (Bevy Resource)
#[derive(Resource, Debug)]
pub struct EditorState {
    /// 当前编辑的地图文档
    pub doc: MapDocument,
    /// 撤销/重做历史
    pub history: CommandHistory,
    /// 当前选中的 tool
    pub current_tool: EditorTool,
    /// 笔刷当前地形类型 (M9.1 默认 plains)
    pub brush_terrain: TerrainTypeId,
    /// 实体类型 (M9.1: city / fortress / pass)
    pub entity_type: String,
    /// 当前文件路径 (None = 未保存 / 新建)
    pub save_path: Option<PathBuf>,
    /// 状态消息 (最近一次 save / validate / error)
    pub status_message: String,
    /// M10.2: 是否显示编辑器 UI (由 slg-app 同步 GamePhase, default = false)
    /// 修复: render_editor_toolbar 之前没判断 phase, Menu/Playing 也会显示
    pub show: bool,
}

impl Default for EditorState {
    fn default() -> Self {
        // 默认 32x32 全平原, 1 个空 doc
        let doc = MapDocument {
            meta: MapMeta {
                name: "Untitled".to_string(),
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
        };
        Self {
            doc,
            history: CommandHistory::new(200),
            current_tool: EditorTool::Paint,
            brush_terrain: "terrain_plains".to_string(),
            entity_type: "city".to_string(),
            save_path: None,
            status_message: String::new(),
            show: false, // M10.2: 默认不显示, slg-app 同步 GamePhase
        }
    }
}

// ---------------------------------------------------------------------------
// Action event
// ---------------------------------------------------------------------------

/// 编辑器动作 event (UI 按钮 / 快捷键)
#[derive(Event, Debug, Clone)]
pub enum EditorAction {
    /// 切换 tool
    SetTool(EditorTool),
    /// 设置笔刷地形
    SetBrushTerrain(TerrainTypeId),
    /// 设置实体类型
    SetEntityType(String),
    /// 撤销
    Undo,
    /// 重做
    Redo,
    /// 保存
    Save,
    /// 新建空地图
    NewMap,
    /// 打开文件 (path 由 UI 解析, 这里直接传)
    OpenMap(PathBuf),
}

// ---------------------------------------------------------------------------
// 工具 dispatch: HexClickEvent -> EditorCommand
// ---------------------------------------------------------------------------

/// M9.1: 把 HexClickEvent 翻译成 EditorCommand 并 push 到 history
///
/// - 玩家点 hex → 看 EditorState.current_tool → 构造对应命令 → history.execute
///
/// M9.3: Paint tool 笔刷拖动合并 (per-frame)
/// - 同一 frame 内的多个 paint (拖动一帧多次) → 累积到 BatchPaintCommand
/// - frame 末尾 flush 一次 → 1 个 undo
/// - 跨 frame 的 paint 算新 stroke, 不合并
/// - tool 切换 / brush_terrain 切换 / Undo → handle_editor_action 强制 flush
pub fn dispatch_editor_tool(
    mut click_events: EventReader<slg_engine::camera::HexClickEvent>,
    mut editor_state: ResMut<EditorState>,
    mut stroke_state: Local<StrokeState>,
) {
    // ---- 开头: 检查是否需要 flush (tool 切到非 Paint) ----
    if editor_state.current_tool != EditorTool::Paint && stroke_state.buffer.is_some() {
        flush_stroke(&mut editor_state, &mut stroke_state);
    }

    for event in click_events.read() {
        let coord = event.coord;
        let tool = editor_state.current_tool;
        // 构造命令 (借用 brush_terrain / entity_type 一次, 之后 split borrow history + doc)
        let cmd: Box<dyn crate::command::EditorCommand> = match tool {
            EditorTool::None => continue,
            EditorTool::Paint => {
                // M9.3: 笔刷拖动合并 (per-frame)
                let brush_terrain = editor_state.brush_terrain.clone();
                let same_terrain = stroke_state.last_terrain.as_deref()
                    == Some(brush_terrain.as_str());
                if same_terrain && stroke_state.buffer.is_some() {
                    // 累积到当前 batch
                    if let Some(batch) = stroke_state.buffer.as_ref() {
                        batch.add_stroke(coord, brush_terrain.clone());
                    }
                } else {
                    // 不同 brush_terrain: flush 旧 + 开新
                    flush_stroke(&mut editor_state, &mut stroke_state);
                    let batch = crate::tool::BatchPaintCommand::new();
                    batch.add_stroke(coord, brush_terrain.clone());
                    stroke_state.buffer = Some(Box::new(batch));
                    stroke_state.last_terrain = Some(brush_terrain);
                }
                // 不构造独立 cmd, 等 frame 末尾 flush
                continue;
            }
            EditorTool::PlaceEntity => Box::new(crate::tool::PlaceEntity {
                coord,
                entity_type: editor_state.entity_type.clone(),
                properties: BTreeMap::new(),
            }),
            EditorTool::RemoveEntity => {
                Box::new(crate::tool::RemoveEntity::new(coord))
            }
            EditorTool::FloodFill => {
                // FloodFill 需要 compute_fill 先, 拿 target 地形
                let target = crate::tool::get_terrain_at(
                    &editor_state.doc,
                    coord.to_tile_key() as usize,
                );
                let fill = crate::tool::FloodFill::new(
                    coord,
                    target,
                    editor_state.brush_terrain.clone(),
                );
                fill.compute_fill(
                    &editor_state.doc,
                    editor_state.doc.meta.width,
                    editor_state.doc.meta.height,
                );
                Box::new(fill)
            }
        };
        // split borrow: history 借 mut, doc 借 mut, 互不冲突
        // ResMut<T> deref 成 &mut T, 然后 split borrow 字段
        let state = &mut *editor_state;
        let history = &mut state.history;
        let doc = &mut state.doc;
        let result = history.execute(cmd, doc);
        match result {
            Ok(()) => {
                info!(
                    "[Editor] ✅ {:?} 在 ({},{}) 成功",
                    tool, coord.q, coord.r
                );
            }
            Err(e) => {
                editor_state.status_message = format!("错误: {}", e);
                warn!("[Editor] ❌ {:?} 失败: {}", tool, e);
            }
        }
    }

    // ---- 末尾: flush 当前 stroke (任何 paint 都 commit) ----
    // 即使只有 1 个 stroke, 也作为 1 个 undo commit
    flush_stroke(&mut editor_state, &mut stroke_state);
}

/// M9.3: Paint stroke 累积状态 (跨 frame 保留, per-frame flush)
#[derive(Default)]
pub struct StrokeState {
    /// 当前累积的 batch (None = 没有)
    pub buffer: Option<Box<crate::tool::BatchPaintCommand>>,
    /// 当前累积时的 brush_terrain (用来判断下一个 paint 是否同 stroke)
    pub last_terrain: Option<String>,
}

/// M9.3: 把当前 stroke buffer 提交到 undo_stack
///
/// 只在 paint tool 且 buffer 非空时调用。
fn flush_stroke(editor_state: &mut EditorState, stroke_state: &mut StrokeState) {
    if let Some(batch) = stroke_state.buffer.take() {
        let n = batch.len();
        if n > 0 {
            let state = &mut *editor_state;
            let history = &mut state.history;
            let doc = &mut state.doc;
            if let Err(e) = history.execute(batch, doc) {
                warn!("[Editor] stroke flush 失败: {}", e);
            } else {
                info!("[Editor] ✏️ stroke flush: {} paints → 1 undo", n);
            }
        }
        stroke_state.last_terrain = None;
    }
}

// ---------------------------------------------------------------------------
// Undo / Redo handler
// ---------------------------------------------------------------------------

/// 处理 EditorAction::Undo / Redo / SetTool / SetBrushTerrain / Save
pub fn handle_editor_action(
    mut events: EventReader<EditorAction>,
    mut editor_state: ResMut<EditorState>,
    mut stroke_state: Local<StrokeState>,
) {
    for action in events.read() {
        // M9.3: 任何 action 都先 flush 当前 stroke (避免 tool 切换/Undo 后还有未 commit 的 paint)
        if stroke_state.buffer.is_some() {
            flush_stroke(&mut editor_state, &mut stroke_state);
        }
        match action {
            EditorAction::SetTool(tool) => {
                editor_state.current_tool = *tool;
                info!("[Editor] 切换 tool → {:?}", tool);
            }
            EditorAction::SetBrushTerrain(terrain) => {
                editor_state.brush_terrain = terrain.clone();
            }
            EditorAction::SetEntityType(etype) => {
                editor_state.entity_type = etype.clone();
            }
            EditorAction::Undo => {
                let state = &mut *editor_state;
                let history = &mut state.history;
                let doc = &mut state.doc;
                match history.undo(doc) {
                    Ok(()) => {
                        state.status_message = "已撤销".to_string();
                        info!("[Editor] ↶ Undo");
                    }
                    Err(e) => {
                        state.status_message = format!("撤销失败: {}", e);
                    }
                }
            }
            EditorAction::Redo => {
                let state = &mut *editor_state;
                let history = &mut state.history;
                let doc = &mut state.doc;
                match history.redo(doc) {
                    Ok(()) => {
                        state.status_message = "已重做".to_string();
                        info!("[Editor] ↷ Redo");
                    }
                    Err(e) => {
                        state.status_message = format!("重做失败: {}", e);
                    }
                }
            }
            EditorAction::Save => {
                save_editor(&mut editor_state);
            }
            EditorAction::NewMap => {
                *editor_state = EditorState::default();
                editor_state.status_message = "新建空地图".to_string();
            }
            EditorAction::OpenMap(path) => {
                match load_editor(&mut editor_state, path.clone()) {
                    Ok(()) => {
                        editor_state.status_message =
                            format!("已打开: {}", path.display());
                    }
                    Err(e) => {
                        editor_state.status_message = format!("打开失败: {}", e);
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Save / Load
// ---------------------------------------------------------------------------

/// 保存到 .ron 文件
///
/// 1. validate_for_save 检查
/// 2. 序列化 doc → ron → 写文件
pub fn save_editor(editor_state: &mut EditorState) {
    use crate::validate::validate_for_save;

    // 1. 校验
    let result = validate_for_save(&editor_state.doc);
    if !result.is_valid() {
        editor_state.status_message = format!(
            "保存失败: {} 项错误",
            result.errors.len()
        );
        for e in &result.errors {
            info!("[Editor] 校验错误: {:?}", e);
        }
        return;
    }

    // 2. 决定路径
    let path = match editor_state.save_path.clone() {
        Some(p) => p,
        None => {
            // 默认路径: ./saves/editor_<tick>.ron
            let dir = std::path::PathBuf::from("saves");
            let _ = std::fs::create_dir_all(&dir);
            dir.join(format!("editor_{}.ron", chrono_tick_stub()))
        }
    };

    // 3. 序列化
    let ron_str = match ron::to_string(&editor_state.doc) {
        Ok(s) => s,
        Err(e) => {
            editor_state.status_message = format!("序列化失败: {}", e);
            return;
        }
    };

    // 4. 写文件
    if let Err(e) = std::fs::write(&path, ron_str) {
        editor_state.status_message = format!("写文件失败: {}", e);
        return;
    }

    editor_state.save_path = Some(path.clone());
    editor_state.status_message = format!("已保存: {}", path.display());
    info!("[Editor] 💾 已保存到 {}", path.display());
}

/// 从 .ron 文件加载
pub fn load_editor(editor_state: &mut EditorState, path: PathBuf) -> Result<(), String> {
    let text = std::fs::read_to_string(&path).map_err(|e| format!("读文件失败: {}", e))?;
    let doc: MapDocument =
        ron::from_str(&text).map_err(|e| format!("解析失败: {}", e))?;
    editor_state.doc = doc;
    editor_state.history = CommandHistory::new(200);
    editor_state.save_path = Some(path);
    Ok(())
}

/// 简易 tick stub (避免引 chrono 依赖)
fn chrono_tick_stub() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool::PlaceEntity;
    use slg_core::map::grid::HexCoord;

    #[test]
    fn test_editor_state_default() {
        let state = EditorState::default();
        assert_eq!(state.current_tool, EditorTool::Paint);
        assert_eq!(state.brush_terrain, "terrain_plains");
        assert_eq!(state.entity_type, "city");
        assert!(state.save_path.is_none());
        assert_eq!(state.doc.meta.width, 32);
        assert_eq!(state.doc.terrain.total_tiles, 1024);
    }

    #[test]
    fn test_editor_state_dispatch_paint() {
        // 直接测 dispatch: HexClickEvent + Paint tool → history.execute
        let mut state = EditorState::default();
        let coord = HexCoord::new(5, 5);
        // 模拟 paint 命令执行
        let cmd = Box::new(crate::tool::PaintBrush::new(
            coord,
            "terrain_forest".to_string(),
        ));
        state.history.execute(cmd, &mut state.doc).unwrap();
        // history 应有 1 个 undo
        assert_eq!(state.history.undo_stack.len(), 1);
    }

    #[test]
    fn test_editor_state_undo_redo() {
        let mut state = EditorState::default();
        let coord = HexCoord::new(5, 5);

        // 执行 PlaceEntity
        let cmd = Box::new(PlaceEntity {
            coord,
            entity_type: "city".to_string(),
            properties: BTreeMap::new(),
        });
        state.history.execute(cmd, &mut state.doc).unwrap();
        assert!(state
            .doc
            .entities
            .placements
            .contains_key(&coord.to_tile_key()));

        // 撤销
        state.history.undo(&mut state.doc).unwrap();
        assert!(!state
            .doc
            .entities
            .placements
            .contains_key(&coord.to_tile_key()));
        assert_eq!(state.history.undo_stack.len(), 0);
        assert_eq!(state.history.redo_stack.len(), 1);

        // 重做
        state.history.redo(&mut state.doc).unwrap();
        assert!(state
            .doc
            .entities
            .placements
            .contains_key(&coord.to_tile_key()));
    }

    #[test]
    fn test_save_load_roundtrip() {
        // 写到一个临时文件, 再读回
        let dir = std::env::temp_dir();
        let path = dir.join(format!("rh_slg_editor_test_{}.ron", chrono_tick_stub()));

        // save
        let mut state = EditorState::default();
        state.doc.meta.name = "TestMap".to_string();
        let cmd = Box::new(PlaceEntity {
            coord: HexCoord::new(10, 10),
            entity_type: "fortress".to_string(),
            properties: BTreeMap::new(),
        });
        state.history.execute(cmd, &mut state.doc).unwrap();
        state.save_path = Some(path.clone());
        save_editor(&mut state);
        assert!(path.exists(), "save 应写文件");

        // load
        let mut state2 = EditorState::default();
        load_editor(&mut state2, path.clone()).unwrap();
        assert_eq!(state2.doc.meta.name, "TestMap");
        assert!(state2
            .doc
            .entities
            .placements
            .contains_key(&HexCoord::new(10, 10).to_tile_key()));

        // 清理
        let _ = std::fs::remove_file(&path);
    }
}
