//! 地形过渡渲染（Autotiling）
//!
//! M2-T13：为相邻不同地形之间的六边形边缘添加过渡几何体，
//! 使用颜色渐变实现平滑过渡，避免硬切割。
//!
//! 设计思路：
//! - 预定义过渡规则表，指定哪些地形对之间需要过渡、过渡类型及优先级
//! - 对每个 hex 的 6 条边缘，检查相邻 hex 的地形类型
//! - 若需要过渡，在边缘处生成过渡三角形，顶点颜色在两种地形色之间插值
//! - 过渡几何体以 z = 0.001 叠加在基础 mesh 之上

use bevy::prelude::*;
use bevy::render::mesh::{Indices, Mesh, PrimitiveTopology};
use bevy::render::render_asset::RenderAssetUsages;

use crate::render::atlas;
use crate::render::chunk_mesh;

// ---------------------------------------------------------------------------
// 过渡类型与规则
// ---------------------------------------------------------------------------

/// 过渡类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionType {
    /// 硬边（无过渡）
    Hard,
    /// 渐变过渡（50% 混合）
    Gradient,
    /// 纹理混合（30% 混合，更接近中心色）
    Blend,
}

/// 过渡规则
///
/// 定义两种地形之间的过渡方式和优先级。
/// 当两个相邻 hex 之间存在匹配的规则时，按规则生成过渡效果。
#[derive(Debug, Clone)]
pub struct TransitionRule {
    pub from_terrain: u8,
    pub to_terrain: u8,
    pub transition_type: TransitionType,
    /// 优先级（数值越大越优先）
    pub priority: u8,
}

/// 获取地形过渡规则表
///
/// 规则设计原则：
/// - 水域与陆地之间使用渐变过渡（优先级最高）
/// - 不同植被地形之间使用纹理混合
/// - 山地与平原之间使用渐变
/// - 未列出的组合默认无过渡（硬边）
pub fn get_transition_rules() -> Vec<TransitionRule> {
    vec![
        // 水域(2) 与 平原(0)：渐变
        TransitionRule {
            from_terrain: 2,
            to_terrain: 0,
            transition_type: TransitionType::Gradient,
            priority: 10,
        },
        // 水域(2) 与 森林(3)：渐变
        TransitionRule {
            from_terrain: 2,
            to_terrain: 3,
            transition_type: TransitionType::Gradient,
            priority: 10,
        },
        // 水域(2) 与 沙漠(4)：渐变
        TransitionRule {
            from_terrain: 2,
            to_terrain: 4,
            transition_type: TransitionType::Gradient,
            priority: 10,
        },
        // 水域(2) 与 沼泽(5)：渐变
        TransitionRule {
            from_terrain: 2,
            to_terrain: 5,
            transition_type: TransitionType::Gradient,
            priority: 10,
        },
        // 平原(0) 与 森林(3)：纹理混合
        TransitionRule {
            from_terrain: 0,
            to_terrain: 3,
            transition_type: TransitionType::Blend,
            priority: 5,
        },
        // 山地(1) 与 平原(0)：渐变
        TransitionRule {
            from_terrain: 1,
            to_terrain: 0,
            transition_type: TransitionType::Gradient,
            priority: 8,
        },
        // 山地(1) 与 森林(3)：纹理混合
        TransitionRule {
            from_terrain: 1,
            to_terrain: 3,
            transition_type: TransitionType::Blend,
            priority: 5,
        },
        // 平原(0) 与 沙漠(4)：纹理混合
        TransitionRule {
            from_terrain: 0,
            to_terrain: 4,
            transition_type: TransitionType::Blend,
            priority: 5,
        },
        // 平原(0) 与 丘陵(6)：纹理混合
        TransitionRule {
            from_terrain: 0,
            to_terrain: 6,
            transition_type: TransitionType::Blend,
            priority: 5,
        },
        // 平原(0) 与 沼泽(5)：纹理混合
        TransitionRule {
            from_terrain: 0,
            to_terrain: 5,
            transition_type: TransitionType::Blend,
            priority: 5,
        },
    ]
}

/// 查找两个地形之间的过渡规则
///
/// 规则双向匹配（from/to 可互换），返回优先级最高的规则。
/// 返回 None 表示无需过渡（硬边）。
fn find_transition_rule(a: u8, b: u8) -> Option<TransitionRule> {
    get_transition_rules()
        .into_iter()
        .filter(|r| {
            (r.from_terrain == a && r.to_terrain == b) || (r.from_terrain == b && r.to_terrain == a)
        })
        .max_by_key(|r| r.priority)
}

