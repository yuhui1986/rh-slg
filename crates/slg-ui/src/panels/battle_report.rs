//! 战报面板

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

/// 战报条目
#[derive(Debug, Clone)]
pub struct BattleReportEntry {
    pub tick: u64,
    pub attacker: String,
    pub defender: String,
    pub winner: String,
    pub attacker_losses: u32,
    pub defender_losses: u32,
}

/// 战报状态
#[derive(Resource, Default)]
pub struct BattleReportState {
    pub reports: Vec<BattleReportEntry>,
    pub show: bool,
}

/// 渲染战报面板
pub fn render_battle_report(mut contexts: EguiContexts, mut state: ResMut<BattleReportState>) {
    if !state.show {
        return;
    }

    let ctx = contexts.ctx_mut();

    egui::Window::new("战报")
        .default_size([300.0, 400.0])
        .show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                for report in state.reports.iter().rev().take(20) {
                    ui.group(|ui| {
                        ui.label(format!(
                            "Tick {}: {} vs {}",
                            report.tick, report.attacker, report.defender
                        ));
                        ui.label(format!(
                            "胜者: {} | 损失: {}/{}",
                            report.winner, report.attacker_losses, report.defender_losses
                        ));
                    });
                    ui.separator();
                }
            });

            if ui.button("关闭").clicked() {
                state.show = false;
            }
        });
}
