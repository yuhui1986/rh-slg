//! Chunk mesh 生成：六边形几何体
//!
//! 为 32x32 格的 Chunk 生成渲染用 mesh。
//! 支持 4 级 LOD，每级使用不同精度的六边形几何。
//! 每帧最多重建 16 个 Chunk，避免卡顿。

use bevy::prelude::*;
use bevy::render::mesh::{Indices, Mesh, PrimitiveTopology};
use bevy::render::render_asset::RenderAssetUsages;

use crate::render::atlas;
use crate::render::transition;
use crate::render::ChunkData;

// ---------------------------------------------------------------------------
// 六边形几何常量（pointy-top 方向）
// ---------------------------------------------------------------------------

/// 六边形半径（中心到顶点距离）
pub const HEX_SIZE: f32 = 1.0;
/// 六边形高度 = 2 * HEX_SIZE
pub const HEX_HEIGHT: f32 = HEX_SIZE * 2.0;
/// 六边形宽度 = sqrt(3) * HEX_SIZE
pub const HEX_WIDTH: f32 = 1.732_050_8;
/// 行间距（pointy-top 行重叠 25%）
pub const ROW_SPACING: f32 = HEX_HEIGHT * 0.75;
/// 列间距
pub const COL_SPACING: f32 = HEX_WIDTH;

/// 每帧最多重建的 Chunk 数量
const MAX_REBUILDS_PER_FRAME: usize = 16;

// ---------------------------------------------------------------------------
// 公共接口
// ---------------------------------------------------------------------------

/// 为单个 Chunk 生成 mesh
///
/// 根据 LOD 级别选择不同的生成策略：
/// - 0 (Full)：每个 hex 独立六边形
/// - 1 (Merged4)：2x2 合并
/// - 2 (Merged16)：4x4 合并
/// - 3 (Minimap)：整个 Chunk 单个色块
///
/// `fog`: 0 = 黑雾（颜色调暗到 30% 亮度）, 1 = 揭开（正常）
pub fn generate_chunk_mesh(
    terrains: &[u8; 1024],
    owners: &[u8; 1024],
    fog: &[u8; 1024],
    lod_level: u8,
) -> Mesh {
    match lod_level {
        0 => generate_full_mesh(terrains, owners, fog),
        1 => generate_merged_mesh(terrains, owners, fog, 2),
        2 => generate_merged_mesh(terrains, owners, fog, 4),
        3 => generate_minimap_mesh(terrains, owners, fog),
        _ => generate_full_mesh(terrains, owners, fog),
    }
}

/// 生成带地形过渡效果的 Chunk mesh（仅 Full LOD）
///
/// 在原有 mesh 基础上，为相邻不同地形之间的边缘添加过渡三角形，
/// 使用颜色渐变实现平滑过渡，避免硬切割。
pub fn generate_chunk_mesh_with_transitions(
    terrains: &[u8; 1024],
    owners: &[u8; 1024],
    fog: &[u8; 1024],
) -> Mesh {
    let base_mesh = generate_full_mesh(terrains, owners, fog);
    let transition_overlay = transition::generate_transition_mesh(terrains);
    merge_meshes(base_mesh, transition_overlay)
}

/// 合并两个 Mesh 为一个
///
/// 将第二个 mesh 的顶点和索引追加到第一个 mesh 上，
/// 索引偏移量为第一个 mesh 的顶点数。
fn merge_meshes(mut a: Mesh, b: Mesh) -> Mesh {
    use bevy::render::mesh::VertexAttributeValues;

    let a_vert_count = a
        .attribute(Mesh::ATTRIBUTE_POSITION)
        .map(|attr| attr.len())
        .unwrap_or(0) as u32;

    // 追加顶点属性（按类型分别处理）
    for attr_id in [
        Mesh::ATTRIBUTE_POSITION,
        Mesh::ATTRIBUTE_NORMAL,
        Mesh::ATTRIBUTE_UV_0,
        Mesh::ATTRIBUTE_COLOR,
    ] {
        if let (Some(attr_a), Some(attr_b)) = (a.attribute(attr_id), b.attribute(attr_id)) {
            let merged = match (attr_a.to_owned(), attr_b) {
                (
                    VertexAttributeValues::Float32x3(mut va),
                    VertexAttributeValues::Float32x3(vb),
                ) => {
                    va.extend(vb.iter().copied());
                    VertexAttributeValues::Float32x3(va)
                }
                (
                    VertexAttributeValues::Float32x2(mut va),
                    VertexAttributeValues::Float32x2(vb),
                ) => {
                    va.extend(vb.iter().copied());
                    VertexAttributeValues::Float32x2(va)
                }
                (
                    VertexAttributeValues::Float32x4(mut va),
                    VertexAttributeValues::Float32x4(vb),
                ) => {
                    va.extend(vb.iter().copied());
                    VertexAttributeValues::Float32x4(va)
                }
                _ => continue, // 不支持的属性类型，跳过
            };
            a.insert_attribute(attr_id, merged);
        }
    }

    // 追加索引（需要偏移）
    if let (Some(Indices::U32(idx_a)), Some(Indices::U32(idx_b))) = (a.indices(), b.indices()) {
        let mut merged_idx = idx_a.clone();
        for &idx in idx_b {
            merged_idx.push(idx + a_vert_count);
        }
        a.insert_indices(Indices::U32(merged_idx));
    }

    a
}

