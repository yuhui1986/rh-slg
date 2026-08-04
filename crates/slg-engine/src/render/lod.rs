//! LOD 系统：基于相机缩放自动切换渲染精度
//!
//! 4 级 LOD：
//! - Full (0)：每个 hex 独立渲染，近距离细节
//! - Merged4 (1)：2x2 hex 合并为一个，中等距离
//! - Merged16 (2)：4x4 hex 合并为一个，远距离
//! - Minimap (3)：整个 Chunk 渲染为单个色块，鸟瞰视角

use bevy::prelude::*;

use crate::render::ChunkData;

/// LOD 级别枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum LodLevel {
    /// 每个 hex 一个六边形
    Full = 0,
    /// 2x2 hex 合并
    Merged4 = 1,
    /// 4x4 hex 合并
    Merged16 = 2,
    /// 整个 Chunk 纯色块
    Minimap = 3,
}

/// 根据相机缩放值选择 LOD 级别
///
/// `zoom` 是相机缩放因子（1.0 = 标准视图，>1 放大，<1 缩小）。
/// 阈值经过调优，确保在常见缩放范围内平滑切换。
pub fn select_lod(zoom: f32) -> LodLevel {
    if zoom > 0.8 {
        LodLevel::Full
    } else if zoom > 0.4 {
        LodLevel::Merged4
    } else if zoom > 0.15 {
        LodLevel::Merged16
    } else {
        LodLevel::Minimap
    }
}

/// LOD 切换阈值（从高到低）
pub const LOD_THRESHOLDS: [f32; 4] = [0.8, 0.4, 0.15, 0.0];

/// 更新所有 Chunk 的 LOD 级别
///
/// 每帧检查相机缩放，当 LOD 级别变化时标记 Chunk 为 dirty，
/// 由 `rebuild_dirty_chunks` 系统在后续帧重建 mesh。
pub fn update_chunk_lod(
    camera_query: Query<&Projection, With<Camera2d>>,
    mut chunk_query: Query<&mut ChunkData>,
) {
    let Ok(projection) = camera_query.get_single() else {
        return;
    };

    // 从 Projection 提取缩放值
    let scale = match projection {
        Projection::Orthographic(ortho) => ortho.scale,
        _ => 1.0,
    };

    // OrthographicProjection.scale: 1.0 = 标准，>1 放大视口（缩小内容），<1 缩小视口（放大内容）
    // 转换为 zoom：zoom = 1.0 / scale
    let zoom = 1.0 / scale;

    let new_lod = select_lod(zoom) as u8;

    for mut chunk in chunk_query.iter_mut() {
        if chunk.current_lod != new_lod {
            chunk.current_lod = new_lod;
            chunk.dirty = true;
        }
    }
}
