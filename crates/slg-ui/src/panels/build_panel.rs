//! M8 UI: 建筑 / 升级 / 建分城 面板
//!
//! 当玩家选中己方格时, 在屏幕右侧显示一个侧栏:
//! - 当前格坐标
//! - 当前建筑 (如有) + 升级按钮
//! - 5 种建建筑按钮 (Farm/LumberMill/Mine/Barracks/CityWall)
//! - "建分城" 按钮 (条件: 周围 6+ 己方格 + < 2 个分城)
//! - "取消" 按钮
//!
//! 玩家点按钮 → 发 `BuildAction` event → `slg-app::handle_build_order` 处理
//!
//! **Bevy 集成测试不测 UI 渲染 (MinimalPlugins 没 egui)**
//! 测 data flow: 发 BuildAction → 资源 / 建筑 状态变化

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};
use slg_core::building::BuildingType;
use slg_core::map::grid::HexCoord;

use crate::top_bar::TopBarState;

/// 选中状态: 与 slg-app 的 SelectedHex 镜像
///
/// 实际数据源是 slg-app::SelectedHex Resource,
/// slg-ui 的 build_panel 通过 read-only 拿数据 (Bevy Resource 跨 crate 可读)
#[derive(Resource, Default, Debug, Clone)]
pub struct BuildPanelState {
    pub coord: Option<HexCoord>,
    /// 玩家势力 ID
    pub player_faction_id: String,
    /// 当前选中格上的建筑 type (None = 空地)
    pub current_building: Option<BuildingType>,
    /// 当前建筑等级
    pub current_building_level: u8,
    /// 玩家已有分城数
    pub subcity_count: u8,
    /// 资源 (用于判断按钮是否可用)
    pub gold: u64,
    pub food: u64,
    pub wood: u64,
}

/// 面板尺寸 / 位置
const PANEL_WIDTH: f32 = 280.0;

/// 渲染建筑面板
///
/// 在选中状态下显示, 否则不渲染。
pub fn render_build_panel(
    mut contexts: EguiContexts,
    panel_state: Res<BuildPanelState>,
    top_bar: Res<TopBarState>,
    mut action_events: EventWriter<BuildPanelAction>,
) {
    let Some(coord) = panel_state.coord else {
        return;
    };
    let _ = top_bar; // 暂时不用, 留作 future (战报 / 调试信息)
    let ctx = contexts.ctx_mut();

    egui::SidePanel::right("build_panel")
        .default_width(PANEL_WIDTH)
        .resizable(false)
        .show(ctx, |ui| {
            ui.add_space(8.0);
            ui.heading(format!("🏰 地块 ({}, {})", coord.q, coord.r));
            ui.separator();

            // 当前建筑
            ui.label("当前建筑:");
            if let Some(btype) = panel_state.current_building {
                ui.horizontal(|ui| {
                    ui.label(format!(
                        "{} L{}",
                        btype.display_name(),
                        panel_state.current_building_level
                    ));
                    // 升级按钮
                    let can_upgrade = panel_state.current_building_level < 3
                        && (panel_state.gold >= 100 || panel_state.current_building_level == 2)
                        && (panel_state.food >= 50
                            || panel_state.current_building_level == 2);
                    let cost_text = if panel_state.current_building_level == 1 {
                        "100g + 50f"
                    } else if panel_state.current_building_level == 2 {
                        "200g + 100f + 50w"
                    } else {
                        "满级"
                    };
                    if ui
                        .add_enabled(can_upgrade, egui::Button::new(format!("⬆ 升级 ({})", cost_text)))
                        .clicked()
                    {
                        action_events.send(BuildPanelAction::Upgrade(coord));
                    }
                });
            } else {
                ui.label("  (空地)");
            }

            ui.add_space(8.0);
            ui.separator();
            ui.label("建造 (50g/个):");

            // 5 种建筑按钮
            ui.horizontal(|ui| {
                for btype in BuildingType::ALL.iter() {
                    let enabled = panel_state.current_building.is_none() && panel_state.gold >= 50;
                    if ui
                        .add_enabled(enabled, egui::Button::new(btype.display_name()))
                        .clicked()
                    {
                        action_events.send(BuildPanelAction::Build(coord, *btype));
                    }
                }
            });

            ui.add_space(8.0);
            ui.separator();

            // 建分城
            let can_subcity = panel_state.subcity_count < 2
                && panel_state.gold >= 500
                && panel_state.food >= 200
                && panel_state.wood >= 100;
            if ui
                .add_enabled(
                    can_subcity,
                    egui::Button::new(format!(
                        "🏯 建分城 ({}/2, 500g+200f+100w)",
                        panel_state.subcity_count
                    )),
                )
                .clicked()
            {
                action_events.send(BuildPanelAction::EstablishSubcity(coord));
            }
            ui.label("  (要求周围 6+ 邻接为己方)");

            ui.add_space(8.0);
            ui.separator();

            // 取消按钮
            if ui.button("❌ 取消选中 (Esc)").clicked() {
                action_events.send(BuildPanelAction::Deselect);
            }
        });
}

/// 建筑面板 Action (UI → slg-app)
///
/// BuildPanelAction 跟 slg-app 的 BuildAction 是同构的,
/// 但分两个 enum 避免 slg-ui 依赖 slg-app 的 event
/// (slg-app 的 handle_hex_click 取消选中用 SelectedHex.coord = None,
/// 但 UI 的取消按钮也发这个 event, slg-app 收到后调 SelectedHex.coord = None)
#[derive(Event, Debug, Clone, PartialEq, Eq)]
pub enum BuildPanelAction {
    /// 建建筑
    Build(HexCoord, BuildingType),
    /// 升级
    Upgrade(HexCoord),
    /// 建分城
    EstablishSubcity(HexCoord),
    /// 取消选中
    Deselect,
}

// ---------------------------------------------------------------------------
// 测试: 不依赖 Bevy 渲染, 测数据流
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_panel_action_build_event() {
        // 验证 BuildPanelAction::Build 不抛 panic
        let action = BuildPanelAction::Build(
            HexCoord::new(5, 5),
            BuildingType::Farm,
        );
        match action {
            BuildPanelAction::Build(c, t) => {
                assert_eq!(c, HexCoord::new(5, 5));
                assert_eq!(t, BuildingType::Farm);
            }
            _ => panic!("expected Build"),
        }
    }

    #[test]
    fn test_build_panel_state_default_none() {
        let state = BuildPanelState::default();
        assert!(state.coord.is_none());
        assert!(state.current_building.is_none());
        assert_eq!(state.subcity_count, 0);
    }

    #[test]
    fn test_panel_does_not_show_when_no_selection() {
        // 没有 SelectedHex 时, panel 不渲染 (这里只能验证 State 默认值)
        let state = BuildPanelState::default();
        assert!(state.coord.is_none(), "默认不显示");
    }

    /// 验证分城条件检查: CityManager helper
    #[test]
    fn test_can_establish_subcity_via_city_manager() {
        let cm = slg_core::city::CityManager::new();
        // 没有分城 + 6 邻接 owned = 可以
        let coord = HexCoord::new(5, 5);
        let mut keys = std::collections::BTreeSet::new();
        keys.insert(coord.to_tile_key());
        for n in coord.neighbors() {
            keys.insert(n.to_tile_key());
        }
        let result = cm.can_establish_subcity(coord, &"faction_1".to_string(), &keys);
        assert!(result.is_ok(), "应有 6 邻接 owned, 可建分城");
    }
}
