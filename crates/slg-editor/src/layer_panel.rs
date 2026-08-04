//! 图层管理面板：可见性切换、锁定、活跃图层选择、透明度调节

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};
use serde::{Deserialize, Serialize};

/// 图层类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LayerType {
    Terrain,  // 地形
    Resource, // 资源
    Building, // 建筑
    Faction,  // 势力
    Rule,     // 规则
    River,    // 河流
}

impl LayerType {
    pub fn name(&self) -> &str {
        match self {
            LayerType::Terrain => "地形",
            LayerType::Resource => "资源",
            LayerType::Building => "建筑",
            LayerType::Faction => "势力",
            LayerType::Rule => "规则",
            LayerType::River => "河流",
        }
    }

    pub fn all() -> Vec<LayerType> {
        vec![
            LayerType::Terrain,
            LayerType::Resource,
            LayerType::Building,
            LayerType::Faction,
            LayerType::Rule,
            LayerType::River,
        ]
    }
}

/// 图层状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerState {
    pub layer_type: LayerType,
    pub visible: bool,
    pub locked: bool,
    pub opacity: f32,
}

impl LayerState {
    pub fn new(layer_type: LayerType) -> Self {
        Self {
            layer_type,
            visible: true,
            locked: false,
            opacity: 1.0,
        }
    }
}

/// 图层管理器
#[derive(Resource)]
pub struct LayerManager {
    pub layers: Vec<LayerState>,
    pub active_layer: LayerType,
    pub show: bool,
}

impl Default for LayerManager {
    fn default() -> Self {
        let layers = LayerType::all().into_iter().map(LayerState::new).collect();

        Self {
            layers,
            active_layer: LayerType::Terrain,
            show: false,
        }
    }
}

impl LayerManager {
    /// 查询图层是否可见
    pub fn is_visible(&self, layer_type: LayerType) -> bool {
        self.layers
            .iter()
            .find(|l| l.layer_type == layer_type)
            .is_none_or(|l| l.visible)
    }

    /// 查询图层是否锁定
    pub fn is_locked(&self, layer_type: LayerType) -> bool {
        self.layers
            .iter()
            .find(|l| l.layer_type == layer_type)
            .is_some_and(|l| l.locked)
    }

    /// 获取图层透明度
    pub fn get_opacity(&self, layer_type: LayerType) -> f32 {
        self.layers
            .iter()
            .find(|l| l.layer_type == layer_type)
            .map_or(1.0, |l| l.opacity)
    }
}

/// 渲染图层管理面板
pub fn render_layer_panel(mut contexts: EguiContexts, mut layer_manager: ResMut<LayerManager>) {
    if !layer_manager.show {
        return;
    }

    let ctx = contexts.ctx_mut();

    egui::Window::new("图层")
        .default_size([200.0, 300.0])
        .show(ctx, |ui| {
            // 将活跃图层拷贝出来，避免在闭包中同时可变借用 layers 和 active_layer
            let mut new_active = layer_manager.active_layer;

            for layer in layer_manager.layers.iter_mut() {
                ui.horizontal(|ui| {
                    // 可见性切换
                    let vis_icon = if layer.visible { "👁" } else { "—" };
                    if ui.button(vis_icon).clicked() {
                        layer.visible = !layer.visible;
                    }

                    // 锁定切换
                    let lock_icon = if layer.locked { "🔒" } else { "🔓" };
                    if ui.button(lock_icon).clicked() {
                        layer.locked = !layer.locked;
                    }

                    // 图层名称（可选择为活跃图层）
                    let is_active = new_active == layer.layer_type;
                    if ui
                        .selectable_label(is_active, layer.layer_type.name())
                        .clicked()
                    {
                        new_active = layer.layer_type;
                    }
                });

                // 透明度滑块
                ui.horizontal(|ui| {
                    ui.label("透明度:");
                    ui.add(egui::Slider::new(&mut layer.opacity, 0.0..=1.0));
                });

                ui.separator();
            }

            layer_manager.active_layer = new_active;
        });
}
