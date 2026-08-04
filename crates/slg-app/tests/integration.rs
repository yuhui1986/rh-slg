//! 集成测试：验证从启动到可玩的完整流程

use slg_core::clock::*;
use slg_core::gen::{generate_map, GenerationPreset};
use slg_core::map::grid::HexCoord;
use slg_core::map::loader::load_map;
use slg_core::resource::*;

#[test]
fn test_full_pipeline() {
    // 1. 生成地图
    let preset = GenerationPreset::default();
    let doc = generate_map(42, &preset);
    assert_eq!(doc.meta.width, 256);
    assert_eq!(doc.meta.height, 256);

    // 2. 加载地图
    let load_result = load_map(&doc);
    // 256/32 = 8, 8*8 = 64 chunks
    assert_eq!(load_result.chunk_data.len(), 64);
    assert!(!load_result.factions.is_empty());
    assert_eq!(load_result.factions.len(), 6);

    // 3. 验证出生点
    assert!(
        !load_result.entity_placements.is_empty(),
        "should have spawn points"
    );

    // 4. 模拟 100 tick
    let mut clock = GameClock {
        current_tick: 0,
        speed: Speed::X1,
        accumulator: 0.0,
    };

    for _ in 0..100 {
        advance_clock(&mut clock, 100.0); // 1 tick = 100ms
    }

    assert_eq!(clock.current_tick, 100);
}

#[test]
fn test_map_generation_determinism() {
    let preset = GenerationPreset::default();
    let doc1 = generate_map(42, &preset);
    let doc2 = generate_map(42, &preset);

    assert_eq!(doc1.meta.seed, doc2.meta.seed);
    assert_eq!(doc1.terrain.rle_data.len(), doc2.terrain.rle_data.len());
    assert_eq!(doc1.terrain.total_tiles, doc2.terrain.total_tiles);
}

#[test]
fn test_load_map_initializes_factions() {
    let preset = GenerationPreset::default();
    let doc = generate_map(42, &preset);
    let load_result = load_map(&doc);

    // 默认初始化 6 个势力
    assert_eq!(load_result.factions.len(), 6);
    assert!(load_result.factions.contains_key("faction_1"));
    assert!(load_result.factions.contains_key("faction_6"));

    // 每个势力有初始资源结构（默认为 0，游戏启动后会增长）
    for (id, faction) in &load_result.factions {
        // 验证资源结构存在且可访问
        let _ = faction.resources.gold;
        let _ = faction.resources.food;
        let _ = faction.resources.wood;
        let _ = faction.resources.iron;
        let _ = faction.resources.stone;
        assert_eq!(
            faction.resources.troops, 0,
            "{id} should start with 0 troops"
        );
    }
}

#[test]
fn test_spawn_points_create_territory() {
    let preset = GenerationPreset::default();
    let doc = generate_map(42, &preset);
    let load_result = load_map(&doc);

    // 出生点应有 faction_id
    let spawns_with_faction: Vec<_> = load_result
        .entity_placements
        .iter()
        .filter(|(_, e)| e.entity_type == "spawn" && e.faction_id.is_some())
        .collect();

    assert_eq!(
        spawns_with_faction.len(),
        6,
        "should have 6 spawn points with faction_id"
    );

    // 出生点应被记录为 tile_owners
    for (key, entity) in &load_result.entity_placements {
        if let Some(ref faction_id) = entity.faction_id {
            assert_eq!(
                load_result.tile_owners.get(key),
                Some(faction_id),
                "spawn tile should be owned by its faction"
            );
        }
    }
}

