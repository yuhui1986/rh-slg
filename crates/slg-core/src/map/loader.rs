//! 地图加载：MapDocument / Runtime World 转换
//!
//! 三个核心入口：
//! - `load_map`: MapDocument -> LoadResult（运行时数据）
//! - `create_save`: 运行时状态 -> SaveFile
//! - `load_save`: SaveFile + MapDocument -> LoadResult（恢复存档）

use std::collections::BTreeMap;

use slg_data::ids::{FactionId, TileKey};
use slg_data::map_doc::{EntityPlacement, MapDocument, TerrainLayer};
use slg_data::save::SaveFile;

use crate::entity::faction::{FactionResources, FactionState};
use crate::map::tile::ResourceType;

// ---------------------------------------------------------------------------
// 公共数据结构
// ---------------------------------------------------------------------------

/// `load_map` 的返回结果，包含运行时所需的一切静态数据
#[derive(Debug)]
pub struct LoadResult {
    pub chunk_data: Vec<ChunkData>,
    pub tile_owners: BTreeMap<TileKey, FactionId>,
    pub tile_levels: BTreeMap<TileKey, u8>,
    pub tile_resources: BTreeMap<TileKey, ResourceType>,
    pub factions: BTreeMap<FactionId, FactionState>,
    pub entity_placements: BTreeMap<TileKey, EntityPlacement>,
}

/// Chunk 数据（32x32 格 = 1024 格）
#[derive(Debug, Clone)]
pub struct ChunkData {
    pub chunk_x: u32,
    pub chunk_y: u32,
    pub terrains: [u8; 1024],
    pub owners: [u8; 1024],
    pub levels: [u8; 1024],
}

// ---------------------------------------------------------------------------
// MapDocument -> LoadResult
// ---------------------------------------------------------------------------

/// 从 `MapDocument` 加载到运行时结构
///
/// 解码 RLE 地形、展开 Chunk、初始化势力
pub fn load_map(doc: &MapDocument) -> LoadResult {
    let width = doc.meta.width;
    let height = doc.meta.height;

    // 1. 解码 RLE 地形
    let terrains = decode_terrain_rle(&doc.terrain, width * height);

    // 2. 构建 Chunk 数据（32x32 为一个 chunk）
    let chunks_x = width.div_ceil(32);
    let chunks_y = height.div_ceil(32);
    let mut chunk_data = Vec::with_capacity((chunks_x * chunks_y) as usize);

    for cy in 0..chunks_y {
        for cx in 0..chunks_x {
            let mut terrains_chunk = [0u8; 1024];
            let owners_chunk = [0u8; 1024];
            let levels_chunk = [0u8; 1024];

            for ly in 0..32u32 {
                for lx in 0..32u32 {
                    let global_x = cx * 32 + lx;
                    let global_y = cy * 32 + ly;
                    let local_idx = (ly * 32 + lx) as usize;

                    if global_x < width && global_y < height {
                        let global_idx = (global_y * width + global_x) as usize;
                        if global_idx < terrains.len() {
                            terrains_chunk[local_idx] = terrains[global_idx];
                        }
                    }
                }
            }

            chunk_data.push(ChunkData {
                chunk_x: cx,
                chunk_y: cy,
                terrains: terrains_chunk,
                owners: owners_chunk,
                levels: levels_chunk,
            });
        }
    }

    // 3. 从资源层提取 tile 等级和资源类型
    let mut tile_levels = BTreeMap::new();
    let mut tile_resources = BTreeMap::new();

    for (key, entry) in &doc.resources.entries {
        tile_levels.insert(*key, entry.level);
        if let Ok(res_type) = parse_resource_type(&entry.resource_type) {
            tile_resources.insert(*key, res_type);
        }
    }

    // 4. 从实体层提取领地归属
    let mut tile_owners = BTreeMap::new();

    for (key, placement) in &doc.entities.placements {
        if let Some(ref fid) = placement.faction_id {
            tile_owners.insert(*key, fid.clone());
        }
    }

    // 5. 初始化默认势力（1~6）
    let mut factions = BTreeMap::new();
    for i in 1..=6u8 {
        let id = format!("faction_{i}");
        factions.insert(
            id,
            FactionState {
                main_city: None,
                ..Default::default()
            },
        );
    }

    LoadResult {
        chunk_data,
        tile_owners,
        tile_levels,
        tile_resources,
        factions,
        entity_placements: doc.entities.placements.clone(),
    }
}

