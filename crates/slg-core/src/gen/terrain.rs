//! 地形生成：高程图 + 湿度图 + 温度图 -> 地形分类
//!
//! 使用 Simplex fBm 噪声生成连续场，再通过查表离散化为 TerrainType。
//! 所有噪声种子从主种子派生，保证同种子同地图。

use std::collections::VecDeque;

use crate::map::grid::HexCoord;
use crate::map::tile::TerrainType;
use noise::{NoiseFn, OpenSimplex, Simplex, Seedable};
use rand::Rng;
use rand::SeedableRng;
use rand_chacha::ChaCha12Rng;

// ---------------------------------------------------------------------------
// 高程图
// ---------------------------------------------------------------------------

/// fBm 噪声采样（手动实现，避免 noise crate Fbm 兼容问题）
/// 注意：x, y 应为原始像素坐标（非归一化），frequency 控制噪声频率
fn fbm_noise(noise: &Simplex, x: f64, y: f64, octaves: u32, frequency: f64, lacunarity: f64, persistence: f64) -> f64 {
    let mut total = 0.0;
    let mut amplitude = 1.0;
    let mut freq = frequency;
    let mut max_amplitude = 0.0;

    for _ in 0..octaves {
        total += noise.get([x * freq, y * freq]) * amplitude;
        max_amplitude += amplitude;
        amplitude *= persistence;
        freq *= lacunarity;
    }

    total / max_amplitude
}

/// 高程图生成
///
/// Simplex fBm (6 octave) + Domain Warping，值域映射到 0.0..1.0。
/// `terrain_style` (0.0~1.0) 通过偏移海平面阈值控制水域/陆地比例：
/// - 0.0 = 更多水域，1.0 = 更多陆地。
pub fn generate_heightmap(seed: u64, width: u32, height: u32, terrain_style: f64) -> Vec<f64> {
    // 派生噪声种子
    let mut rng = ChaCha12Rng::seed_from_u64(seed);
    let noise_seed: u32 = rng.gen();

    // 主噪声
    let main_noise = Simplex::new(noise_seed);

    // Domain Warp 噪声（独立种子）
    let warp_x_noise = Simplex::new(noise_seed.wrapping_add(1));
    let warp_y_noise = Simplex::new(noise_seed.wrapping_add(2));

    let warp_strength = 80.0;

    let mut heights = vec![0.0f64; (width * height) as usize];

    // 噪声频率：控制地形特征大小
    // 频率越高，地形越碎；频率越低，地形越平滑
    let base_freq = 0.015;
    let warp_freq = 0.008;

    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) as usize;
            let fx = x as f64;
            let fy = y as f64;

            // Domain warping（用原始坐标，不归一化）
            let wx = fbm_noise(&warp_x_noise, fx, fy, 4, warp_freq, 2.0, 0.5) * warp_strength;
            let wy = fbm_noise(&warp_y_noise, fx, fy, 4, warp_freq, 2.0, 0.5) * warp_strength;

            let sample_x = fx + wx;
            let sample_y = fy + wy;

            // fBm 采样，噪声输出约 [-1, 1] -> 归一化到 [0, 1]
            let raw = fbm_noise(&main_noise, sample_x, sample_y, 6, base_freq, 2.0, 0.5);
            let normalized = (raw + 1.0) * 0.5;

            // terrain_style 偏移：正值抬高整体高程（更多陆地）
            let biased = (normalized + (terrain_style - 0.5) * 0.15).clamp(0.0, 1.0);

            heights[idx] = biased;
        }
    }

    heights
}

// ---------------------------------------------------------------------------
// 湿度图
// ---------------------------------------------------------------------------

/// 湿度图生成
///
/// 独立 Simplex 噪声通道 + 距水源衰减。
/// 水域格 (height < water_level) 视为水源，附近格湿度偏高。
pub fn generate_moisturemap(seed: u64, width: u32, height: u32, heightmap: &[f64]) -> Vec<f64> {
    let mut rng = ChaCha12Rng::seed_from_u64(seed);
    let noise_seed: u32 = rng.gen();

    let noise = Simplex::new(noise_seed.wrapping_add(100));

    let water_level = 0.30;

    // 先采样基础噪声（用原始坐标，不归一化）
    let mut moisture = vec![0.0f64; (width * height) as usize];
    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) as usize;
            let raw = fbm_noise(&noise, x as f64, y as f64, 4, 0.012, 2.0, 0.5);
            moisture[idx] = (raw + 1.0) * 0.5; // [0, 1]
        }
    }

    // 距水源衰减：BFS 多波前传播，距离越远衰减越大
    // 优化：用 BFS 距离场替代暴力搜索，从 O(n * r^2) 降至 O(n)
    let search_radius: i32 = 12;
    let total = (width * height) as usize;
    let mut dist_field = vec![search_radius + 1i32; total];
    let mut queue: VecDeque<(i32, i32)> = VecDeque::new();

    // 初始化水源格（距离=0）
    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) as usize;
            if heightmap[idx] < water_level {
                dist_field[idx] = 0;
                queue.push_back((x as i32, y as i32));
            }
        }
    }

    // BFS 传播：计算每个格到最近水源的 hex 距离
    while let Some((cx, cy)) = queue.pop_front() {
        let current_idx = (cy as u32 * width + cx as u32) as usize;
        let current_dist = dist_field[current_idx];

        if current_dist >= search_radius {
            continue;
        }

        // 6 hex 邻域
        let coord = HexCoord::new(cx, cy);
        for neighbor in coord.neighbors() {
            let nx = neighbor.q;
            let ny = neighbor.r;
            if nx >= 0 && ny >= 0 && (nx as u32) < width && (ny as u32) < height {
                let nidx = (ny as u32 * width + nx as u32) as usize;
                let new_dist = current_dist + 1;
                if new_dist < dist_field[nidx] {
                    dist_field[nidx] = new_dist;
                    queue.push_back((nx, ny));
                }
            }
        }
    }

    // 根据距离场叠加湿度加成
    let radius_f = search_radius as f64;
    for idx in 0..total {
        let d = dist_field[idx];
        if d <= search_radius {
            let bonus = 0.4 * (1.0 - d as f64 / radius_f);
            moisture[idx] = (moisture[idx] + bonus).min(1.0);
        }
    }

    moisture
}

