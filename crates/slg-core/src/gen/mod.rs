//! 程序化地图生成管线
//!
//! 确定性生成：ChaCha12Rng + noise crate + BTreeMap（禁 HashMap）。
//! 同种子 -> 同地图（逐格一致），跨平台可复现。
//!
//! 管线流程：
//! 1. 主种子派生子种子（地形/资源/出生点/天气）
//! 2. 高程图（Simplex fBm 6 octave + Domain Warping）
//! 3. 湿度图（独立通道 + 距水源衰减）+ 温度图（纬度梯度 + 海拔衰减）
//! 4. 地形分类（高程 x 湿度查表 -> TerrainType）
//! 5. 河流后处理（山脊源头 -> 最陡梯度下降 -> 注地成湖）
//! 6. 要素投放（土地等级、资源点、出生点）
//! 7. 组装 MapDocument

pub mod resource;
pub mod spawn;
pub mod terrain;
pub mod validate;

use std::collections::BTreeMap;

use rand::SeedableRng;
use rand_chacha::ChaCha12Rng;
use serde::{Deserialize, Serialize};

use crate::map::tile::TerrainType;
use slg_data::ids::*;
use slg_data::map_doc::*;

use self::resource::{generate_resources, generate_tile_levels};
use self::spawn::generate_spawn_points;
use self::terrain::{carve_rivers, classify_all, generate_heightmap, generate_moisturemap};
use self::validate::validate_map;

// ---------------------------------------------------------------------------
// 生成预设
// ---------------------------------------------------------------------------

/// 生成预设
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationPreset {
    pub name: String,
    pub description: String,
    pub width: u32,
    pub height: u32,
    /// 0 = 随机
    pub seed: u64,
    /// 0.0=多水域, 0.5=均衡, 1.0=多陆地
    pub terrain_style: f64,
    /// 0.0~2.0
    pub richness: f64,
    pub num_factions: u32,
    pub tags: Vec<String>,
}

impl Default for GenerationPreset {
    fn default() -> Self {
        Self {
            name: "默认".to_string(),
            description: "均衡的 256x256 地图，6 个势力".to_string(),
            width: 256,
            height: 256,
            seed: 0,
            terrain_style: 0.5,
            richness: 0.5,
            num_factions: 6,
            tags: vec!["标准".to_string()],
        }
    }
}

impl GenerationPreset {
    /// 导出为 RON 文件
    pub fn export_to_file(&self, path: &std::path::Path) -> Result<(), std::io::Error> {
        let ron_string = ron::to_string(self).map_err(std::io::Error::other)?;
        std::fs::write(path, ron_string)
    }

    /// 从 RON 文件导入
    pub fn import_from_file(path: &std::path::Path) -> Result<Self, std::io::Error> {
        let content = std::fs::read_to_string(path)?;
        ron::from_str(&content).map_err(std::io::Error::other)
    }

    /// 校验预设参数
    pub fn validate(&self) -> Vec<String> {
        let mut warnings = Vec::new();

        if self.width == 0 || self.height == 0 {
            warnings.push("地图尺寸不能为 0".to_string());
        }
        if self.width > 2048 || self.height > 2048 {
            warnings.push("地图尺寸超过 2048 可能导致性能问题".to_string());
        }
        if self.num_factions == 0 {
            warnings.push("势力数量不能为 0".to_string());
        }
        if self.num_factions > 8 {
            warnings.push("势力数量超过 8 可能影响平衡".to_string());
        }
        if self.richness < 0.0 || self.richness > 2.0 {
            warnings.push("富饶度应在 0.0~2.0 之间".to_string());
        }

        warnings
    }
}

/// 获取内置预设列表
pub fn get_builtin_presets() -> Vec<GenerationPreset> {
    vec![
        GenerationPreset {
            name: "标准大陆".to_string(),
            description: "均衡的大陆地形，适合新手".to_string(),
            width: 256,
            height: 256,
            seed: 0,
            terrain_style: 0.5,
            richness: 0.5,
            num_factions: 6,
            tags: vec!["标准".to_string(), "新手".to_string()],
        },
        GenerationPreset {
            name: "群岛".to_string(),
            description: "水域较多，岛屿分散".to_string(),
            width: 256,
            height: 256,
            seed: 0,
            terrain_style: 0.2,
            richness: 0.4,
            num_factions: 4,
            tags: vec!["水域".to_string(), "挑战".to_string()],
        },
        GenerationPreset {
            name: "平原激战".to_string(),
            description: "开阔平原，适合快速扩张".to_string(),
            width: 256,
            height: 256,
            seed: 0,
            terrain_style: 0.8,
            richness: 0.7,
            num_factions: 6,
            tags: vec!["平原".to_string(), "快速".to_string()],
        },
        GenerationPreset {
            name: "群雄割据".to_string(),
            description: "8 个势力，资源丰富".to_string(),
            width: 256,
            height: 256,
            seed: 0,
            terrain_style: 0.5,
            richness: 0.8,
            num_factions: 8,
            tags: vec!["大型".to_string(), "丰富".to_string()],
        },
    ]
}

