//! 胜利/失败画面
//!
//! 游戏结束时显示的全屏面板，展示胜利/失败信息及游戏统计数据。
//! 通过 `GameOverAction` 事件通知 `slg-app` 切换游戏阶段。

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};
use slg_core::statistics::GameStatistics;

/// 游戏结束状态
#[derive(Resource, Default)]
pub struct GameOverState {
    /// 是否显示游戏结束画面
    pub show: bool,
    /// 是否为胜利（false 表示失败）
    pub is_victory: bool,
    /// 结束原因描述
    pub reason: String,
    /// 游戏统计数据
    pub statistics: GameStatistics,
}

/// 游戏结束动作事件
///
/// 由游戏结束画面 UI 发出，由 `slg-app` 监听并切换 `GamePhase`。
#[derive(Event)]
pub enum GameOverAction {
    /// 再来一局
    NewGame,
    /// 返回主菜单
    MainMenu,
}

/// 渲染游戏结束画面
pub fn render_game_over(
    mut contexts: EguiContexts,
    state: Res<GameOverState>,
    mut action_events: EventWriter<GameOverAction>,
) {
    if !state.show {
        return;
    }

    let ctx = contexts.ctx_mut();

    egui::CentralPanel::default().show(ctx, |ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(50.0);

            // 标题
            if state.is_victory {
                ui.heading(
                    egui::RichText::new("天下归心")
                        .size(42.0)
                        .color(egui::Color32::GOLD),
                );
            } else {
                ui.heading(
                    egui::RichText::new("壮志未酬")
                        .size(42.0)
                        .color(egui::Color32::RED),
                );
            }

            ui.add_space(12.0);

            // 结束原因
            ui.label(
                egui::RichText::new(&state.reason)
                    .size(18.0)
                    .color(egui::Color32::from_gray(200)),
            );

            ui.add_space(30.0);

            // 统计数据
            ui.heading(egui::RichText::new("游戏统计").size(24.0));
            ui.add_space(10.0);

            egui::Frame::group(ui.style())
                .inner_margin(egui::Margin::symmetric(24, 16))
                .show(ui, |ui| {
                    egui::Grid::new("stats_grid")
                        .num_columns(2)
                        .spacing([40.0, 8.0])
                        .show(ui, |ui| {
                            stat_row(ui, "游戏天数", &format!("{}", state.statistics.game_days()));
                            stat_row(
                                ui,
                                "战斗次数",
                                &format!("{}", state.statistics.battles_fought),
                            );
                            stat_row(
                                ui,
                                "胜率",
                                &format!("{:.1}%", state.statistics.win_rate() * 100.0),
                            );
                            stat_row(
                                ui,
                                "占领格数",
                                &format!("{}", state.statistics.tiles_occupied),
                            );
                            stat_row(ui, "丢失格数", &format!("{}", state.statistics.tiles_lost));
                            stat_row(
                                ui,
                                "峰值领地",
                                &format!("{}", state.statistics.peak_territory),
                            );
                            stat_row(
                                ui,
                                "招募武将",
                                &format!("{}", state.statistics.generals_recruited),
                            );
                            stat_row(
                                ui,
                                "失去武将",
                                &format!("{}", state.statistics.generals_lost),
                            );
                            stat_row(
                                ui,
                                "触发事件",
                                &format!("{}", state.statistics.events_triggered),
                            );
                            stat_row(
                                ui,
                                "累计金币",
                                &format!("{}", state.statistics.total_gold_earned),
                            );
                            stat_row(
                                ui,
                                "消耗粮食",
                                &format!("{}", state.statistics.total_food_consumed),
                            );
                        });
                });

            ui.add_space(30.0);

            // 操作按钮
            let button_size = egui::Vec2::new(180.0, 40.0);

            ui.horizontal(|ui| {
                if ui
                    .add_sized(button_size, egui::Button::new("再来一局"))
                    .clicked()
                {
                    action_events.send(GameOverAction::NewGame);
                }

                if ui
                    .add_sized(button_size, egui::Button::new("返回主菜单"))
                    .clicked()
                {
                    action_events.send(GameOverAction::MainMenu);
                }
            });
        });
    });
}

/// 统计行辅助函数
fn stat_row(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.label(egui::RichText::new(label).size(16.0));
    ui.label(egui::RichText::new(value).size(16.0).strong());
    ui.end_row();
}
