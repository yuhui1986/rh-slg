//! 主菜单面板

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

/// 主菜单状态
#[derive(Resource, Default)]
pub struct MainMenuState {
    pub show: bool,
}

/// 主菜单动作事件
///
/// 由主菜单 UI 发出，由 slg-app 监听并切换 GamePhase。
#[derive(Event)]
pub enum MainMenuAction {
    NewGame,
    ContinueGame,
    Editor,
    Settings,
}

/// 渲染主菜单
pub fn render_main_menu(
    mut contexts: EguiContexts,
    state: Res<MainMenuState>,
    mut action_events: EventWriter<MainMenuAction>,
) {
    if !state.show {
        return;
    }

    let ctx = contexts.ctx_mut();

    egui::CentralPanel::default().show(ctx, |ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(100.0);

            // 游戏标题
            ui.heading(egui::RichText::new("天下策").size(48.0));
            ui.add_space(20.0);
            ui.label("逐鹿天下，谋定而后动");
            ui.add_space(50.0);

            // 菜单按钮
            let button_size = egui::Vec2::new(200.0, 40.0);

            if ui
                .add_sized(button_size, egui::Button::new("新游戏"))
                .clicked()
            {
                action_events.send(MainMenuAction::NewGame);
            }

            if ui
                .add_sized(button_size, egui::Button::new("继续游戏"))
                .clicked()
            {
                action_events.send(MainMenuAction::ContinueGame);
            }

            if ui
                .add_sized(button_size, egui::Button::new("编辑器"))
                .clicked()
            {
                action_events.send(MainMenuAction::Editor);
            }

            if ui
                .add_sized(button_size, egui::Button::new("设置"))
                .clicked()
            {
                action_events.send(MainMenuAction::Settings);
            }

            ui.add_space(50.0);
            ui.label("版本 0.1.0");
        });
    });
}
