//! 设置面板

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};
use serde::{Deserialize, Serialize};

/// 设置状态
#[derive(Resource, Default)]
pub struct SettingsState {
    pub show: bool,
    pub settings: GameSettings,
    pub tab: SettingsTab,
}

/// 游戏设置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameSettings {
    pub audio: AudioSettings,
    pub video: VideoSettings,
    pub gameplay: GameplaySettings,
    pub language: String,
}

impl Default for GameSettings {
    fn default() -> Self {
        Self {
            audio: AudioSettings::default(),
            video: VideoSettings::default(),
            gameplay: GameplaySettings::default(),
            language: "zh-CN".to_string(),
        }
    }
}

/// 音频设置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioSettings {
    pub master_volume: f32,
    pub music_volume: f32,
    pub sfx_volume: f32,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            master_volume: 80.0,
            music_volume: 70.0,
            sfx_volume: 90.0,
        }
    }
}

/// 视频设置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoSettings {
    pub resolution: (u32, u32),
    pub fullscreen: bool,
    pub vsync: bool,
}

impl Default for VideoSettings {
    fn default() -> Self {
        Self {
            resolution: (1280, 720),
            fullscreen: false,
            vsync: true,
        }
    }
}

/// 游戏玩法设置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameplaySettings {
    pub default_speed: u8,
    pub auto_save_interval: u32,
    pub show_tutorial: bool,
}

impl Default for GameplaySettings {
    fn default() -> Self {
        Self {
            default_speed: 1,
            auto_save_interval: 100,
            show_tutorial: true,
        }
    }
}

/// 设置标签页
#[derive(Debug, Default, PartialEq, Eq)]
pub enum SettingsTab {
    #[default]
    Audio,
    Video,
    Gameplay,
    Language,
}

/// 渲染设置面板
pub fn render_settings(mut contexts: EguiContexts, mut state: ResMut<SettingsState>) {
    if !state.show {
        return;
    }

    let ctx = contexts.ctx_mut();

    egui::Window::new("设置")
        .default_size([400.0, 350.0])
        .resizable(true)
        .show(ctx, |ui| {
            // 标签页选择
            ui.horizontal(|ui| {
                ui.selectable_value(&mut state.tab, SettingsTab::Audio, "音频");
                ui.selectable_value(&mut state.tab, SettingsTab::Video, "视频");
                ui.selectable_value(&mut state.tab, SettingsTab::Gameplay, "游戏");
                ui.selectable_value(&mut state.tab, SettingsTab::Language, "语言");
            });

            ui.separator();

            // 根据当前标签页渲染内容
            match state.tab {
                SettingsTab::Audio => render_audio_tab(ui, &mut state.settings.audio),
                SettingsTab::Video => render_video_tab(ui, &mut state.settings.video),
                SettingsTab::Gameplay => render_gameplay_tab(ui, &mut state.settings.gameplay),
                SettingsTab::Language => render_language_tab(ui, &mut state.settings.language),
            }

            ui.separator();

            // 底部按钮
            ui.horizontal(|ui| {
                if ui.button("应用").clicked() {
                    // TODO: 保存设置到文件
                    state.show = false;
                }

                if ui.button("恢复默认").clicked() {
                    state.settings = GameSettings::default();
                }

                if ui.button("返回").clicked() {
                    state.show = false;
                }
            });
        });
}

/// 渲染音频标签页
fn render_audio_tab(ui: &mut egui::Ui, audio: &mut AudioSettings) {
    ui.heading("音频设置");
    ui.add_space(8.0);

    ui.add(
        egui::Slider::new(&mut audio.master_volume, 0.0..=100.0)
            .text("主音量")
            .suffix("%"),
    );
    ui.add(
        egui::Slider::new(&mut audio.music_volume, 0.0..=100.0)
            .text("音乐")
            .suffix("%"),
    );
    ui.add(
        egui::Slider::new(&mut audio.sfx_volume, 0.0..=100.0)
            .text("音效")
            .suffix("%"),
    );
}

/// 渲染视频标签页
fn render_video_tab(ui: &mut egui::Ui, video: &mut VideoSettings) {
    ui.heading("视频设置");
    ui.add_space(8.0);

    // 分辨率选择
    let res_label = format!("{} x {}", video.resolution.0, video.resolution.1);
    egui::ComboBox::from_label("分辨率")
        .selected_text(&res_label)
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut video.resolution, (1280, 720), "1280 x 720");
            ui.selectable_value(&mut video.resolution, (1600, 900), "1600 x 900");
            ui.selectable_value(&mut video.resolution, (1920, 1080), "1920 x 1080");
            ui.selectable_value(&mut video.resolution, (2560, 1440), "2560 x 1440");
        });

    ui.add_space(4.0);
    ui.checkbox(&mut video.fullscreen, "全屏");
    ui.checkbox(&mut video.vsync, "垂直同步");
}

/// 渲染游戏标签页
fn render_gameplay_tab(ui: &mut egui::Ui, gameplay: &mut GameplaySettings) {
    ui.heading("游戏设置");
    ui.add_space(8.0);

    ui.add(
        egui::Slider::new(&mut gameplay.default_speed, 1..=3)
            .text("默认速度")
            .custom_formatter(|v, _| match v as u8 {
                1 => "慢".to_string(),
                2 => "正常".to_string(),
                3 => "快".to_string(),
                _ => "".to_string(),
            }),
    );

    ui.add_space(4.0);

    ui.add(
        egui::Slider::new(&mut gameplay.auto_save_interval, 10..=500)
            .text("自动保存间隔")
            .suffix(" 回合"),
    );

    ui.add_space(4.0);
    ui.checkbox(&mut gameplay.show_tutorial, "显示教程");
}

/// 渲染语言标签页
fn render_language_tab(ui: &mut egui::Ui, language: &mut String) {
    ui.heading("语言设置");
    ui.add_space(8.0);

    egui::ComboBox::from_label("语言")
        .selected_text(match language.as_str() {
            "zh-CN" => "中文",
            "en-US" => "English",
            "ja-JP" => "日本語",
            _ => language.as_str(),
        })
        .show_ui(ui, |ui| {
            ui.selectable_value(language, "zh-CN".to_string(), "中文");
            ui.selectable_value(language, "en-US".to_string(), "English");
            ui.selectable_value(language, "ja-JP".to_string(), "日本語");
        });
}
