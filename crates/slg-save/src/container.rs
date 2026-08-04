//! .slgmap/.slgsave 容器格式读写
//!
//! 二进制容器布局:
//! ```text
//! ┌─────────────────────────────────┐
//! │ Magic "SLGM" │ Version u32 LE  │  文件头 (16 bytes)
//! │ TOC Offset u64 LE              │  → 指向末尾目录
//! ├─────────────────────────────────┤
//! │ Section 0 (bincode [±zstd])    │
//! │ Section 1 (bincode + zstd)     │
//! │ ...                            │
//! ├─────────────────────────────────┤
//! │ TOC: bincode(TableOfContents)  │
//! └─────────────────────────────────┘
//! ```

use std::io::{Read, Seek, SeekFrom, Write};

use serde::{Deserialize, Serialize};

use crate::*;

// ---------------------------------------------------------------------------
// Public API: MapDocument (.slgmap)
// ---------------------------------------------------------------------------

/// 将 `MapDocument` 保存为 `.slgmap` 文件。
pub fn save_map_to_file(
    doc: &slg_data::map_doc::MapDocument,
    path: &std::path::Path,
) -> Result<(), SaveError> {
    let mut file = std::fs::File::create(path)?;

    // 1. 写文件头（占位 toc_offset，最后回填）
    file.write_all(MAGIC)?;
    file.write_all(&VERSION.to_le_bytes())?;
    let toc_offset_pos = file.stream_position()?;
    file.write_all(&0u64.to_le_bytes())?; // 占位 8 bytes

    // 2. 写各节
    let mut toc_entries = Vec::new();

    write_section(&mut file, SectionType::Meta, &doc.meta, &mut toc_entries)?;
    write_section_compressed(
        &mut file,
        SectionType::TerrainLayer,
        &doc.terrain,
        &mut toc_entries,
    )?;
    write_section_compressed(
        &mut file,
        SectionType::ResourceLayer,
        &doc.resources,
        &mut toc_entries,
    )?;
    write_section_compressed(
        &mut file,
        SectionType::EntityLayer,
        &doc.entities,
        &mut toc_entries,
    )?;
    write_section_compressed(
        &mut file,
        SectionType::RuleLayer,
        &doc.rules,
        &mut toc_entries,
    )?;

    // Preview PNG 占位（空数据）
    write_empty_preview(&mut file, &mut toc_entries)?;

    // 3. 写 TOC
    write_toc_and_patch_header(&mut file, &toc_entries, toc_offset_pos)?;

    Ok(())
}

/// 从 `.slgmap` 文件加载 `MapDocument`。
pub fn load_map_from_file(
    path: &std::path::Path,
) -> Result<slg_data::map_doc::MapDocument, SaveError> {
    let mut file = std::fs::File::open(path)?;
    let toc = read_header_and_toc(&mut file)?;

    let meta = read_section::<slg_data::map_doc::MapMeta>(&mut file, &toc, SectionType::Meta)?;
    let terrain = read_section_compressed::<slg_data::map_doc::TerrainLayer>(
        &mut file,
        &toc,
        SectionType::TerrainLayer,
    )?;
    let resources = read_section_compressed::<slg_data::map_doc::ResourceLayer>(
        &mut file,
        &toc,
        SectionType::ResourceLayer,
    )?;
    let entities = read_section_compressed::<slg_data::map_doc::EntityLayer>(
        &mut file,
        &toc,
        SectionType::EntityLayer,
    )?;
    let rules = read_section_compressed::<slg_data::map_doc::RuleLayer>(
        &mut file,
        &toc,
        SectionType::RuleLayer,
    )?;

    Ok(slg_data::map_doc::MapDocument {
        meta,
        terrain,
        resources,
        entities,
        rules,
        rivers: Default::default(),
    })
}

// ---------------------------------------------------------------------------
// Public API: Map validation, preview, import/export
// ---------------------------------------------------------------------------

/// 地图验证结果
#[derive(Debug)]
pub struct MapValidationResult {
    pub valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

/// 写一个 PNG 块（type + data），自动计算长度和 CRC32。
fn png_chunk(chunk_type: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + 4 + data.len() + 4);
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(chunk_type);
    out.extend_from_slice(data);
    let crc_input: Vec<u8> = chunk_type.iter().chain(data.iter()).copied().collect();
    out.extend_from_slice(&crc32fast::hash(&crc_input).to_be_bytes());
    out
}