// ---------------------------------------------------------------------------
// 过渡颜色计算
// ---------------------------------------------------------------------------

/// 计算 hex 边缘的过渡颜色
///
/// 对于 hex 的每条边缘，检查相邻 hex 的地形类型，
/// 根据过渡规则计算边缘的混合颜色。
///
/// 返回 None 表示不需要过渡（相同地形或无匹配规则）。
pub fn calculate_edge_color(
    center_terrain: u8,
    neighbor_terrain: u8,
    center_color: Color,
    neighbor_color: Color,
) -> Option<Color> {
    if center_terrain == neighbor_terrain {
        return None;
    }

    let rule = find_transition_rule(center_terrain, neighbor_terrain)?;

    let blend_factor = match rule.transition_type {
        TransitionType::Hard => return None,
        TransitionType::Gradient => 0.5,
        TransitionType::Blend => 0.3,
    };

    Some(blend_colors(center_color, neighbor_color, blend_factor))
}

/// 根据两个地形类型和过渡规则生成过渡颜色
///
/// 供 mesh 生成使用。返回 None 表示无需过渡。
pub fn generate_transition_color(terrain_a: u8, terrain_b: u8) -> Option<Color> {
    if terrain_a == terrain_b {
        return None;
    }

    let rule = find_transition_rule(terrain_a, terrain_b)?;

    let blend_factor = match rule.transition_type {
        TransitionType::Hard => return None,
        TransitionType::Gradient => 0.5,
        TransitionType::Blend => 0.3,
    };

    let color_a = atlas::terrain_color_from_u8(terrain_a);
    let color_b = atlas::terrain_color_from_u8(terrain_b);
    Some(blend_colors(color_a, color_b, blend_factor))
}

/// 颜色混合
///
/// 在颜色 a 和 b 之间按因子 t 插值。t=0 返回 a，t=1 返回 b。
fn blend_colors(a: Color, b: Color, t: f32) -> Color {
    let sa = a.to_srgba();
    let sb = b.to_srgba();
    Color::srgba(
        sa.red * (1.0 - t) + sb.red * t,
        sa.green * (1.0 - t) + sb.green * t,
        sa.blue * (1.0 - t) + sb.blue * t,
        sa.alpha * (1.0 - t) + sb.alpha * t,
    )
}

/// Color 转 [f32; 4]（Srgba 顺序）
fn color_to_f32_4(c: Color) -> [f32; 4] {
    let s = c.to_srgba();
    [s.red, s.green, s.blue, s.alpha]
}

// ---------------------------------------------------------------------------
// 六边形边缘邻居计算（pointy-top offset 坐标系）
// ---------------------------------------------------------------------------

/// 6 个方向的邻居偏移量（偶数行）
///
/// 方向顺序：东、东北、西北、西、西南、东南
const EVEN_ROW_NEIGHBORS: [(i32, i32); 6] = [
    (0, 1),  // 东
    (-1, 1), // 东北（偶数行）
    (-1, 0), // 西北（偶数行）
    (0, -1), // 西
    (1, -1), // 西南（偶数行）
    (1, 0),  // 东南（偶数行）
];

/// 6 个方向的邻居偏移量（奇数行）
///
/// 方向顺序：东、东北、西北、西、西南、东南
const ODD_ROW_NEIGHBORS: [(i32, i32); 6] = [
    (0, 1),   // 东
    (-1, 0),  // 东北（奇数行）
    (-1, -1), // 西北（奇数行）
    (0, -1),  // 西
    (1, 0),   // 西南（奇数行）
    (1, 1),   // 东南（奇数行）
];