#[test]
fn test_100_tick_no_panic() {
    // 模拟完整的游戏循环 100 tick
    let preset = GenerationPreset::default();
    let doc = generate_map(42, &preset);
    let load_result = load_map(&doc);

    let mut clock = GameClock {
        current_tick: 0,
        speed: Speed::X1,
        accumulator: 0.0,
    };

    // 初始化势力
    let mut factions = load_result.factions;

    // 模拟100 tick 的资源产出
    for _ in 0..100 {
        let ticks = advance_clock(&mut clock, 100.0);
        for _ in 0..ticks {
            // 简化资源产出
            for faction in factions.values_mut() {
                faction.resources.gold += 10;
                faction.resources.food += 5;
            }
        }
    }

    assert_eq!(clock.current_tick, 100);

    // 验证资源增长
    for (id, faction) in &factions {
        assert!(
            faction.resources.gold > 0,
            "{id} should have accumulated gold"
        );
        assert!(
            faction.resources.food > 0,
            "{id} should have accumulated food"
        );
    }
}

#[test]
fn test_pause_resume_speed_control() {
    let mut clock = GameClock {
        current_tick: 0,
        speed: Speed::X1,
        accumulator: 0.0,
    };

    // 正常推进 0.5 秒 => 5 tick
    let ticks = advance_clock(&mut clock, 500.0);
    assert_eq!(ticks, 5);
    assert_eq!(clock.current_tick, 5);

    // 暂停
    clock.speed = Speed::Paused;
    let ticks = advance_clock(&mut clock, 5000.0);
    assert_eq!(ticks, 0);
    assert_eq!(clock.current_tick, 5);

    // 恢复 x1，再推 0.5 秒 => 又 5 tick
    clock.speed = Speed::X1;
    let ticks = advance_clock(&mut clock, 500.0);
    assert_eq!(ticks, 5);
    assert_eq!(clock.current_tick, 10);

    // 变速 x3
    clock.speed = Speed::X3;
    let ticks = advance_clock(&mut clock, 1000.0);
    assert_eq!(ticks, 30);
    assert_eq!(clock.current_tick, 40);
}

#[test]
fn test_terrain_distribution() {
    let preset = GenerationPreset::default();
    let doc = generate_map(42, &preset);

    // 解压 RLE 计算陆地格
    let mut land_count = 0u32;
    let mut water_count = 0u32;
    for (terrain_id, count) in &doc.terrain.rle_data {
        if terrain_id == "terrain_water" {
            water_count += count;
        } else {
            land_count += count;
        }
    }

    let total = land_count + water_count;
    assert_eq!(total, 256 * 256);

    // 陆地占比应 > 60%
    let ratio = land_count as f64 / total as f64;
    assert!(
        ratio > 0.60,
        "land ratio {:.1}% < 60% ({}/{})",
        ratio * 100.0,
        land_count,
        total
    );
}

#[test]
fn test_hex_coord_tile_key_roundtrip() {
    let coords = vec![
        HexCoord::new(0, 0),
        HexCoord::new(100, -200),
        HexCoord::new(-50, 300),
        HexCoord::new(255, 255),
    ];

    for coord in coords {
        let key = coord.to_tile_key();
        let back = HexCoord::from_tile_key(key);
        assert_eq!(
            coord, back,
            "roundtrip failed for ({}, {})",
            coord.q, coord.r
        );
    }
}

#[test]
fn test_territory_manager_occupy() {
    use slg_core::map::territory::TerritoryManager;

    let mut mgr = TerritoryManager::new(100);
    for y in 0..10 {
        for x in 0..10 {
            mgr.register_tile(HexCoord::new(x, y));
        }
    }

    let faction = "faction_1".to_string();
    let main_city = HexCoord::new(5, 5);
    mgr.set_main_city(&faction, main_city);
    mgr.occupy(main_city, &faction);

    // 占领相邻格
    let adjacent = HexCoord::new(6, 5);
    mgr.occupy(adjacent, &faction);

    assert_eq!(mgr.owner_map.get(&adjacent.to_tile_key()), Some(&faction));
}

#[test]
fn test_tick_phases_order() {
    // 确保 TICK_PHASES 有 9 个阶段且顺序正确
    assert_eq!(TICK_PHASES.len(), 9);
    assert_eq!(TICK_PHASES[0], TickPhase::TickStart);
    assert_eq!(TICK_PHASES[1], TickPhase::ResourceProduction);
    assert_eq!(TICK_PHASES[8], TickPhase::TickEnd);
}
