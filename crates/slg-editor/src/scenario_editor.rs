//! 剧本编辑器

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};
use serde::{Deserialize, Serialize};

/// 剧本编辑器状态
#[derive(Resource, Default)]
pub struct ScenarioEditorState {
    pub show: bool,
    pub step: ScenarioStep,
    pub scenario: ScenarioConfig,
}

/// 剧本步骤
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ScenarioStep {
    #[default]
    BasicInfo,
    Factions,
    VictoryConditions,
    EventChains,
    ZoneRules,
    Preview,
}

/// 剧本配置
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScenarioConfig {
    pub id: String,
    pub name: String,
    pub description: String,
    pub map_size: (u32, u32),
    pub seed: u64,
    pub factions: Vec<FactionConfig>,
    pub victory_conditions: Vec<VictoryConfig>,
}

/// 势力配置
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FactionConfig {
    pub id: String,
    pub name: String,
    pub personality: String,
    pub main_city: (i32, i32),
    pub color: [f32; 3],
}

/// 胜利条件配置
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VictoryConfig {
    pub label: String,
    pub condition_type: String,
    pub value: String,
}

/// 渲染剧本编辑器
pub fn render_scenario_editor(mut contexts: EguiContexts, mut state: ResMut<ScenarioEditorState>) {
    if !state.show {
        return;
    }

    let ctx = contexts.ctx_mut();

    egui::Window::new("剧本编辑器")
        .default_size([500.0, 400.0])
        .show(ctx, |ui| {
            // 步骤导航
            ui.horizontal(|ui| {
                let steps = vec![
                    (ScenarioStep::BasicInfo, "基本信息"),
                    (ScenarioStep::Factions, "势力"),
                    (ScenarioStep::VictoryConditions, "胜利条件"),
                    (ScenarioStep::EventChains, "事件链"),
                    (ScenarioStep::ZoneRules, "区域规则"),
                    (ScenarioStep::Preview, "预览"),
                ];

                for (step, name) in &steps {
                    let selected = state.step == *step;
                    if ui.selectable_label(selected, *name).clicked() {
                        state.step = step.clone();
                    }
                }
            });

            ui.separator();

            match state.step {
                ScenarioStep::BasicInfo => render_basic_info(ui, &mut state),
                ScenarioStep::Factions => render_factions(ui, &mut state),
                ScenarioStep::VictoryConditions => render_victory_conditions(ui, &mut state),
                ScenarioStep::EventChains => render_event_chains(ui, &mut state),
                ScenarioStep::ZoneRules => render_zone_rules(ui, &mut state),
                ScenarioStep::Preview => render_preview(ui, &mut state),
            }

            ui.separator();

            if ui.button("保存剧本").clicked() {
                // TODO: 保存到文件
                state.show = false;
            }

            if ui.button("关闭").clicked() {
                state.show = false;
            }
        });
}

/// 渲染基本信息
fn render_basic_info(ui: &mut egui::Ui, state: &mut ScenarioEditorState) {
    ui.heading("基本信息");

    ui.horizontal(|ui| {
        ui.label("剧本 ID：");
        ui.text_edit_singleline(&mut state.scenario.id);
    });

    ui.horizontal(|ui| {
        ui.label("名称：");
        ui.text_edit_singleline(&mut state.scenario.name);
    });

    ui.horizontal(|ui| {
        ui.label("描述：");
        ui.text_edit_multiline(&mut state.scenario.description);
    });

    ui.horizontal(|ui| {
        ui.label("地图宽度：");
        ui.add(egui::DragValue::new(&mut state.scenario.map_size.0).range(64..=2048));
    });

    ui.horizontal(|ui| {
        ui.label("地图高度：");
        ui.add(egui::DragValue::new(&mut state.scenario.map_size.1).range(64..=2048));
    });

    ui.horizontal(|ui| {
        ui.label("种子：");
        ui.add(egui::DragValue::new(&mut state.scenario.seed));
    });
}

/// 渲染势力配置
fn render_factions(ui: &mut egui::Ui, state: &mut ScenarioEditorState) {
    ui.heading("势力配置");

    for (i, faction) in state.scenario.factions.iter_mut().enumerate() {
        ui.group(|ui| {
            ui.label(format!("势力 {}", i + 1));
            ui.horizontal(|ui| {
                ui.label("ID：");
                ui.text_edit_singleline(&mut faction.id);
            });
            ui.horizontal(|ui| {
                ui.label("名称：");
                ui.text_edit_singleline(&mut faction.name);
            });
        });
    }

    if ui.button("+ 添加势力").clicked() {
        state.scenario.factions.push(FactionConfig::default());
    }
}

/// 渲染胜利条件
fn render_victory_conditions(ui: &mut egui::Ui, state: &mut ScenarioEditorState) {
    ui.heading("胜利条件");

    for condition in &mut state.scenario.victory_conditions {
        ui.horizontal(|ui| {
            ui.text_edit_singleline(&mut condition.label);
            ui.text_edit_singleline(&mut condition.condition_type);
        });
    }

    if ui.button("+ 添加条件").clicked() {
        state
            .scenario
            .victory_conditions
            .push(VictoryConfig::default());
    }
}

/// 渲染事件链
fn render_event_chains(ui: &mut egui::Ui, _state: &mut ScenarioEditorState) {
    ui.heading("事件链");
    ui.label("（事件链编辑功能待实现）");
}

/// 渲染区域规则
fn render_zone_rules(ui: &mut egui::Ui, _state: &mut ScenarioEditorState) {
    ui.heading("区域规则");
    ui.label("（区域规则编辑功能待实现）");
}

/// 渲染预览
fn render_preview(ui: &mut egui::Ui, state: &mut ScenarioEditorState) {
    ui.heading("预览");

    ui.label(format!("剧本: {}", state.scenario.name));
    ui.label(format!(
        "地图: {}x{}",
        state.scenario.map_size.0, state.scenario.map_size.1
    ));
    ui.label(format!("势力: {} 个", state.scenario.factions.len()));
    ui.label(format!(
        "胜利条件: {} 个",
        state.scenario.victory_conditions.len()
    ));
}