// ---------------------------------------------------------------------------
// 生成管线入口
// ---------------------------------------------------------------------------

/// 生成管线入口
///
/// 确定性：同种子 + 同预设 -> 同地图（逐格一致）。
///
/// # 流程
/// 1. 从主种子派生 ChaCha12Rng
/// 2. 生成高程图 (Simplex fBm + Domain Warping)
/// 3. 生成湿度图（距水源衰减）
/// 4. 地形分类 (高程 x 湿度 -> TerrainType)
/// 5. 河流后处理
/// 6. 要素投放（土地等级、资源点、出生点）
/// 7. 组装 MapDocument
pub fn generate_map(seed: u64, preset: &GenerationPreset) -> MapDocument {
    let w = preset.width;
    let h = preset.height;
    // ---- Step 1: 派生 RNG ----
    let mut rng = ChaCha12Rng::seed_from_u64(seed);

    // ---- Step 2: 高程图 ----
    let heightmap = generate_heightmap(seed, w, h, preset.terrain_style);

    // ---- Step 3: 湿度图 ----
    let moisturemap = generate_moisturemap(seed, w, h, &heightmap);

    // ---- Step 4: 地形分类 ----
    let mut terrain = classify_all(&heightmap, &moisturemap);

    // ---- Step 5: 河流后处理 ----
    carve_rivers(&mut rng, w, h, &heightmap, &mut terrain);

    // ---- Step 6: 要素投放 ----
    // 6a. 土地等级
    let levels = generate_tile_levels(&mut rng, w, h, &terrain, &heightmap);

    // 6b. 资源点
    let mut resource_rng = ChaCha12Rng::seed_from_u64(seed.wrapping_add(1000));
    let resources = generate_resources(&mut resource_rng, w, h, &terrain, &levels, preset.richness);

    // 6c. 出生点
    let mut spawn_rng = ChaCha12Rng::seed_from_u64(seed.wrapping_add(2000));
    let spawns = generate_spawn_points(&mut spawn_rng, w, h, preset.num_factions, &terrain);

    // ---- Step 7: 校验 ----
    let validation = validate_map(w, h, &terrain, &spawns);
    if !validation.is_valid {
        // 打印警告但不 panic（简化版允许非致命问题）
        for err in &validation.errors {
            eprintln!("[gen] validation error: {}", err);
        }
    }
    for warn in &validation.warnings {
        eprintln!("[gen] validation warning: {}", warn);
    }

    // ---- Step 8: 组装 MapDocument ----
    let terrain_layer = build_terrain_layer(&terrain);
    let entity_layer = build_entity_layer(&spawns);

    MapDocument {
        meta: MapMeta {
            name: format!("generated_{}", seed),
            seed,
            width: w,
            height: h,
            preset_name: None,
        },
        terrain: terrain_layer,
        resources: ResourceLayer { entries: resources },
        entities: entity_layer,
        rules: RuleLayer {
            zones: Vec::new(),
            triggers: Vec::new(),
        },
        rivers: Default::default(),
    }
}

// ---------------------------------------------------------------------------
// 组装辅助
// ---------------------------------------------------------------------------

/// TerrainType 数组 -> RLE 编码 TerrainLayer
fn build_terrain_layer(terrain: &[TerrainType]) -> TerrainLayer {
    let mut rle_data: Vec<(TerrainTypeId, u32)> = Vec::new();
    let mut iter = terrain.iter().peekable();

    while let Some(&first) = iter.next() {
        let terrain_id = terrain_type_to_id(first);
        let mut count = 1u32;

        while let Some(&next) = iter.peek() {
            if *next == first {
                count += 1;
                iter.next();
            } else {
                break;
            }
        }

        // 如果和上一个相同，合并
        if let Some(last) = rle_data.last_mut() {
            if last.0 == terrain_id {
                last.1 += count;
                continue;
            }
        }

        rle_data.push((terrain_id, count));
    }

    TerrainLayer {
        rle_data,
        total_tiles: terrain.len() as u32,
    }
}

