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
/// M10.2: 16 → 4, 减少一帧内 mesh 替换数, 缓解视觉抢渲染 (用户报"新对局就抖")
const MAX_REBUILDS_PER_FRAME: usize = 4;

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
/// `selected`: 0 = 未选中, 1 = 选中（叠加金色 (1.0, 0.84, 0.0) × 30%）
/// `atlas_uv`: 8 地形 UV 数组 (索引 = TerrainType::to_u8), M10.3 接 atlas 用
pub fn generate_chunk_mesh(
    terrains: &[u8; 1024],
    owners: &[u8; 1024],
    fog: &[u8; 1024],
    selected: &[u8; 1024],
    lod_level: u8,
    atlas_uv: &[[f32; 4]; 8],
) -> Mesh {
    match lod_level {
        0 => generate_full_mesh(terrains, owners, fog, selected, atlas_uv),
        1 => generate_merged_mesh(terrains, owners, fog, selected, 2, atlas_uv),
        2 => generate_merged_mesh(terrains, owners, fog, selected, 4, atlas_uv),
        3 => generate_minimap_mesh(terrains, owners, fog, selected, atlas_uv),
        _ => generate_full_mesh(terrains, owners, fog, selected, atlas_uv),
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
    selected: &[u8; 1024],
    atlas_uv: &[[f32; 4]; 8],
) -> Mesh {
    let base_mesh = generate_full_mesh(terrains, owners, fog, selected, atlas_uv);
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
    atlas_uv: Res<crate::render::embedded_atlas::AtlasUvRes>,
) {
    let mut rebuilt = 0;

    for (entity, mut chunk, _old_mesh) in query.iter_mut() {
        if !chunk.dirty || rebuilt >= MAX_REBUILDS_PER_FRAME {
            continue;
        }

        // Full LOD 时启用地形过渡渲染，更高 LOD 级别跳过过渡以节省性能
        let new_mesh = if chunk.current_lod == 0 {
            generate_chunk_mesh_with_transitions(
                &chunk.terrains,
                &chunk.owners,
                &chunk.fog,
                &chunk.selected,
                &atlas_uv.0,
            )
        } else {
            generate_chunk_mesh(
                &chunk.terrains,
                &chunk.owners,
                &chunk.fog,
                &chunk.selected,
                chunk.current_lod,
                &atlas_uv.0,
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

fn generate_full_mesh(
    terrains: &[u8; 1024],
    owners: &[u8; 1024],
    fog: &[u8; 1024],
    selected: &[u8; 1024],
    atlas_uv: &[[f32; 4]; 8],
) -> Mesh {
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
            let is_selected = selected[idx] != 0;

            let center = hex_center(row, col);
            // M10.3: 从 atlas_uv 按 terrain_id 索引 (默认 0 = Plains 全白 fallback)
            // uv: [u_min, v_min, u_max, v_max]
            // `atlas_uv` 是 `&[[f32; 4]; 8]`, `.get(i)` 给 `Option<&[f32; 4]>`,
            // `.copied()` 转成 `Option<[f32; 4]>` ([f32; 4] 是 Copy).
            let uv_rect = atlas_uv
                .get(terrain_id as usize)
                .copied()
                .unwrap_or([0.0, 0.0, 1.0, 1.0]); // fallback 整张图

            // M10.3.1 修复: vertex color 走 terrain_col (M9 那套), 不再走 faction_col
            //
            // 之前的 bug: vertex color = faction_col = Color::NONE (alpha 0) for unowned hex
            //   → material.color (WHITE) * mesh.color (0,0,0,1) * texture = 黑色
            //   → 整张图发黑
            //
            // 正确做法: vertex color = terrain_col (绿/灰/蓝...), 当作 tint 跟 atlas texture 叠加
            //   → 即使 atlas texture 没绑上, 也能看到地形色 (M9 行为)
            //   → atlas 绑上后, terrain tint × atlas art = 双重效果
            let terrain_col = atlas::terrain_color_from_u8(terrain_id);
            let tc = terrain_col.to_srgba();
            // 势力色 overlay (alpha 0.7 叠在 terrain 之上), 无主 = 不叠
            let faction_col = atlas::faction_color(owner_id);
            let fc = faction_col.to_srgba();
            let (r, g, b) = if fc.alpha > 0.0 {
                // terrain × (1 - 0.7) + faction × 0.7
                (
                    tc.red * (1.0 - fc.alpha) + fc.red * fc.alpha,
                    tc.green * (1.0 - fc.alpha) + fc.green * fc.alpha,
                    tc.blue * (1.0 - fc.alpha) + fc.blue * fc.alpha,
                )
            } else {
                (tc.red, tc.green, tc.blue)
            };
            let fog_alpha: f32 = if is_fogged { 0.55 } else { 1.0 };
            // 选中: 70% 上面 + 30% 金色
            let (r, g, b) = if is_selected {
                (r * 0.7 + 1.0 * 0.3, g * 0.7 + 0.84 * 0.3, b * 0.7)
            } else {
                (r, g, b)
            };
            let color_arr = [r, g, b, fog_alpha];

            let vert_start = positions.len() as u32;

            // 中心顶点: UV 在 tile 中心
            positions.push([center.x, center.y, 0.0]);
            normals.push([0.0, 0.0, 1.0]);
            uvs.push([
                (uv_rect[0] + uv_rect[2]) * 0.5,
                (uv_rect[1] + uv_rect[3]) * 0.5,
            ]);
            colors.push(color_arr);

            // 6 个角顶点（pointy-top：起始角 30 度）
            for i in 0..6 {
                let angle = std::f32::consts::FRAC_PI_3 * i as f32 + std::f32::consts::FRAC_PI_6;
                let x = center.x + HEX_SIZE * angle.cos();
                let y = center.y + HEX_SIZE * angle.sin();
                positions.push([x, y, 0.0]);
                normals.push([0.0, 0.0, 1.0]);
                // UV: 中心 + 0.5 * (cos, sin) * 范围, 加 0.5 (因为 hex mask 0.5 中心)
                uvs.push([
                    (uv_rect[0] + uv_rect[2]) * 0.5 + 0.5 * (uv_rect[2] - uv_rect[0]) * 0.5 * angle.cos(),
                    (uv_rect[1] + uv_rect[3]) * 0.5 + 0.5 * (uv_rect[3] - uv_rect[1]) * 0.5 * angle.sin(),
                ]);
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
    _selected: &[u8; 1024],
    merge_size: u32,
    _atlas_uv: &[[f32; 4]; 8],
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

fn generate_minimap_mesh(
    terrains: &[u8; 1024],
    owners: &[u8; 1024],
    fog: &[u8; 1024],
    selected: &[u8; 1024],
    atlas_uv: &[[f32; 4]; 8],
) -> Mesh {
    generate_merged_mesh(terrains, owners, fog, selected, 32, atlas_uv)
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

// ---------------------------------------------------------------------------
// M10.3 测试：chunk mesh 集成 atlas_uv
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// 8 地形测试用 atlas_uv（每个地形 1 个独立 32x32 tile, 1024x1024 排成 8 块）
    ///
    /// terrain_id i 占用 [i*128, (i+1)*128] 范围, 中心 32x32 tile 简化测试。
    fn fake_atlas_uv() -> [[f32; 4]; 8] {
        let mut arr = [[0.0f32; 4]; 8];
        for i in 0..8 {
            let base = i as f32 * 0.125; // 1024 → 8 tile 一字排开
            arr[i] = [base, 0.0, base + 0.03125, 0.03125]; // 32/1024 ≈ 0.03125
        }
        arr
    }

    /// TEST42: generate_full_mesh 用 atlas_uv 不 panic
    /// 验证: terrains 0..8 各填一种, mesh 生成成功, vertex count = 1024 * 7
    #[test]
    fn test_generate_full_mesh_with_atlas_uv() {
        let mut terrains = [0u8; 1024];
        // 8 地形各占 1/8 区域 (前 128 列为 terrain 0, 后 128 列为 terrain 1, ...)
        for i in 0..1024 {
            terrains[i] = (i / 128) as u8;
        }
        let owners = [0u8; 1024];
        let fog = [1u8; 1024];
        let selected = [0u8; 1024];
        let atlas_uv = fake_atlas_uv();

        let mesh = generate_full_mesh(&terrains, &owners, &fog, &selected, &atlas_uv);

        // 1024 hex * 7 vertices = 7168
        let pos_count = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .map(|a| a.len())
            .unwrap_or(0);
        assert_eq!(pos_count, 7168, "Full mesh vertex count");

        // UV 数量应一致
        let uv_count = mesh
            .attribute(Mesh::ATTRIBUTE_UV_0)
            .map(|a| a.len())
            .unwrap_or(0);
        assert_eq!(uv_count, 7168, "Full mesh UV count");
    }

    /// TEST43: UV 随 terrain_id 改变 (不会所有 hex 都用同一张图)
    /// 验证: 索引 0 (terrain 0) 中心 UV 在 arr[0] 范围, 索引 1023 (terrain 7) 中心 UV 在 arr[7] 范围
    #[test]
    fn test_uv_varies_per_terrain_id() {
        let mut terrains = [0u8; 1024];
        // 让 0..128 都用 terrain 0, 1024-128..1024 用 terrain 7
        for i in 0..128 {
            terrains[i] = 0;
        }
        for i in 896..1024 {
            terrains[i] = 7;
        }
        let owners = [0u8; 1024];
        let fog = [1u8; 1024];
        let selected = [0u8; 1024];
        let atlas_uv = fake_atlas_uv();

        let mesh = generate_full_mesh(&terrains, &owners, &fog, &selected, &atlas_uv);
        let uvs = mesh
            .attribute(Mesh::ATTRIBUTE_UV_0)
            .expect("UV attribute missing");

        use bevy::render::mesh::VertexAttributeValues;
        let uv_vec = match uvs {
            VertexAttributeValues::Float32x2(v) => v,
            _ => panic!("UV attribute wrong type"),
        };

        // 第一个 hex (index 0) 中心 vertex 是 uv_vec[0], 中心 UV = (arr[0][0]+arr[0][2])/2
        let terrain0_center_u = (atlas_uv[0][0] + atlas_uv[0][2]) * 0.5;
        let terrain7_center_u = (atlas_uv[7][0] + atlas_uv[7][2]) * 0.5;
        // 第一个 hex (terrain 0) 中心 U
        assert!(
            (uv_vec[0][0] - terrain0_center_u).abs() < 0.0001,
            "terrain 0 center U expected {}, got {}",
            terrain0_center_u,
            uv_vec[0][0]
        );
        // 最后一个 hex (index 1023, terrain 7) 中心 vertex 是 uv_vec[1023*7]
        let last_hex_first_vert = 1023 * 7;
        assert!(
            (uv_vec[last_hex_first_vert][0] - terrain7_center_u).abs() < 0.0001,
            "terrain 7 center U expected {}, got {}",
            terrain7_center_u,
            uv_vec[last_hex_first_vert][0]
        );
        // 验证两个 terrain UV 范围不同
        assert!(
            (terrain0_center_u - terrain7_center_u).abs() > 0.1,
            "terrain UV ranges should differ: t0={} t7={}",
            terrain0_center_u,
            terrain7_center_u
        );
    }

    /// TEST44: out-of-range terrain_id (>= 8) 走 fallback, 不 panic
    #[test]
    fn test_generate_full_mesh_out_of_range_terrain_fallback() {
        let mut terrains = [255u8; 1024]; // 全是 out-of-range terrain_id
        let owners = [0u8; 1024];
        let fog = [1u8; 1024];
        let selected = [0u8; 1024];
        let atlas_uv = fake_atlas_uv();

        let mesh = generate_full_mesh(&terrains, &owners, &fog, &selected, &atlas_uv);
        // 不 panic 即可; vertex count 仍是 7168
        let pos_count = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .map(|a| a.len())
            .unwrap_or(0);
        assert_eq!(pos_count, 7168);
    }

    /// TEST47 (M10.3.1 修复): unowned hex 的 vertex color 应是 terrain_col (绿/灰/蓝...),
    /// 不应是 Color::NONE (黑) — 之前的 bug 导致整张图发黑
    #[test]
    fn test_unowned_hex_color_is_terrain_not_black() {
        let mut terrains = [0u8; 1024]; // 全 Plains
        for i in 0..1024 {
            terrains[i] = (i % 8) as u8; // 8 地形各占 1/8
        }
        let owners = [0u8; 1024]; // 全 unowned
        let fog = [1u8; 1024];
        let selected = [0u8; 1024];
        let atlas_uv = fake_atlas_uv();

        let mesh = generate_full_mesh(&terrains, &owners, &fog, &selected, &atlas_uv);
        let colors = mesh
            .attribute(Mesh::ATTRIBUTE_COLOR)
            .expect("COLOR attribute missing");

        use bevy::render::mesh::VertexAttributeValues;
        let col_vec = match colors {
            VertexAttributeValues::Float32x4(v) => v,
            _ => panic!("COLOR wrong type"),
        };

        // Plains (terrain 0) 是 (0.4, 0.7, 0.3). 第 0 个 hex 是 Plains.
        // unowned + fog + not selected → 应该就是 (0.4, 0.7, 0.3, 1.0)
        let plains_color = col_vec[0]; // hex 0 中心 vertex
        // 允许一点浮点误差
        assert!(
            (plains_color[0] - 0.4).abs() < 0.01,
            "Plains R 应是 0.4, got {}",
            plains_color[0]
        );
        assert!(
            (plains_color[1] - 0.7).abs() < 0.01,
            "Plains G 应是 0.7, got {}",
            plains_color[1]
        );
        assert!(
            (plains_color[2] - 0.3).abs() < 0.01,
            "Plains B 应是 0.3, got {}",
            plains_color[2]
        );
        assert!(
            plains_color[3] > 0.9,
            "un-fogged alpha 应是 1.0, got {}",
            plains_color[3]
        );

        // 关键检查: 不应是纯黑 (0, 0, 0) — 那就是之前 faction_col = Color::NONE 的 bug
        assert!(
            plains_color[0] > 0.1 || plains_color[1] > 0.1 || plains_color[2] > 0.1,
            "Plains color 不是全黑: ({}, {}, {}) — 之前 M10.3 bug 是 0,0,0",
            plains_color[0], plains_color[1], plains_color[2]
        );
    }

    /// TEST48 (M10.3.1 修复): owned hex 的 vertex color = terrain × (1-α) + faction × α
    /// owner=1 是蓝色, alpha=0.7; Plains 是 (0.4, 0.7, 0.3)
    /// 期望: (0.4*0.3 + 0.1*0.7, 0.7*0.3 + 0.3*0.7, 0.3*0.3 + 0.95*0.7)
    ///     = (0.19, 0.42, 0.755)
    #[test]
    fn test_owned_hex_blends_terrain_with_faction() {
        let terrains = [0u8; 1024]; // 全 Plains
        let mut owners = [0u8; 1024];
        owners[0] = 1; // 第 0 个 hex = Plains + 魏(blue)
        let fog = [1u8; 1024];
        let selected = [0u8; 1024];
        let atlas_uv = fake_atlas_uv();

        let mesh = generate_full_mesh(&terrains, &owners, &fog, &selected, &atlas_uv);
        let colors = mesh
            .attribute(Mesh::ATTRIBUTE_COLOR)
            .expect("COLOR attribute missing");

        use bevy::render::mesh::VertexAttributeValues;
        let col_vec = match colors {
            VertexAttributeValues::Float32x4(v) => v,
            _ => panic!("COLOR wrong type"),
        };

        // 第 0 个 hex (Plains + 魏) 中心 vertex
        let owned_color = col_vec[0];
        // 期望 (0.19, 0.42, 0.755, 1.0)
        assert!(
            (owned_color[0] - 0.19).abs() < 0.01,
            "Plains+魏 R 应是 0.19, got {}",
            owned_color[0]
        );
        assert!(
            (owned_color[2] - 0.755).abs() < 0.02,
            "Plains+魏 B 应是 0.755 (蓝色强), got {}",
            owned_color[2]
        );

        // 第 1 个 hex (Plains + unowned) 应该跟 unowned Plains 一致
        let unowned_color = col_vec[7]; // hex 1 中心 vertex 是 col_vec[1*7 + 0] = col_vec[7]
        assert!(
            (unowned_color[0] - 0.4).abs() < 0.01,
            "Plains+unowned R 应是 0.4 (terrain), got {}",
            unowned_color[0]
        );
    }

    /// TEST49: fogged hex 的 alpha 是 0.55, 不是 1.0
    #[test]
    fn test_fogged_hex_alpha_is_055() {
        let terrains = [0u8; 1024];
        let owners = [0u8; 1024];
        let mut fog = [1u8; 1024];
        fog[0] = 0; // hex 0 = 黑雾
        let selected = [0u8; 1024];
        let atlas_uv = fake_atlas_uv();

        let mesh = generate_full_mesh(&terrains, &owners, &fog, &selected, &atlas_uv);
        let colors = mesh
            .attribute(Mesh::ATTRIBUTE_COLOR)
            .expect("COLOR attribute missing");

        use bevy::render::mesh::VertexAttributeValues;
        let col_vec = match colors {
            VertexAttributeValues::Float32x4(v) => v,
            _ => panic!("COLOR wrong type"),
        };

        // hex 0 中心 vertex
        let fogged = col_vec[0];
        assert!(
            (fogged[3] - 0.55).abs() < 0.01,
            "fogged alpha 应是 0.55, got {}",
            fogged[3]
        );
    }
}
