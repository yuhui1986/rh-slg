//! 地图画廊 UI 面板
//!
//! M2-T14: 让玩家可以浏览、筛选、加载地图。

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};
use slg_editor::gallery::*;

/// 画廊 UI 状态
#[derive(Resource, Default)]
pub struct GalleryUiState {
    /// 是否显示画廊面板
    pub show: bool,
    /// 搜索关键词
    pub search_text: String,
    /// 当前选中的标签筛选
    pub selected_tag: Option<String>,
    /// 状态消息（加载成功/失败等）
    pub status_message: Option<String>,
}

/// 可用的标签列表
const TAG_LIST: &[&str] = &["全部", "标准", "挑战", "水域", "平原"];

/// 用于显示的地图条目快照（避免借用冲突）
struct GalleryEntryView {
    index: usize,
    name: String,
    description: String,
    author: String,
    width: u32,
    height: u32,
    tags: Vec<String>,
}

/// 渲染地图画廊面板
pub fn render_gallery(
    mut contexts: EguiContexts,
    mut gallery: ResMut<MapGallery>,
    mut ui_state: ResMut<GalleryUiState>,
) {
    if !ui_state.show {
        return;
    }

    let ctx = contexts.ctx_mut();

    egui::Window::new("地图画廊")
        .default_size([500.0, 400.0])
        .show(ctx, |ui| {
            // 搜索栏
            ui.horizontal(|ui| {
                ui.label("搜索:");
                ui.text_edit_singleline(&mut ui_state.search_text);
            });

            // 标签筛选
            ui.horizontal(|ui| {
                ui.label("标签:");
                for &tag in TAG_LIST {
                    let selected = match &ui_state.selected_tag {
                        Some(t) => t == tag,
                        None => tag == "全部",
                    };
                    if ui.selectable_label(selected, tag).clicked() {
                        ui_state.selected_tag = if tag == "全部" {
                            None
                        } else {
                            Some(tag.to_string())
                        };
                        gallery.filter_by_tag(ui_state.selected_tag.clone());
                    }
                }
            });

            ui.separator();

            // 预先收集筛选结果（避免在 UI 闭包中借用 gallery）
            // 使用原始索引，避免筛选后索引不匹配
            let selected_index = gallery.selected_index;
            let filtered: Vec<GalleryEntryView> = gallery
                .entries
                .iter()
                .enumerate()
                .filter(|(_, entry)| {
                    // 标签筛选
                    if let Some(ref tag) = gallery.filter_tag {
                        if !entry.tags.contains(tag) {
                            return false;
                        }
                    }
                    // 搜索筛选
                    if !ui_state.search_text.is_empty() {
                        let query = ui_state.search_text.to_lowercase();
                        entry.name.to_lowercase().contains(&query)
                            || entry.description.to_lowercase().contains(&query)
                            || entry.author.to_lowercase().contains(&query)
                    } else {
                        true
                    }
                })
                .map(|(i, entry)| GalleryEntryView {
                    index: i,
                    name: entry.name.clone(),
                    description: entry.description.clone(),
                    author: entry.author.clone(),
                    width: entry.width,
                    height: entry.height,
                    tags: entry.tags.clone(),
                })
                .collect();

            ui.label(format!("共 {} 张地图", filtered.len()));

            // 跟踪用户点击选中的索引
            let mut clicked_index: Option<usize> = None;

            egui::ScrollArea::vertical()
                .max_height(250.0)
                .show(ui, |ui| {
                    for entry in &filtered {
                        let selected = selected_index == Some(entry.index);

                        let response = ui.group(|ui| {
                            ui.horizontal(|ui| {
                                if ui.selectable_label(selected, &entry.name).clicked() {
                                    clicked_index = Some(entry.index);
                                }

                                ui.label(format!("{}x{}", entry.width, entry.height));

                                for tag in &entry.tags {
                                    let _ = ui.small_button(tag);
                                }
                            });

                            ui.horizontal(|ui| {
                                ui.label(&entry.description);
                                if !entry.author.is_empty() {
                                    ui.label(format!("作者: {}", entry.author));
                                }
                            });
                        });

                        // 高亮选中项
                        if selected {
                            let rect = response.response.rect;
                            ui.painter().rect_stroke(
                                rect,
                                0.0,
                                egui::Stroke::new(2.0_f32, egui::Color32::from_rgb(100, 200, 255)),
                                egui::StrokeKind::Outside,
                            );
                        }
                    }
                });

            // 在闭包外应用选中操作
            if let Some(idx) = clicked_index {
                gallery.select(idx);
            }

            ui.separator();

            // 状态消息
            if let Some(msg) = &ui_state.status_message {
                ui.colored_label(egui::Color32::from_rgb(255, 200, 50), msg);
            }

            // 操作按钮
            ui.horizontal(|ui| {
                if ui.button("刷新").clicked() {
                    gallery.entries.clear();
                    scan_builtin_maps(&mut gallery);
                    scan_user_maps(&mut gallery);
                    ui_state.status_message =
                        Some(format!("已刷新，共 {} 张地图", gallery.entries.len()));
                }

                let has_selection = gallery.selected().is_some();
                if ui
                    .add_enabled(has_selection, egui::Button::new("加载选中地图"))
                    .clicked()
                {
                    if let Some(_doc) = gallery.load_selected() {
                        // TODO: 将 MapDocument 加载到游戏世界
                        ui_state.status_message = Some("地图加载成功".to_string());
                        ui_state.show = false;
                    } else {
                        ui_state.status_message = Some("地图加载失败".to_string());
                    }
                }

                if ui.button("关闭").clicked() {
                    ui_state.show = false;
                }
            });
        });
}