/// TerrainType -> TerrainTypeId (string)
fn terrain_type_to_id(t: TerrainType) -> TerrainTypeId {
    match t {
        TerrainType::Plains => "terrain_plains".to_string(),
        TerrainType::Mountain => "terrain_mountain".to_string(),
        TerrainType::Water => "terrain_water".to_string(),
        TerrainType::Forest => "terrain_forest".to_string(),
        TerrainType::Desert => "terrain_desert".to_string(),
        TerrainType::Swamp => "terrain_swamp".to_string(),
        TerrainType::Hills => "terrain_hills".to_string(),
        TerrainType::Pass => "terrain_pass".to_string(),
    }
}

/// 出生点 -> EntityLayer
fn build_entity_layer(spawns: &[spawn::SpawnPoint]) -> EntityLayer {
    let mut placements = BTreeMap::new();
    for s in spawns {
        let key: TileKey = s.coord.to_tile_key();
        let mut props = BTreeMap::new();
        // faction_id 用 1-indexed（faction_1 ~ faction_6），与 loader 的
        // FactionStore 默认势力（loader.rs `for i in 1..=6u8`）保持一致
        //
        // 历史上曾用 0-indexed（faction_0 ~ faction_5），导致 spawn faction_0
        // 的出生点在 `FactionStore.get_mut("faction_0")` 时返回 None，main_city
        // 设不进去——玩家主城永远缺失一个。
        props.insert("faction_index".to_string(), s.faction_index.to_string());
        placements.insert(
            key,
            EntityPlacement {
                entity_type: "spawn".to_string(),
                faction_id: Some(format!("faction_{}", s.faction_index + 1)),
                properties: props,
            },
        );
    }
    EntityLayer { placements }
}

