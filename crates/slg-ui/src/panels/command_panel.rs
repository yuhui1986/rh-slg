//! 指令面板：部队指令

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

/// 指令面板状态
#[derive(Resource, Default)]
pub struct CommandPanelState {
    pub selected_army: Option<String>,
    pub show: bool,
}

/// 渲染指令面板
pub fn render_command_panel(mut contexts: EguiContexts, state: Res<CommandPanelState>) {
    if !state.show {
        return;
    }

    let ctx = contexts.ctx_mut();

    egui::Window::new("指令")
        .default_size([200.0, 150.0])
        .show(ctx, |ui| {
            if let Some(ref army) = state.selected_army {
                ui.label(format!("选中部队: {}", army));
                ui.separator();

                if ui.button("行军").clicked() {
                    // TODO: 发送行军指令
                }
                if ui.button("征兵").clicked() {
                    // TODO: 发送征兵指令
                }
                if ui.button("驻守").clicked() {
                    // TODO: 发送驻守指令
                }
            } else {
                ui.label("未选中部队");
            }
        });
}
