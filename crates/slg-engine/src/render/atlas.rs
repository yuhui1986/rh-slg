//! 纹理图集：地形颜色 + 势力着色
//!
//! 当前使用纯色占位，后续替换为真实纹理图集。
//! 地形颜色基于 TerrainType 枚举，势力颜色基于 owner 编号。

use bevy::prelude::*;
use slg_core::map::tile::TerrainType;

/// 地形颜色（占位，后续替换为纹理采样）
///
/// 每种 TerrainType 对应一个基础颜色，用于 chunk mesh 的顶点着色。
pub fn terrain_color(terrain: TerrainType) -> Color {
    match terrain {
        TerrainType::Plains => Color::srgb(0.4, 0.7, 0.3),
        TerrainType::Mountain => Color::srgb(0.5, 0.5, 0.5),
        TerrainType::Water => Color::srgb(0.2, 0.4, 0.8),
        TerrainType::Forest => Color::srgb(0.2, 0.5, 0.2),
        TerrainType::Desert => Color::srgb(0.8, 0.7, 0.4),
        TerrainType::Swamp => Color::srgb(0.3, 0.4, 0.3),
        TerrainType::Hills => Color::srgb(0.6, 0.5, 0.4),
        TerrainType::Pass => Color::srgb(0.7, 0.6, 0.5),
    }
}

/// 从 u8 地形 ID 获取颜色
///
/// Chunk 数组中存储的是 u8 编码的地形类型，此函数做转换。
pub fn terrain_color_from_u8(id: u8) -> Color {
    match TerrainType::from_u8(id) {
        Some(t) => terrain_color(t),
        None => Color::srgb(0.5, 0.5, 0.5), // 未知地形用灰色
    }
}

/// 势力颜色（半透明叠加层）
///
/// owner=0 表示无主，返回完全透明色。
/// 各势力使用不同颜色，alpha=0.3 用于叠加在地形颜色之上。
pub fn faction_color(owner: u8) -> Color {
    match owner {
        0 => Color::NONE,                       // 无主 - 透明
        1 => Color::srgba(0.2, 0.2, 0.8, 0.3),  // 魏 - 蓝
        2 => Color::srgba(0.2, 0.7, 0.2, 0.3),  // 蜀 - 绿
        3 => Color::srgba(0.8, 0.2, 0.2, 0.3),  // 吴 - 红
        4 => Color::srgba(0.8, 0.8, 0.2, 0.3),  // 辽东 - 黄
        5 => Color::srgba(0.6, 0.3, 0.6, 0.3),  // 南中 - 紫
        6 => Color::srgba(1.0, 0.84, 0.0, 0.3), // 玩家 - 金
        _ => Color::srgba(0.5, 0.5, 0.5, 0.3),  // 其他 - 灰
    }
}
