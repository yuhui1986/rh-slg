//! slg-ui: 《天下策》egui HUD 面板层

pub mod panels;

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts, EguiPlugin};
use panels::*;
use slg_editor::gallery::MapGallery;

pub struct SlgUiPlugin;

impl Plugin for SlgUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(EguiPlugin)
            .init_resource::<top_bar::TopBarState>()
            .init_resource::<battle_report::BattleReportState>()
            .init_resource::<build_panel::BuildPanelState>() // M8
            .add_event::<build_panel::BuildPanelAction>()    // M8
            .init_resource::<command_panel::CommandPanelState>()
            .init_resource::<minimap::MinimapState>()
            .init_resource::<gallery::GalleryUiState>()
            .init_resource::<MapGallery>()
            .init_resource::<main_menu::MainMenuState>()
            .add_event::<main_menu::MainMenuAction>()
            .init_resource::<new_game::NewGameState>()
            .add_event::<new_game::NewGameAction>()
            .init_resource::<settings::SettingsState>()
            .init_resource::<game_over::GameOverState>()
            .add_event::<game_over::GameOverAction>()
            .add_systems(Startup, setup_chinese_font)
            .add_systems(
                Update,
                (
                    top_bar::render_top_bar,
                    battle_report::render_battle_report,
                    build_panel::render_build_panel, // M8
                    command_panel::render_command_panel,
                    minimap::render_minimap,
                    gallery::render_gallery,
                    main_menu::render_main_menu,
                    new_game::render_new_game,
                    settings::render_settings,
                    game_over::render_game_over,
                ),
            );
    }
}

/// 加载中文字体，解决 egui 默认字体不含中文的问题
fn setup_chinese_font(mut contexts: EguiContexts) {
    let ctx = contexts.ctx_mut();

    let mut fonts = egui::FontDefinitions::default();

    // 尝试加载 Windows 系统中文字体
    let font_paths = [
        // Microsoft YaHei（微软雅黑）
        r"C:\Windows\Fonts\msyh.ttc",
        r"C:\Windows\Fonts\msyhbd.ttc",
        // SimHei（黑体）
        r"C:\Windows\Fonts\simhei.ttf",
        // SimSun（宋体）
        r"C:\Windows\Fonts\simsun.ttc",
    ];

    for path in &font_paths {
        if let Ok(font_data) = std::fs::read(path) {
            fonts.font_data.insert(
                "chinese".to_owned(),
                egui::FontData::from_owned(font_data).into(),
            );
            // 将中文字体作为 proportional 和 monospace 的 fallback
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .push("chinese".to_owned());
            fonts
                .families
                .entry(egui::FontFamily::Monospace)
                .or_default()
                .push("chinese".to_owned());
            info!("已加载中文字体: {}", path);
            break;
        }
    }

    ctx.set_fonts(fonts);
}
