//! 资源与土地等级生成
//!
//! 土地等级：圈层梯度 + 噪声扰动（率土式由外到内 4 圈）。
//! 资源点：按地形掩码 + 等级概率约束放置。

use std::collections::BTreeMap;

use crate::map::grid::HexCoord;
use crate::map::tile::TerrainType;
use rand::Rng;
use rand_chacha::ChaCha12Rng;
use slg_data::ids::TileKey;
use slg_data::map_doc::ResourceEntry;

// ---------------------------------------------------------------------------
// 土地等级
// ---------------------------------------------------------------------------

/// 生成土地等级（圈层梯度 + 噪声扰动）
///
/// 率土式 4 圈层：
/// - 最外圈 (0.75~1.0 归一化距离)：等级 1~3
/// - 次外圈 (0.50~0.75)：等级 3~5
/// - 次内圈 (0.25~0.50)：等级 5~7
/// - 最内圈 (0.00~0.25)：等级 7~9
///
/// 噪声扰动 +/-2，中心富饶但有贫瘠缝隙，边缘偶有飞地。
pub fn generate_tile_levels(
    rng: &mut ChaCha12Rng,
    width: u32,
    height: u32,
    terrain: &[TerrainType],
    _heightmap: &[f64],
) -> Vec<u8> {
    let center_x = width as f64 / 2.0;
    let center_y = height as f64 / 2.0;
    // 使用对角线长度做归一化
    let max_dist = (center_x * center_x + center_y * center_y).sqrt();

    let mut levels = vec![1u8; (width * height) as usize];

    // 生成噪声扰动场
    let noise = generate_noise_field(rng, width, height);

    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) as usize;
            if terrain[idx] == TerrainType::Water {
                levels[idx] = 0;
                continue;
            }

            // 归一化距离（0.0=中心，1.0=角落）
            let dx = x as f64 - center_x;
            let dy = y as f64 - center_y;
            let dist = (dx * dx + dy * dy).sqrt() / max_dist;

            // 圈层基础等级
            let base_level: i32 = if dist < 0.25 {
                8
            } else if dist < 0.50 {
                6
            } else if dist < 0.75 {
                4
            } else {
                2
            };

            // 噪声扰动 +/-2
            let perturbation = ((noise[idx] - 0.5) * 4.0).round() as i32; // [-2, +2]
            let level = (base_level + perturbation).clamp(1, 9) as u8;
            levels[idx] = level;
        }
    }

    levels
}

/// 生成一个简单的噪声场（用于等级扰动）
fn generate_noise_field(rng: &mut ChaCha12Rng, width: u32, height: u32) -> Vec<f64> {
    let mut field = vec![0.0f64; (width * height) as usize];
    // 使用 3 个不同频率的正弦叠加模拟低频噪声
    let seed1: f64 = rng.gen::<f64>() * 1000.0;
    let seed2: f64 = rng.gen::<f64>() * 1000.0;
    let seed3: f64 = rng.gen::<f64>() * 1000.0;

    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) as usize;
            let fx = x as f64;
            let fy = y as f64;
            let v1 = ((fx * 0.03 + seed1).sin() * (fy * 0.03 + seed2).cos() + 1.0) * 0.5;
            let v2 = ((fx * 0.07 + seed2).cos() * (fy * 0.05 + seed3).sin() + 1.0) * 0.25;
            let v3 = ((fx * 0.12 + seed3).sin() * (fy * 0.11 + seed1).cos() + 1.0) * 0.125;
            field[idx] = (v1 + v2 + v3).clamp(0.0, 1.0);
        }
    }
    field
}

// ---------------------------------------------------------------------------
// 资源点
// ---------------------------------------------------------------------------

