//! M9.1: 编辑器工具栏 UI
//!
//! Editor phase 时显示:
//! - 左侧: 4 个 tool 按钮 (Paint / FloodFill / PlaceEntity / RemoveEntity)
//! - 笔刷地形选择 (4 种常见地形)
//! - 实体类型选择 (city / fortress / pass)
//! - 右侧: Undo / Redo / Save / New / Open 按钮
//! - 底部: status message (save/validate 反馈)
//!
//! 点击按钮 → 发 `EditorAction` event → `slg-editor::handle_editor_action` 处理

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};
use slg_editor::editor_state::{EditorAction, EditorState, EditorTool};

/// 编辑器工具栏 (顶部 + 状态栏)
pub fn render_editor_toolbar(
    mut contexts: EguiContexts,
    editor_state: Res<EditorState>,
    mut action_events: EventWriter<EditorAction>,
) {
    let ctx = contexts.ctx_mut();

    egui::TopBottomPanel::top("editor_toolbar").show(ctx, |ui| {
        ui.horizontal(|ui| {
            // ---- 工具按钮 ----
            ui.label("🛠");
            for tool in [
                EditorTool::Paint,
                EditorTool::FloodFill,
                EditorTool::PlaceEntity,
                EditorTool::RemoveEntity,
            ] {
                let selected = editor_state.current_tool == tool;
                let label = if selected {
                    format!("[{}]", tool.display_name())
                } else {
                    tool.display_name().to_string()
                };
                if ui.button(label).clicked() {
                    action_events.send(EditorAction::SetTool(tool));
                }
            }

            ui.separator();

            // ---- 笔刷地形选择 ----
            ui.label("笔刷:");
            for terrain in ["terrain_plains", "terrain_forest", "terrain_mountain", "terrain_water"] {
                let selected = editor_state.brush_terrain == terrain;
                let label = if selected {
                    format!("[{}]", terrain_short(terrain))
                } else {
                    terrain_short(terrain).to_string()
                };
                if ui.button(label).clicked() {
                    action_events.send(EditorAction::SetBrushTerrain(terrain.to_string()));
                }
            }

            ui.separator();

            // ---- 实体类型 ----
            ui.label("实体:");
            for etype in ["city", "fortress", "pass"] {
                let selected = editor_state.entity_type == etype;
                if ui
                    .add(
                        egui::SelectableLabel::new(
                            selected,
                            if selected {
                                format!("[{}]", etype)
                            } else {
                                etype.to_string()
                            },
                        ),
                    )
                    .clicked()
                {
                    action_events.send(EditorAction::SetEntityType(etype.to_string()));
                }
            }

            ui.separator();

            // ---- 撤销/重做 ----
            let can_undo = !editor_state.history.undo_stack.is_empty();
            let can_redo = !editor_state.history.redo_stack.is_empty();
            ui.add_enabled_ui(can_undo, |ui| {
                if ui.button("↶ Undo").clicked() {
                    action_events.send(EditorAction::Undo);
                }
            });
            ui.add_enabled_ui(can_redo, |ui| {
                if ui.button("↷ Redo").clicked() {
                    action_events.send(EditorAction::Redo);
                }
            });

            ui.separator();

            // ---- 保存 / 新建 ----
            if ui.button("💾 Save").clicked() {
                action_events.send(EditorAction::Save);
            }
            if ui.button("📄 New").clicked() {
                action_events.send(EditorAction::NewMap);
            }
        });
    });

    // ---- 状态栏 (底部) ----
    if !editor_state.status_message.is_empty() {
        egui::TopBottomPanel::bottom("editor_status").show(ctx, |ui| {
            ui.label(&editor_state.status_message);
        });
    }
}

fn terrain_short(terrain: &str) -> &str {
    match terrain {
        "terrain_plains" => "平原",
        "terrain_forest" => "森林",
        "terrain_mountain" => "山地",
        "terrain_water" => "水域",
        _ => terrain,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_terrain_short_chinese_names() {
        assert_eq!(terrain_short("terrain_plains"), "平原");
        assert_eq!(terrain_short("terrain_forest"), "森林");
        assert_eq!(terrain_short("terrain_mountain"), "山地");
        assert_eq!(terrain_short("terrain_water"), "水域");
    }
}
