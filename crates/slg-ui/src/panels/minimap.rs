//! 小地图面板

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

/// 小地图状态
#[derive(Resource, Default)]
pub struct MinimapState {
    pub show: bool,
}

/// 渲染小地图
pub fn render_minimap(mut contexts: EguiContexts, state: Res<MinimapState>) {
    if !state.show {
        return;
    }

    let ctx = contexts.ctx_mut();

    egui::Window::new("小地图")
        .default_size([200.0, 200.0])
        .resizable(false)
        .show(ctx, |ui| {
            // 简化实现：显示占位矩形
            let (response, painter) =
                ui.allocate_painter(egui::Vec2::new(180.0, 180.0), egui::Sense::hover());

            // 绘制占位背景
            painter.rect_filled(response.rect, 0.0, egui::Color32::from_rgb(40, 80, 40));

            ui.label("小地图占位");
        });
}