/// 资源点分布：地形掩码 + 等级概率约束
///
/// 资源类型与地形绑定：
/// - Mountain -> Iron
/// - Hills -> Stone
/// - Forest -> Wood
/// - Plains -> Food
/// - 其余 -> Gold（稀有）
///
/// 放置概率与土地等级正相关，整体密度受 `richness` 控制。
pub fn generate_resources(
    rng: &mut ChaCha12Rng,
    width: u32,
    height: u32,
    terrain: &[TerrainType],
    levels: &[u8],
    richness: f64,
) -> BTreeMap<TileKey, ResourceEntry> {
    let mut resources = BTreeMap::new();

    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) as usize;

            // 水域跳过
            if terrain[idx] == TerrainType::Water || levels[idx] == 0 {
                continue;
            }

            // 等级越高，资源概率越大
            let level_factor = levels[idx] as f64 / 9.0; // 0.11 ~ 1.0
            let base_chance = 0.04 * richness; // 基础概率 4% * richness
            let chance = base_chance * level_factor;

            if rng.gen::<f64>() < chance {
                let resource_type = terrain_to_resource(terrain[idx]);
                let key = HexCoord::new(x as i32, y as i32).to_tile_key();
                resources.insert(
                    key,
                    ResourceEntry {
                        resource_type: resource_type.to_string(),
                        level: levels[idx],
                    },
                );
            }
        }
    }

    resources
}

/// 地形到资源类型的映射
fn terrain_to_resource(terrain: TerrainType) -> &'static str {
    match terrain {
        TerrainType::Mountain => "iron",
        TerrainType::Hills => "stone",
        TerrainType::Forest => "wood",
        TerrainType::Plains => "food",
        TerrainType::Desert => "gold",
        TerrainType::Swamp => "food",
        TerrainType::Pass => "iron",
        TerrainType::Water => "food", // 不应到达
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gen::terrain::generate_heightmap;
    use rand::SeedableRng;

    #[test]
    fn test_tile_levels_deterministic() {
        let h = generate_heightmap(42, 32, 32, 0.5);
        let terrain: Vec<TerrainType> = h
            .iter()
            .map(|v| {
                if *v < 0.30 {
                    TerrainType::Water
                } else if *v > 0.78 {
                    TerrainType::Mountain
                } else {
                    TerrainType::Plains
                }
            })
            .collect();

        let mut rng1 = ChaCha12Rng::seed_from_u64(42);
        let l1 = generate_tile_levels(&mut rng1, 32, 32, &terrain, &h);
        let mut rng2 = ChaCha12Rng::seed_from_u64(42);
        let l2 = generate_tile_levels(&mut rng2, 32, 32, &terrain, &h);
        assert_eq!(l1, l2);
    }

    #[test]
    fn test_tile_levels_range() {
        let h = generate_heightmap(42, 32, 32, 0.5);
        let terrain: Vec<TerrainType> = h
            .iter()
            .map(|v| {
                if *v < 0.30 {
                    TerrainType::Water
                } else {
                    TerrainType::Plains
                }
            })
            .collect();

        let mut rng = ChaCha12Rng::seed_from_u64(42);
        let levels = generate_tile_levels(&mut rng, 32, 32, &terrain, &h);
        for (i, l) in levels.iter().enumerate() {
            if terrain[i] == TerrainType::Water {
                assert_eq!(*l, 0);
            } else {
                assert!(*l >= 1 && *l <= 9, "level out of range: {l} at {i}");
            }
        }
    }

    #[test]
    fn test_resources_deterministic() {
        let h = generate_heightmap(42, 32, 32, 0.5);
        let terrain: Vec<TerrainType> = h
            .iter()
            .map(|v| {
                if *v < 0.30 {
                    TerrainType::Water
                } else {
                    TerrainType::Plains
                }
            })
            .collect();
        let mut rng = ChaCha12Rng::seed_from_u64(42);
        let levels = generate_tile_levels(&mut rng, 32, 32, &terrain, &h);

        let mut rng1 = ChaCha12Rng::seed_from_u64(99);
        let r1 = generate_resources(&mut rng1, 32, 32, &terrain, &levels, 0.5);
        let mut rng2 = ChaCha12Rng::seed_from_u64(99);
        let r2 = generate_resources(&mut rng2, 32, 32, &terrain, &levels, 0.5);
        assert_eq!(r1.len(), r2.len());
        for (k, v) in &r1 {
            assert_eq!(r2.get(k), Some(v));
        }
    }
}
