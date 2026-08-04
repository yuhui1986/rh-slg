//! 渲染子系统：Chunk 网格、纹理图集、LOD
//!
//! M1-T12：实现六边形地图的 Chunk 渲染与 LOD 系统。

pub mod atlas;
pub mod chunk_mesh;
pub mod fog;
pub mod lod;
pub mod transition;

use bevy::prelude::*;

use crate::render::chunk_mesh::generate_chunk_mesh;

/// Chunk 数据组件
///
/// 每个 Chunk 覆盖 32x32 格，存储地形、势力、等级信息。
/// 当数据变化时设置 `dirty = true`，由 `rebuild_dirty_chunks` 系统重建 mesh。
#[derive(Component)]
pub struct ChunkData {
    /// Chunk 在地图网格中的 X 坐标
    pub chunk_x: i32,
    /// Chunk 在地图网格中的 Y 坐标
    pub chunk_y: i32,
    /// 32x32 = 1024 格地形类型（u8 编码，对应 TerrainType::to_u8）
    pub terrains: [u8; 1024],
    /// 32x32 = 1024 格势力归属（0 = 无主，1~255 = 势力编号）
    pub owners: [u8; 1024],
    /// 32x32 = 1024 格土地等级（1~9）
    pub levels: [u8; 1024],
    /// 是否需要重建 mesh
    pub dirty: bool,
    /// 当前 LOD 级别（0=Full, 1=Merged4, 2=Merged16, 3=Minimap）
    pub current_lod: u8,
}

impl Default for ChunkData {
    fn default() -> Self {
        Self {
            chunk_x: 0,
            chunk_y: 0,
            terrains: [0; 1024],
            owners: [0; 1024],
            levels: [1; 1024],
            dirty: true,
            current_lod: 0,
        }
    }
}

/// Chunk 渲染插件
///
/// 注册 LOD 更新、mesh 重建等系统到 Bevy App。
pub struct ChunkRenderPlugin;

impl Plugin for ChunkRenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                lod::update_chunk_lod,
                chunk_mesh::rebuild_dirty_chunks,
                fog::sync_fog_overlay,
            ),
        );
    }
}

/// 生成 Chunk 在世界坐标中的位置
///
/// 每个 Chunk 是 32x32 格，坐标原点在左下角。
pub fn chunk_world_offset(chunk_x: i32, chunk_y: i32) -> Vec2 {
    // 每个 Chunk 的像素尺寸
    let chunk_w = 32.0 * chunk_mesh::COL_SPACING;
    let chunk_h = 32.0 * chunk_mesh::ROW_SPACING;
    Vec2::new(chunk_x as f32 * chunk_w, chunk_y as f32 * chunk_h)
}

/// 生成 Chunk mesh 并返回 Handle
///
/// 供地图加载系统调用，创建新的 Chunk Entity。
pub fn build_chunk_mesh(terrains: &[u8; 1024], owners: &[u8; 1024], lod_level: u8) -> Mesh {
    generate_chunk_mesh(terrains, owners, lod_level)
}

/// 生成带地形过渡效果的 Chunk mesh（Full LOD）
///
/// 供近距离观察时使用，在基础地形 mesh 之上叠加过渡几何体，
/// 让相邻不同地形之间有平滑的颜色渐变。
pub fn build_chunk_mesh_with_transitions(terrains: &[u8; 1024], owners: &[u8; 1024]) -> Mesh {
    chunk_mesh::generate_chunk_mesh_with_transitions(terrains, owners)
}
