//! slg-editor: 《天下策》地图编辑器
//!
//! 负责编辑器工具集（笔刷/填充/印章/选择）、命令模式撤销重做、分级实时校验。
//! 编辑器是游戏视图的超集，复用 slg-engine 的渲染管线。

use bevy::prelude::*;

pub mod command;
pub mod editor_state;
pub mod gallery;
pub mod layer_panel;
pub mod rule_editor;
pub mod scenario_editor;
pub mod tool;
pub mod validate;

/// 《天下策》编辑器插件
///
/// M0 阶段为空壳，M1 填充实际编辑器工具。
pub struct SlgEditorPlugin;

impl Plugin for SlgEditorPlugin {
    fn build(&self, app: &mut App) {
        // M2: 注册规则编辑器
        app.init_resource::<rule_editor::RuleEditorState>()
            .add_systems(Update, rule_editor::render_rule_editor);

        // M2-T08: 注册图层管理面板
        app.init_resource::<layer_panel::LayerManager>()
            .add_systems(Update, layer_panel::render_layer_panel);

        // M2-T12: 注册地图画廊
        app.init_resource::<gallery::MapGallery>();

        // M3-T12: 注册剧本编辑器
        app.init_resource::<scenario_editor::ScenarioEditorState>()
            .add_systems(Update, scenario_editor::render_scenario_editor);

        // M9.1: 编辑器运行时状态 + 工具 dispatch + Undo/Redo + Save
        app.init_resource::<editor_state::EditorState>()
            .add_event::<editor_state::EditorAction>()
            .add_systems(
                Update,
                (
                    editor_state::dispatch_editor_tool,
                    editor_state::handle_editor_action,
                ),
            );
    }
}