// ---------------------------------------------------------------------------
// 温度图
// ---------------------------------------------------------------------------

/// 温度图生成
///
/// 纬度梯度（南北冷、中间热）+ 海拔衰减。
/// NOTE: Reserved for future use (e.g., biome generation, seasonal effects).
/// Currently unused in terrain classification.
pub fn generate_temperaturemap(seed: u64, width: u32, height: u32, heightmap: &[f64]) -> Vec<f64> {
    let mut rng = ChaCha12Rng::seed_from_u64(seed);
    let noise_seed: u32 = rng.gen();

    let noise_gen = Simplex::new(noise_seed.wrapping_add(200));

    let scale_x = width as f64;
    let scale_y = height as f64;
    let mut temperature = vec![0.0f64; (width * height) as usize];

    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) as usize;

            // 纬度梯度：y=0 (北) 和 y=height (南) 最冷，中间最热
            let lat = y as f64 / scale_y;
            let lat_temp = 1.0 - (2.0 * lat - 1.0).abs(); // 三角形：0 在两端, 1 在中间

            // 海拔衰减：每 0.1 高程衰减约 0.12
            let alt_penalty = heightmap[idx] * 1.2;

            // 微扰动（用原始坐标，不归一化）
            let noise = fbm_noise(&noise_gen, x as f64, y as f64, 3, 0.01, 2.0, 0.4) * 0.1;

            temperature[idx] = (lat_temp - alt_penalty + noise + 0.5).clamp(0.0, 1.0);
        }
    }

    temperature
}

// ---------------------------------------------------------------------------
// 地形分类
// ---------------------------------------------------------------------------

/// 地形分类：高程 + 湿度 -> TerrainType
///
/// 使用阈值查表，阈值与率土风格对齐：
/// - 高水域比例（~30% 海洋）形成岛屿格局
/// - 山地、森林、沙漠、沼泽按高程x湿度组合自然分布
/// - Pass（关隘）仅由编辑器/手动放置设定，不在此函数中自动生成。
pub fn classify_terrain(height: f64, moisture: f64) -> TerrainType {
    // 高程阈值（从低到高）：水 < 0.30, 平原 < 0.55, 丘陵 < 0.70, 山地 >= 0.70
    // 湿度影响次要地形：
    //   - 高湿度+低高程 -> 沼泽
    //   - 低湿度+中高程 -> 沙漠
    //   - 高湿度+中高程 -> 森林

    if height < 0.30 {
        return TerrainType::Water;
    }
    if height >= 0.78 {
        return TerrainType::Mountain;
    }
    if height >= 0.65 {
        return if moisture > 0.6 {
            TerrainType::Forest
        } else {
            TerrainType::Hills
        };
    }
    if height <= 0.40 && moisture > 0.75 {
        return TerrainType::Swamp;
    }
    if height > 0.40 && height < 0.55 && moisture < 0.25 {
        return TerrainType::Desert;
    }
    if (0.55..0.65).contains(&height) {
        return if moisture > 0.5 {
            TerrainType::Forest
        } else {
            TerrainType::Hills
        };
    }
    TerrainType::Plains
}

/// 将高程图+湿度图转为 TerrainType 数组
pub fn classify_all(heightmap: &[f64], moisturemap: &[f64]) -> Vec<TerrainType> {
    heightmap
        .iter()
        .zip(moisturemap.iter())
        .map(|(h, m)| classify_terrain(*h, *m))
        .collect()
}

// ---------------------------------------------------------------------------
// 河流后处理
// ---------------------------------------------------------------------------

