//! 顶部资源栏：显示资源/速度/tick

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

/// 顶部栏组件
#[derive(Resource, Default)]
pub struct TopBarState {
    pub gold: u64,
    pub food: u64,
    pub wood: u64,
    pub iron: u64,
    pub stone: u64,
    pub tick: u64,
    pub speed: String,
    pub show: bool,
    /// 玩家当前行军中的队伍数
    pub marching_count: u32,
    /// M10.2: 状态消息 (派兵/操作反馈, 1.5s 自动清空)
    pub status_message: String,
}

/// 渲染顶部资源栏（仅在游戏进行中显示）
pub fn render_top_bar(mut contexts: EguiContexts, state: Res<TopBarState>) {
    if !state.show {
        return;
    }

    let ctx = contexts.ctx_mut();

    egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.label(format!("\u{1f4b0} {}", state.gold));
            ui.separator();
            ui.label(format!("\u{1f33e} {}", state.food));
            ui.separator();
            ui.label(format!("\u{1fa93} {}", state.wood));
            ui.separator();
            ui.label(format!("\u{26cf} {}", state.iron));
            ui.separator();
            ui.label(format!("\u{1faa8} {}", state.stone));
            ui.separator();
            ui.label(format!("\u{23f1} Tick: {}", state.tick));
            ui.separator();
            ui.label(format!("\u{23e9} {}", state.speed));
            ui.separator();
            ui.label(format!("\u{1f3c3} 行军: {}", state.marching_count));
        });
        // M10.2: 状态消息单独一行 (派兵反馈等)
        if !state.status_message.is_empty() {
            ui.label(&state.status_message);
        }
    });
}