/// 生成地图预览图（简化版：生成最小合法 1x1 PNG 占位）
pub fn generate_preview_png(_doc: &slg_data::map_doc::MapDocument) -> Vec<u8> {
    // 完整实现应根据地形数据渲染缩略图
    // 当前生成一个 1x1 白色 RGB PNG 作为占位

    let mut out = Vec::new();

    // PNG signature
    out.extend_from_slice(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);

    // IHDR: 1x1, 8-bit RGB
    out.extend(png_chunk(
        b"IHDR",
        &[
            0x00, 0x00, 0x00, 0x01, // width = 1
            0x00, 0x00, 0x00, 0x01, // height = 1
            0x08, 0x02, // bit depth=8, color type=2 (RGB)
            0x00, 0x00, 0x00, // compression=0, filter=0, interlace=0
        ],
    ));

    // IDAT: uncompressed deflate block for 1 row of RGB white pixel
    // Raw row: filter_byte(0) + R(255) G(255) B(255)
    // Zlib: header(0x78 0x01) + stored block + adler32
    let raw_row: [u8; 4] = [0x00, 0xFF, 0xFF, 0xFF]; // filter=none, white pixel
    let adler = {
        let mut s1: u32 = 1;
        let mut s2: u32 = 0;
        for &b in &raw_row {
            s1 = (s1 + b as u32) % 65521;
            s2 = (s2 + s1) % 65521;
        }
        (s2 << 16) | s1
    };
    let mut idat_data = Vec::new();
    idat_data.extend_from_slice(&[0x78, 0x01]); // zlib header (no compression)
                                                // stored block: BFINAL=1, BTYPE=00
    idat_data.push(0x01); // BFINAL=1, BTYPE=00
    idat_data.extend_from_slice(&(raw_row.len() as u16).to_le_bytes()); // LEN
    idat_data.extend_from_slice(&(!raw_row.len() as u16).to_le_bytes()); // NLEN
    idat_data.extend_from_slice(&raw_row);
    idat_data.extend_from_slice(&adler.to_be_bytes());
    out.extend(png_chunk(b"IDAT", &idat_data));

    // IEND
    out.extend(png_chunk(b"IEND", &[]));

    out
}

/// 验证地图文件完整性
pub fn validate_map_file(path: &std::path::Path) -> Result<MapValidationResult, SaveError> {
    let doc = load_map_from_file(path)?;

    let mut result = MapValidationResult {
        valid: true,
        errors: Vec::new(),
        warnings: Vec::new(),
    };

    // 检查地图尺寸
    if doc.meta.width == 0 || doc.meta.height == 0 {
        result.errors.push("地图尺寸为 0".to_string());
        result.valid = false;
    }

    if doc.meta.width > 2048 || doc.meta.height > 2048 {
        result
            .warnings
            .push("地图尺寸超过 2048 可能导致性能问题".to_string());
    }

    // 检查种子
    if doc.meta.seed == 0 {
        result.warnings.push("种子为 0，地图不可复现".to_string());
    }

    // 检查地形总格数与宽高是否匹配
    let expected_tiles = doc.meta.width * doc.meta.height;
    if doc.terrain.total_tiles != expected_tiles {
        result.errors.push(format!(
            "地形总格数 {} 与宽高乘积 {} 不匹配",
            doc.terrain.total_tiles, expected_tiles
        ));
        result.valid = false;
    }

    Ok(result)
}

/// 导出地图为可分享格式
pub fn export_map_for_sharing(
    doc: &slg_data::map_doc::MapDocument,
    path: &std::path::Path,
) -> Result<(), SaveError> {
    // 生成预览图（目前仅占位，后续可嵌入文件或单独输出）
    let _preview = generate_preview_png(doc);

    // 保存地图文件
    save_map_to_file(doc, path)?;

    Ok(())
}

/// 从分享文件导入地图
pub fn import_map_from_sharing(
    path: &std::path::Path,
) -> Result<slg_data::map_doc::MapDocument, SaveError> {
    // 验证文件完整性
    let validation = validate_map_file(path)?;
    if !validation.valid {
        return Err(SaveError::ValidationFailed(validation.errors.join("; ")));
    }

    // 加载地图
    load_map_from_file(path)
}