// ---------------------------------------------------------------------------
// 运行时状态 -> SaveFile
// ---------------------------------------------------------------------------

/// 从运行时状态保存为 `SaveFile`
///
/// `tile_owners` 与 `original_doc` 对比可生成 `TileDelta`；当前简化实现
/// 直接输出空 delta 列表。
pub fn create_save(
    tile_owners: &BTreeMap<TileKey, FactionId>,
    factions: &BTreeMap<FactionId, FactionState>,
    original_doc: &MapDocument,
    current_tick: u64,
) -> SaveFile {
    // 将运行时 FactionState 转为存档格式
    let faction_states = factions
        .iter()
        .map(|(id, state)| slg_data::save::FactionState {
            faction_id: id.clone(),
            resources: slg_data::save::FactionResources {
                gold: state.resources.gold,
                food: state.resources.food,
                wood: state.resources.wood,
                iron: state.resources.iron,
                troops: state.resources.troops,
            },
            diplomacy: state
                .diplomacy
                .iter()
                .map(|(k, v)| (k.clone(), *v))
                .collect(),
        })
        .collect();

    // 计算 tile_delta（简化：与原始地图对比所有 tile_owners）
    let mut tile_delta = Vec::new();
    for (key, new_owner) in tile_owners {
        // 查原始地图中该 tile 是否有归属
        let old_owner = original_doc
            .entities
            .placements
            .get(key)
            .and_then(|p| p.faction_id.clone());

        if old_owner.as_ref() != Some(new_owner) {
            // 该格发生了归属变更，需要记录 old_terrain / new_terrain
            // 从原始地形层中无法直接按 TileKey 查找 terrain（RLE 是线性序列），
            // 这里用占位 terrain_id；完整实现需要在 load_map 时反向索引
            tile_delta.push(slg_data::save::TileDelta {
                tile_key: *key,
                old_terrain: String::new(),
                new_terrain: String::new(),
                old_owner,
                new_owner: Some(new_owner.clone()),
            });
        }
    }

    // 计算 map_hash（简化：SHA-256 需要外部 crate，这里用零值占位）
    let map_hash = [0u8; 32];

    SaveFile {
        map_ref: slg_data::save::MapRef {
            path: original_doc.meta.name.clone(),
            content_hash: map_hash,
        },
        tick: current_tick,
        faction_states,
        entity_snapshots: Vec::new(),
        tile_delta,
        event_log: Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// SaveFile + MapDocument -> LoadResult
// ---------------------------------------------------------------------------

/// 从存档恢复运行时状态
///
/// 需要同时传入原始 `MapDocument` 以获取地形数据（存档不保存完整地形）。
/// 存档中的 `tile_delta` 会覆盖地图文档的默认领地归属。
pub fn load_save(save: &SaveFile, doc: &MapDocument) -> LoadResult {
    // 先从地图文档加载基础数据
    let mut result = load_map(doc);

    // 1. 恢复势力状态
    result.factions.clear();
    for fs in &save.faction_states {
        let mut diplomacy = BTreeMap::new();
        for (other, value) in &fs.diplomacy {
            diplomacy.insert(other.clone(), *value);
        }

        result.factions.insert(
            fs.faction_id.clone(),
            FactionState {
                resources: FactionResources {
                    gold: fs.resources.gold,
                    food: fs.resources.food,
                    wood: fs.resources.wood,
                    iron: fs.resources.iron,
                    stone: 0,
                    troops: fs.resources.troops,
                },
                main_city: None,
                diplomacy,
                ..Default::default()
            },
        );
    }

    // 2. 应用 tile_delta（领地变更）
    for delta in &save.tile_delta {
        if let Some(ref new_owner) = delta.new_owner {
            result.tile_owners.insert(delta.tile_key, new_owner.clone());
        } else {
            result.tile_owners.remove(&delta.tile_key);
        }
    }

    result
}

// ---------------------------------------------------------------------------
// 内部辅助函数
// ---------------------------------------------------------------------------

/// 解码 RLE 地形为密集 u8 数组
///
/// terrain_type_id 到 u8 的映射与 `TerrainType::to_u8` 一致。
fn decode_terrain_rle(layer: &TerrainLayer, total: u32) -> Vec<u8> {
    let mut result = Vec::with_capacity(total as usize);

    for (terrain_id, count) in &layer.rle_data {
        let terrain_byte = terrain_id_to_u8(terrain_id);
        for _ in 0..*count {
            result.push(terrain_byte);
        }
    }

    // 若 RLE 数据不足则用平原填充
    while result.len() < total as usize {
        result.push(0);
    }

    result
}

/// terrain_type_id 字符串 -> u8 编码
///
/// 映射与 `TerrainType::to_u8` / `TerrainType::from_u8` 对齐。
fn terrain_id_to_u8(id: &str) -> u8 {
    match id {
        "terrain_plains" => 0,
        "terrain_mountain" => 1,
        "terrain_water" => 2,
        "terrain_forest" => 3,
        "terrain_desert" => 4,
        "terrain_swamp" => 5,
        "terrain_hills" => 6,
        "terrain_pass" => 7,
        _ => 0, // 未知地形默认平原
    }
}

/// 解析资源类型字符串
fn parse_resource_type(s: &str) -> Result<ResourceType, String> {
    match s {
        "gold" | "Gold" => Ok(ResourceType::Gold),
        "food" | "Food" => Ok(ResourceType::Food),
        "wood" | "Wood" => Ok(ResourceType::Wood),
        "iron" | "Iron" => Ok(ResourceType::Iron),
        "stone" | "Stone" => Ok(ResourceType::Stone),
        _ => Err(format!("未知资源类型: {s}")),
    }
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::faction::FactionPersonality;
    use slg_data::map_doc::{EntityLayer, MapMeta, ResourceLayer, RuleLayer};

    /// 构造一个 64x64 的测试 MapDocument
    fn create_test_doc() -> MapDocument {
        MapDocument {
            meta: MapMeta {
                name: "测试地图".to_string(),
                seed: 42,
                width: 64,
                height: 64,
                preset_name: None,
            },
            terrain: TerrainLayer {
                rle_data: vec![
                    ("terrain_plains".to_string(), 2048),
                    ("terrain_water".to_string(), 2048),
                ],
                total_tiles: 4096,
            },
            resources: ResourceLayer {
                entries: BTreeMap::new(),
            },
            entities: EntityLayer {
                placements: BTreeMap::new(),
            },
            rules: RuleLayer {
                zones: vec![],
                triggers: vec![],
            },
            rivers: Default::default(),
        }
    }

    #[test]
    fn test_load_map_chunk_count() {
        let doc = create_test_doc();
        let result = load_map(&doc);

        // 64x64 -> 2x2 chunks = 4
        assert_eq!(result.chunk_data.len(), 4);
    }

    #[test]
    fn test_load_map_terrain_distribution() {
        let doc = create_test_doc();
        let result = load_map(&doc);

        // 前 2048 格是平原（0），后 2048 格是水域（2）
        // chunk(0,0) 和 chunk(1,0) 全平原；chunk(0,1) 和 chunk(1,1) 全水域
        assert!(
            result.chunk_data[0].terrains.iter().all(|&t| t == 0),
            "chunk(0,0) 应全为平原"
        );
        assert!(
            result.chunk_data[1].terrains.iter().all(|&t| t == 0),
            "chunk(1,0) 应全为平原"
        );
        assert!(
            result.chunk_data[2].terrains.iter().all(|&t| t == 2),
            "chunk(0,1) 应全为水域"
        );
        assert!(
            result.chunk_data[3].terrains.iter().all(|&t| t == 2),
            "chunk(1,1) 应全为水域"
        );
    }

    #[test]
    fn test_load_map_default_factions() {
        let doc = create_test_doc();
        let result = load_map(&doc);

        assert_eq!(result.factions.len(), 6);
        assert!(result.factions.contains_key("faction_1"));
        assert!(result.factions.contains_key("faction_6"));
    }

    #[test]
    fn test_decode_terrain_rle_basic() {
        let layer = TerrainLayer {
            rle_data: vec![
                ("terrain_plains".to_string(), 5),
                ("terrain_water".to_string(), 3),
            ],
            total_tiles: 8,
        };
        let result = decode_terrain_rle(&layer, 8);
        assert_eq!(result, vec![0, 0, 0, 0, 0, 2, 2, 2]);
    }

    #[test]
    fn test_decode_terrain_rle_padding() {
        // RLE 数据不足 total 时应填充平原
        let layer = TerrainLayer {
            rle_data: vec![("terrain_forest".to_string(), 3)],
            total_tiles: 3,
        };
        let result = decode_terrain_rle(&layer, 5);
        assert_eq!(result, vec![3, 3, 3, 0, 0]);
    }

    #[test]
    fn test_parse_resource_type() {
        assert_eq!(parse_resource_type("gold"), Ok(ResourceType::Gold));
        assert_eq!(parse_resource_type("Gold"), Ok(ResourceType::Gold));
        assert_eq!(parse_resource_type("food"), Ok(ResourceType::Food));
        assert_eq!(parse_resource_type("wood"), Ok(ResourceType::Wood));
        assert_eq!(parse_resource_type("iron"), Ok(ResourceType::Iron));
        assert_eq!(parse_resource_type("stone"), Ok(ResourceType::Stone));
        assert!(parse_resource_type("unknown").is_err());
    }

    #[test]
    fn test_terrain_id_to_u8_mapping() {
        assert_eq!(terrain_id_to_u8("terrain_plains"), 0);
        assert_eq!(terrain_id_to_u8("terrain_mountain"), 1);
        assert_eq!(terrain_id_to_u8("terrain_water"), 2);
        assert_eq!(terrain_id_to_u8("terrain_forest"), 3);
        assert_eq!(terrain_id_to_u8("terrain_desert"), 4);
        assert_eq!(terrain_id_to_u8("terrain_swamp"), 5);
        assert_eq!(terrain_id_to_u8("terrain_hills"), 6);
        assert_eq!(terrain_id_to_u8("terrain_pass"), 7);
        assert_eq!(terrain_id_to_u8("unknown"), 0);
    }

    #[test]
    fn test_create_save_basic() {
        let doc = create_test_doc();
        let factions = BTreeMap::new();
        let tile_owners = BTreeMap::new();

        let save = create_save(&tile_owners, &factions, &doc, 100);
        assert_eq!(save.tick, 100);
        assert_eq!(save.map_ref.path, "测试地图");
        assert!(save.faction_states.is_empty());
        assert!(save.tile_delta.is_empty());
    }

    #[test]
    fn test_create_save_with_factions() {
        let doc = create_test_doc();
        let mut factions = BTreeMap::new();
        factions.insert(
            "faction_wei".to_string(),
            FactionState {
                resources: FactionResources {
                    gold: 1000,
                    food: 500,
                    wood: 0,
                    iron: 0,
                    stone: 0,
                    troops: 0,
                },
                personality: FactionPersonality {
                    aggression: 0.8,
                    expansion: 0.6,
                    diplomacy: 0.4,
                    caution: 0.3,
                },
                main_city: None,
                diplomacy: BTreeMap::new(),
                ..Default::default()
            },
        );

        let save = create_save(&BTreeMap::new(), &factions, &doc, 0);
        assert_eq!(save.faction_states.len(), 1);
        assert_eq!(save.faction_states[0].faction_id, "faction_wei");
        assert_eq!(save.faction_states[0].resources.gold, 1000);
    }

    #[test]
    fn test_load_save_restores_factions() {
        let doc = create_test_doc();
        let save = SaveFile {
            map_ref: slg_data::save::MapRef {
                path: "test".to_string(),
                content_hash: [0; 32],
            },
            tick: 42,
            faction_states: vec![slg_data::save::FactionState {
                faction_id: "faction_cao".to_string(),
                resources: slg_data::save::FactionResources {
                    gold: 9999,
                    food: 8888,
                    wood: 0,
                    iron: 0,
                    troops: 100,
                },
                diplomacy: vec![],
            }],
            entity_snapshots: vec![],
            tile_delta: vec![],
            event_log: vec![],
        };

        let result = load_save(&save, &doc);

        // load_save 会用 save 中的势力替换 load_map 的默认势力
        assert_eq!(result.factions.len(), 1);
        let faction = result.factions.get("faction_cao").unwrap();
        assert_eq!(faction.resources.gold, 9999);
        assert_eq!(faction.resources.troops, 100);
    }

    #[test]
    fn test_load_save_applies_tile_delta() {
        let doc = create_test_doc();
        let tile_key = slg_data::ids::tile_key(10, 20);

        let save = SaveFile {
            map_ref: slg_data::save::MapRef {
                path: "test".to_string(),
                content_hash: [0; 32],
            },
            tick: 0,
            faction_states: vec![],
            entity_snapshots: vec![],
            tile_delta: vec![slg_data::save::TileDelta {
                tile_key,
                old_terrain: String::new(),
                new_terrain: String::new(),
                old_owner: None,
                new_owner: Some("faction_conqueror".to_string()),
            }],
            event_log: vec![],
        };

        let result = load_save(&save, &doc);
        assert_eq!(
            result.tile_owners.get(&tile_key),
            Some(&"faction_conqueror".to_string())
        );
    }

    #[test]
    fn test_chunk_data_fields_initialized() {
        let doc = create_test_doc();
        let result = load_map(&doc);

        for chunk in &result.chunk_data {
            // owners 和 levels 默认全 0
            assert!(chunk.owners.iter().all(|&o| o == 0));
            assert!(chunk.levels.iter().all(|&l| l == 0));
        }
    }

    #[test]
    fn test_load_map_entity_placements() {
        let mut placements = BTreeMap::new();
        let key = slg_data::ids::tile_key(5, 5);
        placements.insert(
            key,
            slg_data::map_doc::EntityPlacement {
                entity_type: "city".to_string(),
                faction_id: Some("faction_wei".to_string()),
                properties: BTreeMap::new(),
            },
        );

        let doc = MapDocument {
            meta: MapMeta {
                name: "test".to_string(),
                seed: 0,
                width: 64,
                height: 64,
                preset_name: None,
            },
            terrain: TerrainLayer {
                rle_data: vec![("terrain_plains".to_string(), 4096)],
                total_tiles: 4096,
            },
            resources: ResourceLayer {
                entries: BTreeMap::new(),
            },
            entities: EntityLayer { placements },
            rules: RuleLayer {
                zones: vec![],
                triggers: vec![],
            },
            rivers: Default::default(),
        };

        let result = load_map(&doc);

        // entity_placements 应被透传
        assert!(result.entity_placements.contains_key(&key));

        // 有 faction_id 的实体应被录入 tile_owners
        assert_eq!(
            result.tile_owners.get(&key),
            Some(&"faction_wei".to_string())
        );
    }
}
