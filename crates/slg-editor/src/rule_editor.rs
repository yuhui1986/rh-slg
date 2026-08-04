//! 规则层编辑器：事件链/胜利条件/区域规则的可视化编辑

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

/// 规则编辑器状态
#[derive(Resource, Default)]
pub struct RuleEditorState {
    pub show: bool,
    pub active_tab: RuleTab,
    pub selected_chain: Option<String>,
    pub selected_zone: Option<String>,
    pub selected_victory: Option<String>,
}

/// 规则标签页
#[derive(Debug, Default, PartialEq, Eq)]
pub enum RuleTab {
    #[default]
    Events,
    Zones,
    Victory,
}

/// 渲染规则编辑器面板
pub fn render_rule_editor(mut contexts: EguiContexts, mut state: ResMut<RuleEditorState>) {
    if !state.show {
        return;
    }

    let ctx = contexts.ctx_mut();

    egui::Window::new("规则编辑器")
        .default_size([400.0, 500.0])
        .show(ctx, |ui| {
            // 标签页选择
            ui.horizontal(|ui| {
                if ui
                    .selectable_label(state.active_tab == RuleTab::Events, "事件链")
                    .clicked()
                {
                    state.active_tab = RuleTab::Events;
                }
                if ui
                    .selectable_label(state.active_tab == RuleTab::Zones, "区域规则")
                    .clicked()
                {
                    state.active_tab = RuleTab::Zones;
                }
                if ui
                    .selectable_label(state.active_tab == RuleTab::Victory, "胜利条件")
                    .clicked()
                {
                    state.active_tab = RuleTab::Victory;
                }
            });

            ui.separator();

            match state.active_tab {
                RuleTab::Events => render_events_tab(ui, &mut state),
                RuleTab::Zones => render_zones_tab(ui, &mut state),
                RuleTab::Victory => render_victory_tab(ui, &mut state),
            }

            if ui.button("关闭").clicked() {
                state.show = false;
            }
        });
}

/// 渲染事件链标签页
fn render_events_tab(ui: &mut egui::Ui, state: &mut RuleEditorState) {
    ui.heading("事件链");

    // 事件链列表
    egui::ScrollArea::vertical().show(ui, |ui| {
        let chains = vec!["开局事件", "黄巾余党", "天灾系统"];

        for chain_name in &chains {
            let selected = state.selected_chain.as_deref() == Some(chain_name);
            if ui.selectable_label(selected, *chain_name).clicked() {
                state.selected_chain = Some(chain_name.to_string());
            }
        }
    });

    ui.separator();

    // 添加新事件链
    if ui.button("+ 新建事件链").clicked() {
        // TODO: 创建新事件链
    }

    // 编辑选中的事件链
    if let Some(ref chain) = state.selected_chain {
        ui.separator();
        ui.heading(format!("编辑: {}", chain));

        ui.label("触发条件:");
        // TODO: 条件编辑器

        ui.label("效果:");
        // TODO: 效果编辑器
    }
}

/// 渲染区域规则标签页
fn render_zones_tab(ui: &mut egui::Ui, state: &mut RuleEditorState) {
    ui.heading("区域规则");

    egui::ScrollArea::vertical().show(ui, |ui| {
        let zones = vec!["富饶之地", "战区", "中立区"];

        for zone_name in &zones {
            let selected = state.selected_zone.as_deref() == Some(zone_name);
            if ui.selectable_label(selected, *zone_name).clicked() {
                state.selected_zone = Some(zone_name.to_string());
            }
        }
    });

    ui.separator();

    if ui.button("+ 新建区域").clicked() {
        // TODO: 创建新区域规则
    }

    if let Some(ref zone) = state.selected_zone {
        ui.separator();
        ui.heading(format!("编辑: {}", zone));

        ui.label("效果:");
        // TODO: 区域效果编辑器
    }
}

/// 渲染胜利条件标签页
fn render_victory_tab(ui: &mut egui::Ui, state: &mut RuleEditorState) {
    ui.heading("胜利条件");

    egui::ScrollArea::vertical().show(ui, |ui| {
        let conditions = vec!["占领洛阳", "统一全国", "存活 365 天"];

        for condition_name in &conditions {
            let selected = state.selected_victory.as_deref() == Some(condition_name);
            if ui.selectable_label(selected, *condition_name).clicked() {
                state.selected_victory = Some(condition_name.to_string());
            }
        }
    });

    ui.separator();

    if ui.button("+ 新建条件").clicked() {
        // TODO: 创建新胜利条件
    }

    if let Some(ref condition) = state.selected_victory {
        ui.separator();
        ui.heading(format!("编辑: {}", condition));

        ui.label("条件类型:");
        // TODO: 条件类型选择器
    }
}