/// 河流后处理：山脊源头 -> 最陡梯度下降 -> 注地成湖
///
/// 从高程局部极大值点出发，沿最陡下降方向标记河流格。
/// 当下降至洼地（四周都更高）时，将洼地标记为水域（湖泊）。
pub fn carve_rivers(
    rng: &mut ChaCha12Rng,
    width: u32,
    height: u32,
    heightmap: &[f64],
    terrain: &mut [TerrainType],
) {
    // 找山脊源头：高程 > 0.70 且是局部极大值
    // NOTE: The source threshold (0.70) is intentionally lower than the Mountain
    // classification threshold (0.78). This is by design: river sources should
    // originate from hills and high ground, not only from mountain peaks. This
    // produces more natural-looking river networks that flow from elevated terrain
    // through valleys to the sea.
    let mut sources: Vec<(u32, u32)> = Vec::new();
    for y in 2..(height - 2) {
        for x in 2..(width - 2) {
            let idx = (y * width + x) as usize;
            if heightmap[idx] < 0.70 {
                continue;
            }
            let coord = HexCoord::new(x as i32, y as i32);
            let h = heightmap[idx];
            // 检查是否比所有邻居都高
            let is_peak = coord.neighbors().iter().all(|n| {
                let nx = n.q as u32;
                let ny = n.r as u32;
                if nx >= width || ny >= height {
                    return true; // 边界视为更高
                }
                let nidx = (ny * width + nx) as usize;
                heightmap[nidx] <= h
            });
            if is_peak {
                sources.push((x, y));
            }
        }
    }

    // 限制河流数量，避免过多
    let max_rivers = (sources.len()).min(30);
    // 确定性 Fisher-Yates 洗牌
    for i in (1..sources.len()).rev() {
        let j = rng.gen_range(0..=i);
        sources.swap(i, j);
    }
    sources.truncate(max_rivers);

    // 沿最陡梯度下降
    for &(sx, sy) in &sources {
        let mut cx = sx as i32;
        let mut cy = sy as i32;
        let mut steps = 0;
        let max_steps = (width + height) as i32;

        while steps < max_steps {
            let idx = (cy as u32 * width + cx as u32) as usize;
            if idx >= terrain.len() {
                break;
            }

            // 如果到达水域，停止
            if terrain[idx] == TerrainType::Water {
                break;
            }

            // 标记为水域（河流/湖泊）
            terrain[idx] = TerrainType::Water;

            // 找最陡下降邻居
            let coord = HexCoord::new(cx, cy);
            let current_h = heightmap[idx];
            let mut best_h = current_h;
            let mut best_nx = cx;
            let mut best_ny = cy;

            for n in coord.neighbors() {
                let nx = n.q;
                let ny = n.r;
                if nx >= 0 && ny >= 0 && (nx as u32) < width && (ny as u32) < height {
                    let nidx = (ny as u32 * width + nx as u32) as usize;
                    if heightmap[nidx] < best_h {
                        best_h = heightmap[nidx];
                        best_nx = nx;
                        best_ny = ny;
                    }
                }
            }

            // 无法继续下降（已到最低点/洼地）
            if best_nx == cx && best_ny == cy {
                break;
            }

            cx = best_nx;
            cy = best_ny;
            steps += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heightmap_deterministic() {
        let h1 = generate_heightmap(42, 64, 64, 0.5);
        let h2 = generate_heightmap(42, 64, 64, 0.5);
        assert_eq!(h1.len(), h2.len());
        for (a, b) in h1.iter().zip(h2.iter()) {
            assert!((a - b).abs() < 1e-15, "height mismatch: {a} vs {b}");
        }
    }

    #[test]
    fn test_heightmap_range() {
        let h = generate_heightmap(123, 64, 64, 0.5);
        for v in &h {
            assert!(*v >= 0.0 && *v <= 1.0, "out of range: {v}");
        }
    }

    #[test]
    fn test_moisturemap_deterministic() {
        let h = generate_heightmap(42, 32, 32, 0.5);
        let m1 = generate_moisturemap(42, 32, 32, &h);
        let m2 = generate_moisturemap(42, 32, 32, &h);
        for (a, b) in m1.iter().zip(m2.iter()) {
            assert!((a - b).abs() < 1e-15);
        }
    }

    #[test]
    fn test_classify_terrain_water() {
        assert_eq!(classify_terrain(0.1, 0.5), TerrainType::Water);
        assert_eq!(classify_terrain(0.29, 0.9), TerrainType::Water);
    }

    #[test]
    fn test_classify_terrain_mountain() {
        assert_eq!(classify_terrain(0.85, 0.5), TerrainType::Mountain);
    }

    #[test]
    fn test_classify_terrain_plains() {
        assert_eq!(classify_terrain(0.45, 0.4), TerrainType::Plains);
    }

    #[test]
    fn test_temperaturemap_deterministic() {
        let h = generate_heightmap(42, 32, 32, 0.5);
        let t1 = generate_temperaturemap(42, 32, 32, &h);
        let t2 = generate_temperaturemap(42, 32, 32, &h);
        for (a, b) in t1.iter().zip(t2.iter()) {
            assert!((a - b).abs() < 1e-15);
        }
    }
}
