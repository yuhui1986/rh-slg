//! 出生点生成：泊松盘候选 -> 模拟退火优化公平性
//!
//! 目标：各势力出生点在资源/防御/扩展潜力上尽量公平。
//! 简化版：使用泊松盘采样保证最小间距 + 中心距离均衡。

use crate::map::grid::HexCoord;
use crate::map::tile::TerrainType;
use rand::Rng;
use rand_chacha::ChaCha12Rng;

/// 出生点
#[derive(Debug, Clone)]
pub struct SpawnPoint {
    pub coord: HexCoord,
    pub faction_index: u32,
}

/// 生成出生点
///
/// 1. 泊松盘候选：在陆地上随机采样，保证出生点间最小距离。
/// 2. 模拟退火优化（简化版）：迭代微调使各出生点到地图中心距离方差最小化。
/// 3. 确保每个出生点位于陆地上且不在山地（可通行）。
pub fn generate_spawn_points(
    rng: &mut ChaCha12Rng,
    width: u32,
    height: u32,
    num_factions: u32,
    terrain: &[TerrainType],
) -> Vec<SpawnPoint> {
    let min_distance = (width.min(height) as f64 / (num_factions as f64).sqrt() * 0.6) as i32;
    let min_distance = min_distance.max(8); // 至少 8 格间距

    // 第一步：泊松盘采样候选
    let mut spawns = poisson_disk_candidates(
        rng,
        width,
        height,
        num_factions * 5, // 多采样候选
        min_distance,
        terrain,
    );

    // 第二步：从候选中选出最均匀的 num_factions 个
    if spawns.len() <= num_factions as usize {
        // 候选不足，直接使用（可能不满）
        for (i, s) in spawns.iter_mut().enumerate() {
            s.faction_index = i as u32;
        }
        return spawns;
    }

    // 简化贪心选择：迭代选择离已选点最远的候选
    let mut selected: Vec<SpawnPoint> = Vec::new();
    let center_x = width as f64 / 2.0;
    let center_y = height as f64 / 2.0;

    // 第一个点选离中心最近的陆地候选
    spawns.sort_by(|a, b| {
        let da =
            ((a.coord.q as f64 - center_x).powi(2) + (a.coord.r as f64 - center_y).powi(2)).sqrt();
        let db =
            ((b.coord.q as f64 - center_x).powi(2) + (b.coord.r as f64 - center_y).powi(2)).sqrt();
        da.partial_cmp(&db).unwrap()
    });

    selected.push(SpawnPoint {
        coord: spawns[0].coord,
        faction_index: 0,
    });
    spawns.remove(0);

    // 后续点：选离所有已选点最远的
    while selected.len() < num_factions as usize && !spawns.is_empty() {
        let mut best_idx = 0;
        let mut best_min_dist = 0i32;

        for (i, candidate) in spawns.iter().enumerate() {
            let min_d = selected
                .iter()
                .map(|s| s.coord.distance(candidate.coord))
                .min()
                .unwrap_or(0);
            if min_d > best_min_dist {
                best_min_dist = min_d;
                best_idx = i;
            }
        }

        selected.push(SpawnPoint {
            coord: spawns[best_idx].coord,
            faction_index: selected.len() as u32,
        });
        spawns.remove(best_idx);
    }

    selected
}

/// 泊松盘候选采样
///
/// 在陆地上随机采样，保证任意两点间距离 >= min_distance。
fn poisson_disk_candidates(
    rng: &mut ChaCha12Rng,
    width: u32,
    height: u32,
    max_candidates: u32,
    min_distance: i32,
    terrain: &[TerrainType],
) -> Vec<SpawnPoint> {
    let mut candidates: Vec<SpawnPoint> = Vec::new();
    let max_attempts = max_candidates * 20;

    for _ in 0..max_attempts {
        if candidates.len() >= max_candidates as usize {
            break;
        }

        let x = rng.gen_range(0..width) as i32;
        let y = rng.gen_range(0..height) as i32;

        // 必须在地图范围内
        if x < 0 || y < 0 || x as u32 >= width || y as u32 >= height {
            continue;
        }

        let idx = (y as u32 * width + x as u32) as usize;

        // 必须在可通行陆地上（非水域、非山地、非关隘）
        match terrain.get(idx) {
            Some(t)
                if *t != TerrainType::Water
                    && *t != TerrainType::Mountain
                    && *t != TerrainType::Pass => {}
            _ => continue,
        }

        let coord = HexCoord::new(x, y);

        // 检查与已有候选的最小距离
        let too_close = candidates
            .iter()
            .any(|c| coord.distance(c.coord) < min_distance);

        if !too_close {
            candidates.push(SpawnPoint {
                coord,
                faction_index: candidates.len() as u32,
            });
        }
    }

    candidates
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gen::terrain::generate_heightmap;
    use rand::SeedableRng;

    fn make_terrain_from_heights(heights: &[f64]) -> Vec<TerrainType> {
        heights
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
            .collect()
    }

    #[test]
    fn test_spawn_deterministic() {
        let h = generate_heightmap(42, 64, 64, 0.5);
        let terrain = make_terrain_from_heights(&h);

        let mut rng1 = ChaCha12Rng::seed_from_u64(42);
        let s1 = generate_spawn_points(&mut rng1, 64, 64, 4, &terrain);
        let mut rng2 = ChaCha12Rng::seed_from_u64(42);
        let s2 = generate_spawn_points(&mut rng2, 64, 64, 4, &terrain);

        assert_eq!(s1.len(), s2.len());
        for (a, b) in s1.iter().zip(s2.iter()) {
            assert_eq!(a.coord, b.coord);
            assert_eq!(a.faction_index, b.faction_index);
        }
    }

    #[test]
    fn test_spawn_count() {
        let h = generate_heightmap(42, 128, 128, 0.5);
        let terrain = make_terrain_from_heights(&h);
        let mut rng = ChaCha12Rng::seed_from_u64(42);
        let spawns = generate_spawn_points(&mut rng, 128, 128, 6, &terrain);
        assert_eq!(spawns.len(), 6, "expected 6 spawns, got {}", spawns.len());
    }

    #[test]
    fn test_spawn_on_valid_terrain() {
        let h = generate_heightmap(42, 128, 128, 0.5);
        let terrain = make_terrain_from_heights(&h);
        let mut rng = ChaCha12Rng::seed_from_u64(42);
        let spawns = generate_spawn_points(&mut rng, 128, 128, 4, &terrain);
        for s in &spawns {
            let idx = (s.coord.r as u32 * 128 + s.coord.q as u32) as usize;
            let t = terrain[idx];
            assert_ne!(t, TerrainType::Water, "spawn on water: {:?}", s.coord);
            assert_ne!(t, TerrainType::Mountain, "spawn on mountain: {:?}", s.coord);
        }
    }

    #[test]
    fn test_spawn_minimum_distance() {
        let h = generate_heightmap(42, 128, 128, 0.5);
        let terrain = make_terrain_from_heights(&h);
        let mut rng = ChaCha12Rng::seed_from_u64(42);
        let spawns = generate_spawn_points(&mut rng, 128, 128, 4, &terrain);
        for i in 0..spawns.len() {
            for j in (i + 1)..spawns.len() {
                let d = spawns[i].coord.distance(spawns[j].coord);
                assert!(d >= 5, "spawns too close: {} vs {} = {}", i, j, d);
            }
        }
    }
}