// ---------------------------------------------------------------------------
// 验收测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// 同种子两次生成结果逐格相同
    #[test]
    fn test_deterministic_same_seed() {
        let preset = GenerationPreset {
            width: 64,
            height: 64,
            num_factions: 4,
            richness: 0.5,
            terrain_style: 0.5,
            ..Default::default()
        };
        let doc1 = generate_map(42, &preset);
        let doc2 = generate_map(42, &preset);

        // meta 相同
        assert_eq!(doc1.meta.seed, doc2.meta.seed);
        assert_eq!(doc1.meta.width, doc2.meta.width);
        assert_eq!(doc1.meta.height, doc2.meta.height);

        // RLE 地形数据相同
        assert_eq!(doc1.terrain.rle_data, doc2.terrain.rle_data);
        assert_eq!(doc1.terrain.total_tiles, doc2.terrain.total_tiles);

        // 资源层相同
        assert_eq!(doc1.resources.entries.len(), doc2.resources.entries.len());
        for (k, v) in &doc1.resources.entries {
            assert_eq!(doc2.resources.entries.get(k), Some(v));
        }

        // 实体层（出生点）相同
        assert_eq!(
            doc1.entities.placements.len(),
            doc2.entities.placements.len()
        );
        for (k, v) in &doc1.entities.placements {
            assert_eq!(doc2.entities.placements.get(k), Some(v));
        }
    }

    /// 不同种子生成不同地图
    #[test]
    fn test_different_seeds_differ() {
        let preset = GenerationPreset {
            width: 128,
            height: 128,
            num_factions: 4,
            richness: 0.5,
            terrain_style: 0.5,
            ..Default::default()
        };
        let doc1 = generate_map(42, &preset);
        let doc2 = generate_map(99999, &preset);

        // 比较资源层（更敏感的差异检测）
        let r1: Vec<_> = doc1.resources.entries.keys().collect();
        let r2: Vec<_> = doc2.resources.entries.keys().collect();
        assert_ne!(
            r1, r2,
            "different seeds should produce different resource distributions"
        );
    }

    /// 生成的出生点数量 = 预期势力数
    #[test]
    fn test_spawn_count_matches_factions() {
        let preset = GenerationPreset {
            width: 128,
            height: 128,
            num_factions: 6,
            richness: 0.5,
            terrain_style: 0.6, // 偏多陆地，确保有足够空间
            ..Default::default()
        };
        let doc = generate_map(42, &preset);
        assert_eq!(
            doc.entities.placements.len(),
            6,
            "expected 6 spawns, got {}",
            doc.entities.placements.len()
        );
    }

    /// 诊断：打印 6 个 spawn 的 (q, r) 验证 round-trip 不丢符号
    /// 用户报告主城 marker 出现在 r<0 的位置——先确认 gen→entity_placements 这一步正确
    #[test]
    fn test_diagnostic_sanguo_dl_spawns() {
        let preset = GenerationPreset {
            name: "三国鼎立".to_string(),
            description: "诊断用".to_string(),
            width: 128,
            height: 128,
            seed: 42,
            terrain_style: 0.5,
            richness: 0.6,
            num_factions: 6,
            tags: vec!["三国".to_string()],
        };
        let doc = generate_map(42, &preset);

        eprintln!("─── 三国鼎立 spawn 诊断 (共 {}) ───", doc.entities.placements.len());
        for (key, placement) in &doc.entities.placements {
            use crate::map::grid::HexCoord;
            let c = HexCoord::from_tile_key(*key);
            eprintln!(
                "  {} @ key=0x{:016X} hex=(q={}, r={})",
                placement.faction_id.as_deref().unwrap_or("?"),
                key,
                c.q,
                c.r
            );
            // round-trip 验证
            let round_trip = c.to_tile_key();
            assert_eq!(round_trip, *key, "round-trip mismatch");
            // 范围验证
            assert!(c.q >= 0 && c.q < preset.width as i32, "q out of range: {}", c.q);
            assert!(c.r >= 0 && c.r < preset.height as i32, "r out of range: {}", c.r);
        }
        assert_eq!(doc.entities.placements.len(), 6, "expected 6 spawns");

        // 回归：spawn faction_id 必须是 1-indexed（faction_1 ~ faction_6）
        // 与 loader.rs 的 FactionStore 默认势力 ID 对齐
        // BTreeMap 按 key 排序迭代，所以不能按 enumerate 顺序断言
        let mut faction_ids: Vec<String> = doc
            .entities
            .placements
            .values()
            .filter_map(|p| p.faction_id.clone())
            .collect();
        faction_ids.sort();
        let expected: Vec<String> = (1..=6u8).map(|i| format!("faction_{i}")).collect();
        assert_eq!(
            faction_ids, expected,
            "spawn faction_id 应为 1-indexed (faction_1~faction_6)"
        );

        // 同样：properties.faction_index 应是 0-indexed (0~5)
        let mut indices: Vec<u32> = doc
            .entities
            .placements
            .values()
            .filter_map(|p| p.properties.get("faction_index").and_then(|s| s.parse().ok()))
            .collect();
        indices.sort();
        let expected_idx: Vec<u32> = (0..6).collect();
        assert_eq!(indices, expected_idx, "properties.faction_index 应为 0-indexed (0~5)");
    }

    /// 输出地图无大面积水域死区（陆地占比 > 60%）
    #[test]
    fn test_land_ratio_above_60() {
        let preset = GenerationPreset {
            width: 128,
            height: 128,
            num_factions: 4,
            richness: 0.5,
            terrain_style: 0.5,
            ..Default::default()
        };
        let doc = generate_map(42, &preset);

        // 解压 RLE 计算陆地格
        let mut land_count = 0u32;
        let mut total = 0u32;
        for (terrain_id, count) in &doc.terrain.rle_data {
            total += count;
            if terrain_id != "terrain_water" {
                land_count += count;
            }
        }
        let ratio = land_count as f64 / total as f64;
        assert!(
            ratio > 0.60,
            "land ratio {:.1}% < 60% ({}/{})",
            ratio * 100.0,
            land_count,
            total
        );
    }

    /// MapDocument 的 total_tiles 正确
    #[test]
    fn test_total_tiles_correct() {
        let preset = GenerationPreset {
            width: 32,
            height: 32,
            ..Default::default()
        };
        let doc = generate_map(42, &preset);
        assert_eq!(doc.terrain.total_tiles, 32 * 32);
    }

    /// 默认预设 256x256 能正常完成（不 panic）
    #[test]
    fn test_default_preset_256x256() {
        let preset = GenerationPreset::default();
        let doc = generate_map(42, &preset);
        assert_eq!(doc.meta.width, 256);
        assert_eq!(doc.meta.height, 256);
        assert_eq!(doc.terrain.total_tiles, 256 * 256);
    }

    /// 导出 -> 导入往返一致
    #[test]
    fn test_preset_export_import() {
        let preset = GenerationPreset::default();
        let path = std::path::Path::new("test_preset.ron");

        preset.export_to_file(path).unwrap();
        let loaded = GenerationPreset::import_from_file(path).unwrap();

        assert_eq!(loaded.name, preset.name);
        assert_eq!(loaded.width, preset.width);
        assert_eq!(loaded.num_factions, preset.num_factions);

        std::fs::remove_file(path).ok();
    }

    /// 无效参数被校验拦截
    #[test]
    fn test_preset_validate() {
        let mut preset = GenerationPreset::default();
        assert!(preset.validate().is_empty());

        preset.num_factions = 0;
        assert!(!preset.validate().is_empty());
    }

    /// 内置预设可正确加载
    #[test]
    fn test_builtin_presets() {
        let presets = get_builtin_presets();
        assert!(!presets.is_empty());

        for preset in &presets {
            assert!(preset.validate().is_empty());
        }
    }
}