/// 获取指定方向的邻居坐标
///
/// 对于 pointy-top offset 坐标系，偶数行和奇数行的邻居偏移不同。
/// 返回 None 表示邻居超出 chunk 边界（0..31）。
fn get_neighbor(row: i32, col: i32, dir: usize) -> Option<(usize, usize)> {
    let (dr, dc) = if row % 2 == 0 {
        EVEN_ROW_NEIGHBORS[dir]
    } else {
        ODD_ROW_NEIGHBORS[dir]
    };

    let nr = row + dr;
    let nc = col + dc;

    if (0..32).contains(&nr) && (0..32).contains(&nc) {
        Some((nr as usize, nc as usize))
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// 过渡 Mesh 生成
// ---------------------------------------------------------------------------

/// 生成过渡几何体 mesh
///
/// 遍历 chunk 中每个 hex 的 6 条边缘，若相邻 hex 地形不同且存在过渡规则，
/// 则在该边缘生成两个过渡三角形，顶点颜色在两种地形色之间插值。
///
/// 过渡三角形使用 z = 0.001 叠加在基础 mesh 之上。
pub fn generate_transition_mesh(terrains: &[u8; 1024]) -> Mesh {
    // 预估容量：最坏情况每 hex 6 条边 * 2 三角形 * 3 顶点 = 36 顶点
    // 实际远少于此，但预分配避免频繁扩容
    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(1024 * 12);
    let mut normals: Vec<[f32; 3]> = Vec::with_capacity(1024 * 12);
    let mut uvs: Vec<[f32; 2]> = Vec::with_capacity(1024 * 12);
    let mut colors: Vec<[f32; 4]> = Vec::with_capacity(1024 * 12);
    let mut indices: Vec<u32> = Vec::with_capacity(1024 * 36);

    for row in 0i32..32 {
        for col in 0i32..32 {
            let idx = (row * 32 + col) as usize;
            let center_terrain = terrains[idx];
            let center = chunk_mesh::hex_center(row as u32, col as u32);

            // 6 个角顶点（pointy-top：起始角 30 度）
            let corners: [Vec2; 6] = std::array::from_fn(|i| {
                let angle = std::f32::consts::FRAC_PI_3 * i as f32 + std::f32::consts::FRAC_PI_6;
                Vec2::new(
                    center.x + chunk_mesh::HEX_SIZE * angle.cos(),
                    center.y + chunk_mesh::HEX_SIZE * angle.sin(),
                )
            });

            // 检查每条边缘（边 i 连接角 i 和角 (i+1)%6）
            for dir in 0..6usize {
                let Some((nr, nc)) = get_neighbor(row, col, dir) else {
                    continue;
                };

                let neighbor_idx = nr * 32 + nc;
                let neighbor_terrain = terrains[neighbor_idx];

                // 计算过渡颜色；若无需过渡则跳过
                let Some(transition_color) =
                    generate_transition_color(center_terrain, neighbor_terrain)
                else {
                    continue;
                };

                let tc = color_to_f32_4(transition_color);

                // 边的两个端点
                let edge_a = corners[dir];
                let edge_b = corners[(dir + 1) % 6];
                // 边的中点
                let edge_mid = Vec2::new((edge_a.x + edge_b.x) / 2.0, (edge_a.y + edge_b.y) / 2.0);
                // 向中心内缩的点（边缘到中心的 1/3 处）
                let inner = Vec2::new(
                    center.x + (edge_mid.x - center.x) / 3.0,
                    center.y + (edge_mid.y - center.y) / 3.0,
                );

                let vert_start = positions.len() as u32;

                // 三角形 1：edge_a, edge_mid, inner
                positions.push([edge_a.x, edge_a.y, 0.001]);
                normals.push([0.0, 0.0, 1.0]);
                uvs.push([0.0, 0.0]);
                colors.push(tc);

                positions.push([edge_mid.x, edge_mid.y, 0.001]);
                normals.push([0.0, 0.0, 1.0]);
                uvs.push([0.5, 0.0]);
                colors.push(tc);

                positions.push([inner.x, inner.y, 0.001]);
                normals.push([0.0, 0.0, 1.0]);
                uvs.push([0.25, 1.0]);
                colors.push(tc);

                // 三角形 2：edge_mid, edge_b, inner
                positions.push([edge_mid.x, edge_mid.y, 0.001]);
                normals.push([0.0, 0.0, 1.0]);
                uvs.push([0.5, 0.0]);
                colors.push(tc);

                positions.push([edge_b.x, edge_b.y, 0.001]);
                normals.push([0.0, 0.0, 1.0]);
                uvs.push([1.0, 0.0]);
                colors.push(tc);

                positions.push([inner.x, inner.y, 0.001]);
                normals.push([0.0, 0.0, 1.0]);
                uvs.push([0.75, 1.0]);
                colors.push(tc);

                // 三角形 1：vert 0, 1, 2
                indices.push(vert_start);
                indices.push(vert_start + 1);
                indices.push(vert_start + 2);
                // 三角形 2：vert 3, 4, 5
                indices.push(vert_start + 3);
                indices.push(vert_start + 4);
                indices.push(vert_start + 5);
            }
        }
    }

    build_transition_mesh(positions, normals, uvs, colors, indices)
}

/// 构建过渡 Mesh
fn build_transition_mesh(
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
