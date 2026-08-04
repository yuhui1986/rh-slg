//! 新游戏设置界面

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};
use serde::{Deserialize, Serialize};

/// 新游戏设置状态
#[derive(Resource, Default)]
pub struct NewGameState {
    pub show: bool,
    pub step: NewGameStep,
    pub config: GameSetupConfig,
}

/// 新游戏步骤
#[derive(Debug, Default, PartialEq, Eq)]
pub enum NewGameStep {
    #[default]
    SelectScenario,
    CustomizeFaction,
    SelectDifficulty,
    Confirm,
}

/// 游戏设置配置
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GameSetupConfig {
    pub scenario_id: String,
    pub player_faction_name: String,
    pub player_lord_name: String,
    pub difficulty: Difficulty,
    pub player_color: [f32; 3],
}

/// 难度等级
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Difficulty {
    Easy,
    #[default]
    Normal,
    Hard,
    Nightmare,
}

impl Difficulty {
    pub fn name(&self) -> &str {
        match self {
            Difficulty::Easy => "简单",
            Difficulty::Normal => "普通",
            Difficulty::Hard => "困难",
            Difficulty::Nightmare => "噩梦",
        }
    }

    pub fn description(&self) -> &str {
        match self {
            Difficulty::Easy => "AI 决策间隔 x1.5，资源 x0.8",
            Difficulty::Normal => "标准难度",
            Difficulty::Hard => "AI 决策间隔 x0.8，资源 x1.2",
            Difficulty::Nightmare => "AI 决策间隔 x0.6，资源 x1.5",
        }
    }

    pub fn all() -> Vec<Difficulty> {
        vec![
            Difficulty::Easy,
            Difficulty::Normal,
            Difficulty::Hard,
            Difficulty::Nightmare,
        ]
    }
}

/// 新游戏动作事件
///
/// 由新游戏 UI 发出，由 slg-app 监听并切换 GamePhase。
#[derive(Event)]
pub enum NewGameAction {
    /// 开始游戏（附带完整配置）
    StartGame(GameSetupConfig),
    /// 返回主菜单
    BackToMenu,
}

/// 渲染新游戏设置界面
pub fn render_new_game(
    mut contexts: EguiContexts,
    mut state: ResMut<NewGameState>,
    mut action_events: EventWriter<NewGameAction>,
) {
    if !state.show {
        return;
    }

    let ctx = contexts.ctx_mut();

    egui::CentralPanel::default().show(ctx, |ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(50.0);
            ui.heading("新游戏");
            ui.add_space(20.0);

            match state.step {
                NewGameStep::SelectScenario => render_scenario_selection(ui, &mut state),
                NewGameStep::CustomizeFaction => render_faction_customization(ui, &mut state),
                NewGameStep::SelectDifficulty => render_difficulty_selection(ui, &mut state),
                NewGameStep::Confirm => render_confirmation(ui, &mut state, &mut action_events),
            }
        });
    });
}

/// 渲染剧本选择
fn render_scenario_selection(ui: &mut egui::Ui, state: &mut NewGameState) {
    ui.label("选择剧本：");
    ui.add_space(10.0);

    // 示例剧本列表
    let scenarios = vec![
        ("sanguo_dl", "三国鼎立", "魏蜀吴三分天下，群雄逐鹿"),
        ("sandbox", "沙盒模式", "自由探索，无固定目标"),
    ];

    for (id, name, desc) in &scenarios {
        let selected = state.config.scenario_id == *id;
        if ui
            .selectable_label(selected, format!("{}: {}", name, desc))
            .clicked()
        {
            state.config.scenario_id = id.to_string();
        }
    }

    ui.add_space(20.0);

    if !state.config.scenario_id.is_empty() && ui.button("下一步").clicked() {
        state.step = NewGameStep::CustomizeFaction;
    }
}

/// 渲染势力自定义
fn render_faction_customization(ui: &mut egui::Ui, state: &mut NewGameState) {
    ui.label("自定义势力：");
    ui.add_space(10.0);

    ui.horizontal(|ui| {
        ui.label("势力名称：");
        ui.text_edit_singleline(&mut state.config.player_faction_name);
    });

    ui.horizontal(|ui| {
        ui.label("君主名：");
        ui.text_edit_singleline(&mut state.config.player_lord_name);
    });

    ui.add_space(20.0);

    ui.horizontal(|ui| {
        if ui.button("上一步").clicked() {
            state.step = NewGameStep::SelectScenario;
        }
        if ui.button("下一步").clicked() {
            state.step = NewGameStep::SelectDifficulty;
        }
    });
}

/// 渲染难度选择
fn render_difficulty_selection(ui: &mut egui::Ui, state: &mut NewGameState) {
    ui.label("选择难度：");
    ui.add_space(10.0);

    for difficulty in Difficulty::all() {
        let selected = state.config.difficulty == difficulty;
        if ui
            .selectable_label(
                selected,
                format!("{}: {}", difficulty.name(), difficulty.description()),
            )
            .clicked()
        {
            state.config.difficulty = difficulty;
        }
    }

    ui.add_space(20.0);

    ui.horizontal(|ui| {
        if ui.button("上一步").clicked() {
            state.step = NewGameStep::CustomizeFaction;
        }
        if ui.button("下一步").clicked() {
            state.step = NewGameStep::Confirm;
        }
    });
}

/// 渲染确认面板
fn render_confirmation(
    ui: &mut egui::Ui,
    state: &mut NewGameState,
    action_events: &mut EventWriter<NewGameAction>,
) {
    ui.label("确认设置：");
    ui.add_space(10.0);

    ui.label(format!("剧本: {}", state.config.scenario_id));
    ui.label(format!("势力: {}", state.config.player_faction_name));
    ui.label(format!("君主: {}", state.config.player_lord_name));
    ui.label(format!("难度: {}", state.config.difficulty.name()));

    ui.add_space(20.0);

    ui.horizontal(|ui| {
        if ui.button("上一步").clicked() {
            state.step = NewGameStep::SelectDifficulty;
        }
        if ui.button("开始游戏").clicked() {
            action_events.send(NewGameAction::StartGame(state.config.clone()));
            state.show = false;
        }
    });

    if ui.button("返回主菜单").clicked() {
        action_events.send(NewGameAction::BackToMenu);
        state.show = false;
    }
}