// ---------------------------------------------------------------------------
// 系统：重建 dirty Chunk
// ---------------------------------------------------------------------------

/// 重建 dirty Chunk 的 mesh
///
/// 每帧最多处理 MAX_REBUILDS_PER_FRAME 个 Chunk，避免帧率抖动。
/// 重建时生成新 mesh 并替换旧的 Mesh2d handle。
pub fn rebuild_dirty_chunks(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    _materials: ResMut<Assets<ColorMaterial>>,
    mut query: Query<(Entity, &mut ChunkData, &Mesh2d)>,
) {
    let mut rebuilt = 0;

    for (entity, mut chunk, _old_mesh) in query.iter_mut() {
        if !chunk.dirty || rebuilt >= MAX_REBUILDS_PER_FRAME {
            continue;
        }

        // Full LOD 时启用地形过渡渲染，更高 LOD 级别跳过过渡以节省性能
        let new_mesh = if chunk.current_lod == 0 {
            generate_chunk_mesh_with_transitions(&chunk.terrains, &chunk.owners, &chunk.fog)
        } else {
            generate_chunk_mesh(
                &chunk.terrains,
                &chunk.owners,
                &chunk.fog,
                chunk.current_lod,
            )
        };
        let new_handle = meshes.add(new_mesh);

        // 替换 mesh handle（旧 handle 自动释放）
        commands.entity(entity).insert(Mesh2d(new_handle));

        chunk.dirty = false;
        rebuilt += 1;
    }
}

// ---------------------------------------------------------------------------
// Full LOD：每个 hex 一个六边形
// ---------------------------------------------------------------------------

fn generate_full_mesh(terrains: &[u8; 1024], owners: &[u8; 1024], fog: &[u8; 1024]) -> Mesh {
    // 预分配：1024 hex * 7 vertices = 7168 vertices, 1024 * 18 indices = 18432
    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(7168);
    let mut normals: Vec<[f32; 3]> = Vec::with_capacity(7168);
    let mut uvs: Vec<[f32; 2]> = Vec::with_capacity(7168);
    let mut colors: Vec<[f32; 4]> = Vec::with_capacity(7168);
    let mut indices: Vec<u32> = Vec::with_capacity(18432);

    for row in 0..32u32 {
        for col in 0..32u32 {
            let idx = (row * 32 + col) as usize;
            let terrain_id = terrains[idx];
            let owner_id = owners[idx];
            let is_fogged = fog[idx] == 0;

            let center = hex_center(row, col);
            let terrain_col = atlas::terrain_color_from_u8(terrain_id);
            let faction_col = atlas::faction_color(owner_id);
            let final_color = blend_colors(terrain_col, faction_col);
            // 黑雾：颜色 × 0.3 + 黑 0.0 (alpha 保持 1.0)
            let final_color = if is_fogged {
                let s = final_color.to_srgba();
                // 0.55 调暗（之前 0.3 太暗看着像遮罩，0.55 让玩家能看清地形但不显眼）
                Color::srgb(s.red * 0.55, s.green * 0.55, s.blue * 0.55)
            } else {
                final_color
            };
            let color_arr = color_to_f32_4(final_color);

            let vert_start = positions.len() as u32;

            // 中心顶点
            positions.push([center.x, center.y, 0.0]);
            normals.push([0.0, 0.0, 1.0]);
            uvs.push([0.5, 0.5]);
            colors.push(color_arr);

            // 6 个角顶点（pointy-top：起始角 30 度）
            for i in 0..6 {
                let angle = std::f32::consts::FRAC_PI_3 * i as f32 + std::f32::consts::FRAC_PI_6;
                let x = center.x + HEX_SIZE * angle.cos();
                let y = center.y + HEX_SIZE * angle.sin();
                positions.push([x, y, 0.0]);
                normals.push([0.0, 0.0, 1.0]);
                uvs.push([0.5 + 0.5 * angle.cos(), 0.5 + 0.5 * angle.sin()]);
                colors.push(color_arr);
            }

            // 6 个三角形（扇形）
            for i in 0..6 {
                indices.push(vert_start);
                indices.push(vert_start + 1 + i);
                indices.push(vert_start + 1 + (i + 1) % 6);
            }
        }
    }

    build_mesh(positions, normals, uvs, colors, indices)
}

// ---------------------------------------------------------------------------
// Merged LOD：合并 N*N hex 为一个大六边形
// ---------------------------------------------------------------------------