// ---------------------------------------------------------------------------
// Public API: SaveFile (.slgsave)
// ---------------------------------------------------------------------------

/// 将 `SaveFile` 保存为 `.slgsave` 文件。
pub fn save_save_to_file(
    save: &slg_data::save::SaveFile,
    path: &std::path::Path,
) -> Result<(), SaveError> {
    let mut file = std::fs::File::create(path)?;

    // 1. 写文件头（占位 toc_offset）
    file.write_all(MAGIC)?;
    file.write_all(&VERSION.to_le_bytes())?;
    let toc_offset_pos = file.stream_position()?;
    file.write_all(&0u64.to_le_bytes())?;

    // 2. 写各节
    let mut toc_entries = Vec::new();

    // SaveMeta — 保存 map_ref + tick 元信息（不压缩，体积小）
    #[derive(Serialize)]
    struct SaveMeta<'a> {
        map_ref: &'a slg_data::save::MapRef,
        tick: u64,
    }
    write_section(
        &mut file,
        SectionType::SaveMeta,
        &SaveMeta {
            map_ref: &save.map_ref,
            tick: save.tick,
        },
        &mut toc_entries,
    )?;

    write_section_compressed(
        &mut file,
        SectionType::FactionStates,
        &save.faction_states,
        &mut toc_entries,
    )?;
    write_section_compressed(
        &mut file,
        SectionType::EntitySnapshots,
        &save.entity_snapshots,
        &mut toc_entries,
    )?;
    write_section_compressed(
        &mut file,
        SectionType::TileDeltas,
        &save.tile_delta,
        &mut toc_entries,
    )?;
    write_section_compressed(
        &mut file,
        SectionType::EventLog,
        &save.event_log,
        &mut toc_entries,
    )?;

    // 3. 写 TOC
    write_toc_and_patch_header(&mut file, &toc_entries, toc_offset_pos)?;

    Ok(())
}

