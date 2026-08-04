//! 迷雾渲染系统
//!
//! M1-T14：实现迷雾/视野的渲染层。
//!
//! 设计（§6.7）：
//! - 每个 Chunk 对应一张 R8 纹理（32x32 = 1024 字节）
//! - 状态值：0 = 未探索（黑色），1 = 已探索（半暗），2 = 当前可见（透明）
//! - Fragment shader 在地形纹理之上叠加迷雾层
//! - 视野计算在 slg-core 侧完成，渲染侧只负责显示

use bevy::prelude::*;

/// 迷雾状态
///
/// 对应 FogChunk 中每个字节的值，表示该 hex 的可见性。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FogState {
    /// 未探索：完全黑色遮罩
    Unexplored = 0,
    /// 已探索：半透明黑色遮罩（曾看到过，当前无视野）
    Explored = 1,
    /// 当前可见：完全透明（视野范围内）
    Visible = 2,
}

impl FogState {
    /// 从 u8 值转换为 FogState
    ///
    /// 未识别的值默认为 Unexplored（最安全的遮挡状态）。
    pub fn from_u8(val: u8) -> Self {
        match val {
            0 => Self::Unexplored,
            1 => Self::Explored,
            2 => Self::Visible,
            _ => Self::Unexplored,
        }
    }
}

/// 迷雾覆盖组件
///
/// 每个 Chunk Entity 附加一个 FogOverlay，存储 32x32 = 1024 字节的迷雾状态。
/// 数据与 FogChunk::data 一一对应，由 `sync_fog_overlay` 系统从 FogOfWar 资源同步。
#[derive(Component)]
pub struct FogOverlay {
    /// 32x32 = 1024 字节迷雾状态数据
    pub data: Vec<u8>,
}

impl FogOverlay {
    /// 创建全黑（未探索）的迷雾覆盖
    pub fn new_fogged() -> Self {
        Self {
            data: vec![0; 1024],
        }
    }

    /// 创建全亮（可见）的迷雾覆盖
    pub fn new_visible() -> Self {
        Self {
            data: vec![FogState::Visible as u8; 1024],
        }
    }

    /// 获取指定位置的迷雾状态
    pub fn get(&self, row: usize, col: usize) -> FogState {
        let idx = row * 32 + col;
        FogState::from_u8(self.data[idx])
    }

    /// 设置指定位置的迷雾状态
    pub fn set(&mut self, row: usize, col: usize, state: FogState) {
        let idx = row * 32 + col;
        self.data[idx] = state as u8;
    }
}

impl Default for FogOverlay {
    fn default() -> Self {
        Self::new_fogged()
    }
}

/// 迷雾状态对应的颜色
///
/// 用于在地形颜色之上叠加迷雾遮罩。
/// - Unexplored：完全不透明的黑色，彻底遮挡地形
/// - Explored：半透明黑色，地形可见但变暗
/// - Visible：完全透明，地形正常显示
pub fn fog_color(state: FogState) -> Color {
    match state {
        FogState::Unexplored => Color::srgba(0.0, 0.0, 0.0, 1.0),
        FogState::Explored => Color::srgba(0.0, 0.0, 0.0, 0.5),
        FogState::Visible => Color::NONE,
    }
}

/// 从 u8 值直接获取迷雾颜色（便捷函数）
pub fn fog_color_from_u8(val: u8) -> Color {
    fog_color(FogState::from_u8(val))
}

/// 同步迷雾数据到 FogOverlay 组件
///
/// 当 FogOfWar 资源更新时，将对应的 FogChunk 数据写入各 Chunk Entity 的 FogOverlay。
/// 此系统目前为框架占位，后续连接 slg-core 的 FogOfWar 资源。
pub fn sync_fog_overlay(
    // TODO: 接入 FogOfWar 资源
    // fog_res: Res<FogOfWar>,
    mut fog_query: Query<&mut FogOverlay>,
) {
    // 遍历所有 FogOverlay，标记数据变化
    // 完整实现将在 slg-core 的视野计算完成后接入
    for _fog in fog_query.iter_mut() {
        // 占位：后续从 FogOfWar Resource 读取对应 chunk 数据
        // 并与当前 FogOverlay.data 比较，仅更新变化的 chunk
    }
}