fn generate_merged_mesh(
    terrains: &[u8; 1024],
    _owners: &[u8; 1024],
    fog: &[u8; 1024],
    merge_size: u32,
) -> Mesh {
    let chunks_per_row = 32 / merge_size;
    let total = (chunks_per_row * chunks_per_row) as usize;

    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(total * 7);
    let mut normals: Vec<[f32; 3]> = Vec::with_capacity(total * 7);
    let mut uvs: Vec<[f32; 2]> = Vec::with_capacity(total * 7);
    let mut colors: Vec<[f32; 4]> = Vec::with_capacity(total * 7);
    let mut indices: Vec<u32> = Vec::with_capacity(total * 18);

    for mr in 0..chunks_per_row {
        for mc in 0..chunks_per_row {
            // 计算合并块内所有 hex 的平均地形颜色
            let mut r_sum = 0.0f32;
            let mut g_sum = 0.0f32;
            let mut b_sum = 0.0f32;
            let mut fogged_count = 0u32;
            let mut count = 0u32;

            for dr in 0..merge_size {
                for dc in 0..merge_size {
                    let row = mr * merge_size + dr;
                    let col = mc * merge_size + dc;
                    let idx = (row * 32 + col) as usize;
                    let c = atlas::terrain_color_from_u8(terrains[idx]);
                    let s = c.to_srgba();
                    r_sum += s.red;
                    g_sum += s.green;
                    b_sum += s.blue;
                    if fog[idx] == 0 {
                        fogged_count += 1;
                    }
                    count += 1;
                }
            }

            // 任一格 fogged → 整个合并块视为 fogged（保守：揭示的范围不会跨 chunk）
            let is_fogged = fogged_count > 0;

            let avg_color = Color::srgb(
                r_sum / count as f32,
                g_sum / count as f32,
                b_sum / count as f32,
            );
            let final_color = if is_fogged {
                let s = avg_color.to_srgba();
                Color::srgb(s.red * 0.55, s.green * 0.55, s.blue * 0.55)
            } else {
                avg_color
            };
            let color_arr = color_to_f32_4(final_color);

            // 合并块中心坐标（取块内中间 hex 的位置）
            let center_row = mr * merge_size + merge_size / 2;
            let center_col = mc * merge_size + merge_size / 2;
            let center = hex_center(center_row, center_col);

            let vert_start = positions.len() as u32;
            let scaled_size = HEX_SIZE * merge_size as f32;

            // 中心顶点
            positions.push([center.x, center.y, 0.0]);
            normals.push([0.0, 0.0, 1.0]);
            uvs.push([0.5, 0.5]);
            colors.push(color_arr);

            // 6 个角顶点
            for i in 0..6 {
                let angle = std::f32::consts::FRAC_PI_3 * i as f32 + std::f32::consts::FRAC_PI_6;
                let x = center.x + scaled_size * angle.cos();
                let y = center.y + scaled_size * angle.sin();
                positions.push([x, y, 0.0]);
                normals.push([0.0, 0.0, 1.0]);
                uvs.push([0.5 + 0.5 * angle.cos(), 0.5 + 0.5 * angle.sin()]);
                colors.push(color_arr);
            }

            // 6 个三角形
            for i in 0..6 {
                indices.push(vert_start);
                indices.push(vert_start + 1 + i);
                indices.push(vert_start + 1 + (i + 1) % 6);
            }
        }
    }

    build_mesh(positions, normals, uvs, colors, indices)
}

// ---------------------------------------------------------------------------
// Minimap LOD：整个 Chunk 一个色块
// ---------------------------------------------------------------------------

fn generate_minimap_mesh(terrains: &[u8; 1024], owners: &[u8; 1024], fog: &[u8; 1024]) -> Mesh {
    generate_merged_mesh(terrains, owners, fog, 32)
}

// ---------------------------------------------------------------------------
// 辅助函数
// ---------------------------------------------------------------------------

/// 计算 hex 中心坐标（pointy-top offset 坐标系）
///
/// 奇数行向右偏移半个列宽，形成蜂窝排列。
pub fn hex_center(row: u32, col: u32) -> Vec2 {
    let x = col as f32 * COL_SPACING + if row % 2 == 1 { COL_SPACING * 0.5 } else { 0.0 };
    let y = row as f32 * ROW_SPACING;
    Vec2::new(x, y)
}

/// 颜色 alpha 混合
///
/// 将势力颜色叠加在地形颜色之上。若势力颜色完全透明则直接返回地形色。
fn blend_colors(terrain: Color, faction: Color) -> Color {
    let t = terrain.to_srgba();
    let f = faction.to_srgba();
    if f.alpha == 0.0 {
        return terrain;
    }
    Color::srgba(
        t.red * (1.0 - f.alpha) + f.red * f.alpha,
        t.green * (1.0 - f.alpha) + f.green * f.alpha,
        t.blue * (1.0 - f.alpha) + f.blue * f.alpha,
        1.0,
    )
}

/// Color 转 [f32; 4]（Srgba 顺序）
fn color_to_f32_4(c: Color) -> [f32; 4] {
    let s = c.to_srgba();
    [s.red, s.green, s.blue, s.alpha]
}

/// 构建 Bevy Mesh
pub fn build_mesh(
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    colors: Vec<[f32; 4]>,
    indices: Vec<u32>,
) -> Mesh {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}