/// 从 `.slgsave` 文件加载 `SaveFile`。
pub fn load_save_from_file(path: &std::path::Path) -> Result<slg_data::save::SaveFile, SaveError> {
    let mut file = std::fs::File::open(path)?;
    let toc = read_header_and_toc(&mut file)?;

    // 读取 SaveMeta
    #[derive(Deserialize)]
    struct SaveMeta {
        map_ref: slg_data::save::MapRef,
        tick: u64,
    }
    let meta = read_section::<SaveMeta>(&mut file, &toc, SectionType::SaveMeta)?;

    let faction_states = read_section_compressed::<Vec<slg_data::save::FactionState>>(
        &mut file,
        &toc,
        SectionType::FactionStates,
    )?;
    let entity_snapshots = read_section_compressed::<Vec<slg_data::save::EntitySnapshot>>(
        &mut file,
        &toc,
        SectionType::EntitySnapshots,
    )?;
    let tile_delta = read_section_compressed::<Vec<slg_data::save::TileDelta>>(
        &mut file,
        &toc,
        SectionType::TileDeltas,
    )?;
    let event_log = read_section_compressed::<Vec<slg_data::save::EventLogEntry>>(
        &mut file,
        &toc,
        SectionType::EventLog,
    )?;

    Ok(slg_data::save::SaveFile {
        map_ref: meta.map_ref,
        tick: meta.tick,
        faction_states,
        entity_snapshots,
        tile_delta,
        event_log,
    })
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// 写一个节（纯 bincode 序列化，不压缩）。
fn write_section<T: Serialize>(
    file: &mut std::fs::File,
    section_type: SectionType,
    data: &T,
    toc: &mut Vec<TocEntry>,
) -> Result<(), SaveError> {
    let bytes = bincode::serialize(data).map_err(|e| SaveError::Serialization(e.to_string()))?;
    let offset = file.stream_position()?;
    file.write_all(&bytes)?;
    toc.push(TocEntry {
        section_type,
        offset,
        size: bytes.len() as u64,
        crc32: crc32(&bytes),
    });
    Ok(())
}

/// 写一个节（bincode + zstd 压缩）。
fn write_section_compressed<T: Serialize>(
    file: &mut std::fs::File,
    section_type: SectionType,
    data: &T,
    toc: &mut Vec<TocEntry>,
) -> Result<(), SaveError> {
    let bytes = bincode::serialize(data).map_err(|e| SaveError::Serialization(e.to_string()))?;
    let compressed =
        zstd::encode_all(bytes.as_slice(), 3).map_err(|e| SaveError::Compression(e.to_string()))?;
    let offset = file.stream_position()?;
    file.write_all(&compressed)?;
    toc.push(TocEntry {
        section_type,
        offset,
        size: compressed.len() as u64,
        crc32: crc32(&compressed),
    });
    Ok(())
}

/// 写空的 Preview PNG 占位节。
fn write_empty_preview(file: &mut std::fs::File, toc: &mut Vec<TocEntry>) -> Result<(), SaveError> {
    let preview_data: Vec<u8> = Vec::new();
    let offset = file.stream_position()?;
    // 不写任何字节，size = 0
    toc.push(TocEntry {
        section_type: SectionType::PreviewPng,
        offset,
        size: 0,
        crc32: crc32(&preview_data),
    });
    Ok(())
}

/// 序列化并写入 TOC，然后回填文件头中的 toc_offset。
fn write_toc_and_patch_header(
    file: &mut std::fs::File,
    toc_entries: &[TocEntry],
    toc_offset_pos: u64,
) -> Result<(), SaveError> {
    let toc = TableOfContents {
        entries: toc_entries.to_vec(),
    };
    let toc_bytes =
        bincode::serialize(&toc).map_err(|e| SaveError::Serialization(e.to_string()))?;
    let toc_offset = file.stream_position()?;
    file.write_all(&toc_bytes)?;

    // 回填 toc_offset
    file.seek(SeekFrom::Start(toc_offset_pos))?;
    file.write_all(&toc_offset.to_le_bytes())?;

    Ok(())
}

/// 读取文件头并验证，然后读取 TOC。
fn read_header_and_toc(file: &mut std::fs::File) -> Result<TableOfContents, SaveError> {
    // 1. 读文件头
    let mut magic = [0u8; 4];
    file.read_exact(&mut magic)?;
    if magic != *MAGIC {
        return Err(SaveError::InvalidMagic(magic));
    }

    let mut version_bytes = [0u8; 4];
    file.read_exact(&mut version_bytes)?;
    let version = u32::from_le_bytes(version_bytes);
    if version != VERSION {
        return Err(SaveError::UnsupportedVersion(version));
    }

    let mut toc_offset_bytes = [0u8; 8];
    file.read_exact(&mut toc_offset_bytes)?;
    let toc_offset = u64::from_le_bytes(toc_offset_bytes);

    // 2. 读 TOC
    file.seek(SeekFrom::Start(toc_offset))?;
    let mut toc_bytes = Vec::new();
    file.read_to_end(&mut toc_bytes)?;
    let toc: TableOfContents =
        bincode::deserialize(&toc_bytes).map_err(|e| SaveError::Serialization(e.to_string()))?;

    Ok(toc)
}

/// 读一个节（纯 bincode 反序列化）。
fn read_section<T: for<'de> Deserialize<'de>>(
    file: &mut std::fs::File,
    toc: &TableOfContents,
    section_type: SectionType,
) -> Result<T, SaveError> {
    let entry = toc
        .entries
        .iter()
        .find(|e| e.section_type == section_type)
        .ok_or(SaveError::SectionNotFound(section_type))?;

    file.seek(SeekFrom::Start(entry.offset))?;
    let mut bytes = vec![0u8; entry.size as usize];
    file.read_exact(&mut bytes)?;

    // CRC 校验
    let actual_crc = crc32(&bytes);
    if actual_crc != entry.crc32 {
        return Err(SaveError::CrcMismatch {
            section_type,
            expected: entry.crc32,
            actual: actual_crc,
        });
    }

    bincode::deserialize(&bytes).map_err(|e| SaveError::Serialization(e.to_string()))
}

/// 读一个节（zstd 解压 + bincode 反序列化）。
fn read_section_compressed<T: for<'de> Deserialize<'de>>(
    file: &mut std::fs::File,
    toc: &TableOfContents,
    section_type: SectionType,
) -> Result<T, SaveError> {
    let entry = toc
        .entries
        .iter()
        .find(|e| e.section_type == section_type)
        .ok_or(SaveError::SectionNotFound(section_type))?;

    file.seek(SeekFrom::Start(entry.offset))?;
    let mut compressed = vec![0u8; entry.size as usize];
    file.read_exact(&mut compressed)?;

    // CRC 校验（对压缩后数据）
    let actual_crc = crc32(&compressed);
    if actual_crc != entry.crc32 {
        return Err(SaveError::CrcMismatch {
            section_type,
            expected: entry.crc32,
            actual: actual_crc,
        });
    }

    let bytes = zstd::decode_all(compressed.as_slice())
        .map_err(|e| SaveError::Decompression(e.to_string()))?;

    bincode::deserialize(&bytes).map_err(|e| SaveError::Serialization(e.to_string()))
}

/// CRC32 校验和计算。
fn crc32(data: &[u8]) -> u32 {
    crc32fast::hash(data)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use slg_data::ids::tile_key;
    use slg_data::map_doc::*;
    use std::collections::BTreeMap;

    fn create_test_map() -> MapDocument {
        // 生成较大的 RLE 数据以便压缩效果明显
        let mut rle_data = Vec::new();
        let terrain_types = [
            "terrain_plains",
            "terrain_mountain",
            "terrain_forest",
            "terrain_water",
            "terrain_desert",
        ];
        for i in 0..200u32 {
            rle_data.push((
                terrain_types[(i as usize) % terrain_types.len()].to_string(),
                100,
            ));
        }

        // 生成较多资源条目
        let mut resources_map = BTreeMap::new();
        for q in 0..50i32 {
            for r in 0..50i32 {
                resources_map.insert(
                    tile_key(q, r),
                    ResourceEntry {
                        resource_type: "iron".to_string(),
                        level: ((q + r) % 5) as u8,
                    },
                );
            }
        }

        MapDocument {
            meta: MapMeta {
                name: "测试地图".to_string(),
                seed: 42,
                width: 256,
                height: 256,
                preset_name: None,
            },
            terrain: TerrainLayer {
                rle_data,
                total_tiles: 256 * 256,
            },
            resources: ResourceLayer {
                entries: resources_map,
            },
            entities: {
                let mut m = BTreeMap::new();
                m.insert(
                    tile_key(0, 0),
                    EntityPlacement {
                        entity_type: "city".to_string(),
                        faction_id: Some("faction_wei".to_string()),
                        properties: {
                            let mut p = BTreeMap::new();
                            p.insert("name".to_string(), "许昌".to_string());
                            p
                        },
                    },
                );
                EntityLayer { placements: m }
            },
            rules: RuleLayer {
                zones: vec![ZoneRule {
                    name: "北方区域".to_string(),
                    tiles: vec![tile_key(0, 0), tile_key(1, 0)],
                    rule_type: "supply_zone".to_string(),
                }],
                triggers: vec![TriggerRule {
                    event_id: "event_yellow_turban".to_string(),
                    condition: "tick >= 100".to_string(),
                    effect: "spawn_rebels".to_string(),
                }],
            },
            rivers: Default::default(),
        }
    }

    fn create_test_save() -> slg_data::save::SaveFile {
        slg_data::save::SaveFile {
            map_ref: slg_data::save::MapRef {
                path: "maps/test.slgmap".to_string(),
                content_hash: [0xAB; 32],
            },
            tick: 1234,
            faction_states: vec![slg_data::save::FactionState {
                faction_id: "faction_wei".to_string(),
                resources: slg_data::save::FactionResources {
                    gold: 5000,
                    food: 3000,
                    wood: 2000,
                    iron: 1000,
                    troops: 500,
                },
                diplomacy: vec![("faction_shu".to_string(), -20)],
            }],
            entity_snapshots: vec![slg_data::save::EntitySnapshot {
                entity_id: 1,
                entity_type: "city".to_string(),
                data: vec![1, 2, 3, 4],
            }],
            tile_delta: vec![slg_data::save::TileDelta {
                tile_key: tile_key(3, 4),
                old_terrain: "terrain_plains".to_string(),
                new_terrain: "terrain_mountain".to_string(),
                old_owner: None,
                new_owner: Some("faction_wei".to_string()),
            }],
            event_log: vec![slg_data::save::EventLogEntry {
                tick: 100,
                event_id: "event_yellow_turban".to_string(),
                description: "黄巾之乱爆发".to_string(),
            }],
        }
    }

    #[test]
    fn test_map_roundtrip() {
        let doc = create_test_map();
        let dir = std::env::temp_dir().join("slg_save_test_map_roundtrip");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("test_map.slgmap");

        // 保存
        save_map_to_file(&doc, &path).expect("save failed");

        // 加载
        let loaded = load_map_from_file(&path).expect("load failed");

        // 验证
        assert_eq!(loaded.meta.name, doc.meta.name);
        assert_eq!(loaded.meta.seed, doc.meta.seed);
        assert_eq!(loaded.meta.width, doc.meta.width);
        assert_eq!(loaded.meta.height, doc.meta.height);
        assert_eq!(loaded.terrain.total_tiles, doc.terrain.total_tiles);
        assert_eq!(loaded.terrain.rle_data.len(), doc.terrain.rle_data.len());
        assert_eq!(loaded.resources.entries, doc.resources.entries);
        assert_eq!(loaded.entities.placements, doc.entities.placements);
        assert_eq!(loaded.rules.zones.len(), doc.rules.zones.len());
        assert_eq!(loaded.rules.triggers.len(), doc.rules.triggers.len());

        // 清理
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_save_roundtrip() {
        let save = create_test_save();
        let dir = std::env::temp_dir().join("slg_save_test_save_roundtrip");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("test.slgsave");

        save_save_to_file(&save, &path).expect("save failed");
        let loaded = load_save_from_file(&path).expect("load failed");

        assert_eq!(loaded.map_ref.path, save.map_ref.path);
        assert_eq!(loaded.map_ref.content_hash, save.map_ref.content_hash);
        assert_eq!(loaded.tick, save.tick);
        assert_eq!(loaded.faction_states.len(), save.faction_states.len());
        assert_eq!(
            loaded.faction_states[0].faction_id,
            save.faction_states[0].faction_id
        );
        assert_eq!(
            loaded.faction_states[0].resources.gold,
            save.faction_states[0].resources.gold
        );
        assert_eq!(loaded.entity_snapshots.len(), save.entity_snapshots.len());
        assert_eq!(loaded.tile_delta.len(), save.tile_delta.len());
        assert_eq!(loaded.event_log.len(), save.event_log.len());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_invalid_magic() {
        let dir = std::env::temp_dir().join("slg_save_test_invalid_magic");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("bad.slgmap");

        std::fs::write(&path, b"WRONG_DATA").unwrap();

        let result = load_map_from_file(&path);
        assert!(matches!(result, Err(SaveError::InvalidMagic(_))));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_unsupported_version() {
        let dir = std::env::temp_dir().join("slg_save_test_bad_version");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("bad_ver.slgmap");

        // 写正确 magic 但错误 version
        let mut data = Vec::new();
        data.extend_from_slice(b"SLGM");
        data.extend_from_slice(&99u32.to_le_bytes());
        data.extend_from_slice(&0u64.to_le_bytes());
        std::fs::write(&path, &data).unwrap();

        let result = load_map_from_file(&path);
        assert!(matches!(result, Err(SaveError::UnsupportedVersion(99))));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_crc_mismatch_on_tamper() {
        let doc = create_test_map();
        let dir = std::env::temp_dir().join("slg_save_test_crc_tamper");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("tampered.slgmap");

        save_map_to_file(&doc, &path).expect("save failed");

        // 读取整个文件
        let mut file_bytes = std::fs::read(&path).unwrap();

        // 篡改第一个节的数据（文件头之后的第一个字节）
        if file_bytes.len() > 17 {
            file_bytes[16] ^= 0xFF;
        }
        std::fs::write(&path, &file_bytes).unwrap();

        let result = load_map_from_file(&path);
        // 应该报 CRC 错误或反序列化错误
        assert!(result.is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_compression_effectiveness() {
        let doc = create_test_map();
        let dir = std::env::temp_dir().join("slg_save_test_compression");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("compressed.slgmap");

        save_map_to_file(&doc, &path).expect("save failed");

        // 压缩后的文件应该比 bincode 原始序列化小
        let raw_meta = bincode::serialize(&doc.meta).unwrap();
        let raw_terrain = bincode::serialize(&doc.terrain).unwrap();
        let raw_resources = bincode::serialize(&doc.resources).unwrap();
        let raw_entities = bincode::serialize(&doc.entities).unwrap();
        let raw_rules = bincode::serialize(&doc.rules).unwrap();
        let raw_total = raw_meta.len()
            + raw_terrain.len()
            + raw_resources.len()
            + raw_entities.len()
            + raw_rules.len();

        let file_size = std::fs::metadata(&path).unwrap().len() as usize;

        // 文件大小应小于原始 bincode 总大小（因为 terrain、resources 等被压缩了）
        // 加上文件头(16) + TOC 开销，但仍应显著小于原始总大小
        assert!(
            file_size < raw_total,
            "file_size ({file_size}) should be less than raw_total ({raw_total})"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_validate_map_file() {
        let doc = create_test_map();
        let dir = std::env::temp_dir().join("slg_save_test_validate");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("test_validate.slgmap");

        save_map_to_file(&doc, &path).unwrap();
        let result = validate_map_file(&path).unwrap();

        assert!(result.valid);
        assert!(result.errors.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_validate_map_file_size_mismatch() {
        // 创建一个 total_tiles 与 width*height 不匹配的地图
        let mut doc = create_test_map();
        doc.terrain.total_tiles = 999; // 故意不匹配 (256*256 = 65536)
        let dir = std::env::temp_dir().join("slg_save_test_validate_mismatch");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("mismatch.slgmap");

        save_map_to_file(&doc, &path).unwrap();
        let result = validate_map_file(&path).unwrap();

        assert!(!result.valid);
        assert!(!result.errors.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_validate_map_file_warnings() {
        // 创建一个有警告条件的地图：seed=0, 尺寸>2048
        let mut doc = create_test_map();
        doc.meta.seed = 0;
        doc.meta.width = 4096;
        doc.meta.height = 4096;
        doc.terrain.total_tiles = 4096 * 4096;
        let dir = std::env::temp_dir().join("slg_save_test_validate_warnings");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("warnings.slgmap");

        save_map_to_file(&doc, &path).unwrap();
        let result = validate_map_file(&path).unwrap();

        assert!(result.valid); // 仍然 valid，只是有警告
        assert!(result.warnings.len() >= 2);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_export_import_roundtrip() {
        let doc = create_test_map();
        let dir = std::env::temp_dir().join("slg_save_test_export_import");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("test_share.slgmap");

        export_map_for_sharing(&doc, &path).unwrap();
        let imported = import_map_from_sharing(&path).unwrap();

        assert_eq!(imported.meta.name, doc.meta.name);
        assert_eq!(imported.meta.seed, doc.meta.seed);
        assert_eq!(imported.meta.width, doc.meta.width);
        assert_eq!(imported.meta.height, doc.meta.height);
        assert_eq!(imported.terrain.total_tiles, doc.terrain.total_tiles);
        assert_eq!(imported.terrain.rle_data, doc.terrain.rle_data);
        assert_eq!(imported.resources.entries, doc.resources.entries);
        assert_eq!(imported.entities.placements, doc.entities.placements);
        assert_eq!(imported.rules.zones.len(), doc.rules.zones.len());
        assert_eq!(imported.rules.triggers.len(), doc.rules.triggers.len());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_import_rejects_invalid_file() {
        // 导入一个 total_tiles 不匹配的文件应失败
        let mut doc = create_test_map();
        doc.terrain.total_tiles = 999; // 故意不匹配
        let dir = std::env::temp_dir().join("slg_save_test_import_reject");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("bad.slgmap");

        save_map_to_file(&doc, &path).unwrap();
        let result = import_map_from_sharing(&path);

        assert!(result.is_err());
        assert!(matches!(result, Err(SaveError::ValidationFailed(_))));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_generate_preview_png() {
        let doc = create_test_map();
        let png = generate_preview_png(&doc);

        // 应该是一个有效的 PNG（以 PNG 签名开头）
        assert!(png.len() > 8);
        assert_eq!(
            &png[0..8],
            &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]
        );
    }
}
