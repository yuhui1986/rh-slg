//! 端到端集成测试：模拟核心 SLG 玩法的完整数据流
//!
//! 验证：spawn → 派兵 → 行军推进 → 到达 → 占领 → 揭开迷雾 整条 pipeline
//!
//! 不依赖 Bevy，纯 Rust 在 slg-core 验证算法正确性。
//! Bevy 端的渲染 / egui 集成是另一层。

use std::collections::BTreeMap;

use slg_core::fog::{fill_chunk_fog_arr, FogOfWar, FOG_VISIBLE};
use slg_core::gen::generate_map;
use slg_core::gen::GenerationPreset;
use slg_core::map::grid::HexCoord;
use slg_core::map::loader::load_map;
use slg_core::map::territory::TerritoryManager;
use slg_core::map::tile::TerrainType;
use slg_core::military::{
    compute_path, MarchManager, TICKS_PER_HEX, TROOPS_PER_MARCH,
};
use slg_data::ids::FactionId;

/// 构造 sanguo_dl preset 用于集成测试
fn sanguo_dl_preset() -> GenerationPreset {
    GenerationPreset {
        name: "三国鼎立".to_string(),
        description: "E2E test preset".to_string(),
        width: 128,
        height: 128,
        seed: 42,
        terrain_style: 0.5,
        richness: 0.6,
        num_factions: 6,
        tags: vec!["三国".to_string()],
    }
}

/// 跑 sanguo_dl 完整 generate_map + load_map 流程
fn run_sanguo_dl() -> (slg_data::map_doc::MapDocument, slg_core::map::loader::LoadResult) {
    let preset = sanguo_dl_preset();
    let doc = generate_map(42, &preset);
    let load_result = load_map(&doc);
    (doc, load_result)
}

/// 收集主城列表 (HexCoord, FactionId) — 模拟 start_new_game 里 entity_placements → cities
fn collect_cities(load_result: &slg_core::map::loader::LoadResult) -> Vec<(HexCoord, FactionId)> {
    let mut cities = Vec::new();
    for (key, p) in &load_result.entity_placements {
        if p.entity_type == "spawn" {
            if let Some(fid) = &p.faction_id {
                cities.push((HexCoord::from_tile_key(*key), fid.clone()));
            }
        }
    }
    cities
}

/// 收集 terrain map (TileKey → TerrainType) — 模拟 start_new_game 里 terrain_map.map 填充
fn collect_terrain_map(
    load_result: &slg_core::map::loader::LoadResult,
) -> BTreeMap<u64, TerrainType> {
    let mut map = BTreeMap::new();
    for core_chunk in &load_result.chunk_data {
        let cx = core_chunk.chunk_x;
        let cy = core_chunk.chunk_y;
        for ly in 0..32u32 {
            for lx in 0..32u32 {
                let x = cx * 32 + lx;
                let y = cy * 32 + ly;
                let key = ((y as u64) << 32) | (x as u64);
                if let Some(t) = TerrainType::from_u8(core_chunk.terrains[(ly * 32 + lx) as usize]) {
                    map.insert(key, t);
                }
            }
        }
    }
    map
}

/// E2E 测试 1: 派兵 + 行军 + 到达 + 占地
///
/// 模拟 handle_hex_click 派兵 → process_tick_phases 推进 → 触发 occupy 的完整流程
#[test]
fn test_e2e_dispatch_march_arrive_occupy() {
    let (_doc, load_result) = run_sanguo_dl();
    let cities = collect_cities(&load_result);
    let terrain_map = collect_terrain_map(&load_result);

    // 玩家 = faction_1 (BTreeMap 第一个 key)
    let player_faction: FactionId = "faction_1".to_string();
    let player_city = cities
        .iter()
        .find(|(_, fid)| fid == &player_faction)
        .expect("player city")
        .0;
    eprintln!("E2E1: player city = ({}, {})", player_city.q, player_city.r);

    // 初始化 TerritoryManager（同 start_new_game）
    let mut manager = TerritoryManager::new((128 * 128) as usize);
    for r in 0..128i32 {
        for q in 0..128i32 {
            manager.register_tile(HexCoord::new(q, r));
        }
    }
    // 把玩家主城 occupy + set_main_city
    manager.set_main_city(&player_faction, player_city);
    manager.occupy(player_city, &player_faction);

    // 找玩家主城的一个邻接空地（passes can_occupy）
    let mut target: Option<HexCoord> = None;
    for n in player_city.neighbors() {
        if manager.can_occupy(n, &player_faction, &terrain_map) {
            target = Some(n);
            break;
        }
    }
    let target = target.expect("player city should have at least one passable neighbor");
    eprintln!("E2E1: target = ({}, {})", target.q, target.r);

    // 派兵
    let mut march = MarchManager::new();
    let order = march.dispatch(
        player_faction.clone(),
        player_city,
        target,
        TROOPS_PER_MARCH,
        100, // depart_tick
        None,
        "unit_infantry".to_string(),
    );
    eprintln!(
        "E2E1: dispatched march {} from=({},{}) to=({},{}) arrive_tick={} (1 hex)",
        order.id, player_city.q, player_city.r, target.q, target.r, order.arrive_tick
    );
    assert_eq!(order.path.len(), 2, "1-step path = from + to");
    assert_eq!(order.arrive_tick, 100 + TICKS_PER_HEX);

    // tick 100: 应该没有 arrival
    let arrivals_100 = march.advance_all(100);
    assert!(arrivals_100.is_empty(), "tick 100 should not have arrivals");

    // tick 104: 还没到
    let arrivals_104 = march.advance_all(104);
    assert!(arrivals_104.is_empty(), "tick 104 should not have arrivals");

    // tick 105: 到达
    let arrivals_105 = march.advance_all(105);
    assert_eq!(arrivals_105.len(), 1, "tick 105 should have 1 arrival");
    assert_eq!(arrivals_105[0].to, target);
    assert_eq!(arrivals_105[0].faction_id, player_faction);

    // 模拟 MarchAdvance phase: 在 arrival 时再 check can_occupy + 触发 occupy
    let arrival = &arrivals_105[0];
    let can = manager.can_occupy(arrival.to, &arrival.faction_id, &terrain_map);
    assert!(can, "target should still be passable");
    manager.occupy(arrival.to, &arrival.faction_id);
    eprintln!(
        "E2E1: ✅ 占地成功 ({}, {}) 现在归 {}",
        arrival.to.q, arrival.to.r, arrival.faction_id
    );

    // 验证 owner_map 里有 target
    let target_key = target.to_tile_key();
    assert_eq!(
        manager.owner_map.get(&target_key),
        Some(&player_faction),
        "target should now be owned by player"
    );

    // cleanup
    march.cleanup_finished();
    assert!(
        march.orders.is_empty(),
        "Arrived 后 cleanup_finished 应清掉"
    );
}

/// E2E 测试 2: 派兵 + 揭迷雾 完整 pipeline
///
/// 验证：派兵时 fog.reveal_path → fill_chunk_fog_arr 同步 → 目标 chunk 有 visible cells
#[test]
fn test_e2e_dispatch_reveals_fog_path() {
    let (_doc, load_result) = run_sanguo_dl();
    let cities = collect_cities(&load_result);
    let player_faction: FactionId = "faction_1".to_string();
    let player_city = cities
        .iter()
        .find(|(_, fid)| fid == &player_faction)
        .expect("player city")
        .0;

    // init fog
    let mut fog = FogOfWar::init_with_cities(128, 128, &cities, &player_faction);
    eprintln!("E2E2: fog init 玩家主城周围 7 格 visible");

    // 验证玩家主城揭开
    let arr = fill_chunk_fog_arr(
        (player_city.q as u32) / 32,
        (player_city.r as u32) / 32,
        &fog,
    );
    let before_visible: u32 = arr.iter().filter(|&&v| v == FOG_VISIBLE).count() as u32;
    eprintln!("E2E2: chunk 初始 visible = {} 格", before_visible);

    // 派兵到一个较远的位置（5 hex 距离），应该揭开整条路径
    let far_target = HexCoord::new(player_city.q + 5, player_city.r);
    let path = compute_path(player_city, far_target);
    eprintln!(
        "E2E2: path from=({},{}) to=({},{}) = {} 步",
        player_city.q, player_city.r, far_target.q, far_target.r, path.len() - 1
    );

    // 模拟 handle_hex_click 派兵后调用 reveal_coords_and_sync_chunks
    for coord in &path {
        fog.reveal_one(*coord);
    }

    // 验证：路径上的 hex 现在都是 visible
    for coord in &path {
        assert_eq!(
            fog.get(coord.q, coord.r),
            FOG_VISIBLE,
            "path 上的 hex ({},{}) 应该揭开",
            coord.q,
            coord.r
        );
    }

    // 验证：目标 chunk 的 fog_arr 增加 5+ 格 visible
    let cx = (far_target.q as u32) / 32;
    let cy = (far_target.r as u32) / 32;
    let arr_after = fill_chunk_fog_arr(cx, cy, &fog);
    let after_visible: u32 = arr_after.iter().filter(|&&v| v == FOG_VISIBLE).count() as u32;
    eprintln!(
        "E2E2: 派兵后目标 chunk({},{}) visible = {} 格",
        cx, cy, after_visible
    );
    assert!(
        after_visible >= 5,
        "派兵路径上 ~5 格应在目标 chunk 揭开, got {}",
        after_visible
    );
}

/// E2E 测试 3: 行军中的目标锁定
///
/// 验证：is_target_locked 防止两支兵同时飞向同一格
#[test]
fn test_e2e_target_locked_prevents_double_dispatch() {
    let (_doc, load_result) = run_sanguo_dl();
    let cities = collect_cities(&load_result);
    let player_faction: FactionId = "faction_1".to_string();
    let player_city = cities
        .iter()
        .find(|(_, fid)| fid == &player_faction)
        .expect("player city")
        .0;

    let mut march = MarchManager::new();
    let target = HexCoord::new(player_city.q + 1, player_city.r);

    // 派第一支兵
    let _ = march.dispatch(
        player_faction.clone(),
        player_city,
        target,
        TROOPS_PER_MARCH,
        100,
        None,
        "unit_infantry".to_string(),
    );
    assert!(
        march.is_target_locked(target),
        "target 应该在派兵后被锁住"
    );

    // 模拟 handle_hex_click 派第二支兵到同格
    // 实际代码：先 is_target_locked 检查，发现锁住 → 不派
    let can_dispatch_second = !march.is_target_locked(target);
    assert!(
        !can_dispatch_second,
        "锁住时不应允许派第二支兵到同格"
    );
    eprintln!("E2E3: ✅ 目标锁定生效，阻止双派");
}

/// E2E 测试 4: 完整的"开新游戏 → 派兵 → 行军 → 到达 → 占地 + 揭雾"
///
/// 把上面 3 个测试串起来，跑一个完整的 player 视角流程
#[test]
fn test_e2e_full_player_turn() {
    let (_doc, load_result) = run_sanguo_dl();
    let cities = collect_cities(&load_result);
    let terrain_map = collect_terrain_map(&load_result);
    let player_faction: FactionId = "faction_1".to_string();

    // ===== 开局状态 =====
    let player_city = cities
        .iter()
        .find(|(_, fid)| fid == &player_faction)
        .expect("player city")
        .0;
    eprintln!("E2E4: 开局 player = {}, city = ({},{})", player_faction, player_city.q, player_city.r);

    let mut manager = TerritoryManager::new((128 * 128) as usize);
    for r in 0..128i32 {
        for q in 0..128i32 {
            manager.register_tile(HexCoord::new(q, r));
        }
    }
    manager.set_main_city(&player_faction, player_city);
    manager.occupy(player_city, &player_faction);

    let mut fog = FogOfWar::init_with_cities(128, 128, &cities, &player_faction);
    let mut march = MarchManager::new();

    // ===== Player turn: 派 3 支兵到不同邻接 =====
    let neighbors: Vec<HexCoord> = player_city
        .neighbors()
        .iter()
        .filter(|n| manager.can_occupy(**n, &player_faction, &terrain_map))
        .copied()
        .collect();
    eprintln!("E2E4: 玩家可派兵的邻接数 = {}", neighbors.len());
    assert!(
        neighbors.len() >= 3,
        "玩家主城至少应有 3 个可派兵邻接, got {}",
        neighbors.len()
    );

    for (i, target) in neighbors.iter().take(3).enumerate() {
        let order = march.dispatch(
            player_faction.clone(),
            player_city,
            *target,
            TROOPS_PER_MARCH,
            100 + i as u64, // 错开出发 tick
            None,
            "unit_infantry".to_string(),
        );
        // 揭迷雾
        for c in &order.path {
            fog.reveal_one(*c);
        }
        eprintln!(
            "E2E4: 派兵 #{} to=({},{}) arrive_tick={}",
            i, target.q, target.r, order.arrive_tick
        );
    }

    // ===== Tick 推进: 100 ~ 110 =====
    for tick in 100..=110 {
        let arrivals = march.advance_all(tick);
        for arrival in &arrivals {
            if manager.can_occupy(arrival.to, &arrival.faction_id, &terrain_map) {
                manager.occupy(arrival.to, &arrival.faction_id);
                // 揭开到达 + 邻域
                fog.reveal_one(arrival.to);
                for n in arrival.to.neighbors() {
                    fog.reveal_one(n);
                }
                eprintln!(
                    "E2E4: ✅ tick={} 占地 ({},{}) + 揭邻域",
                    tick, arrival.to.q, arrival.to.r
                );
            }
        }
        march.cleanup_finished();
    }

    // ===== 验证最终状态 =====
    // 玩家应该占地 4 格（主城 + 3 个邻接）
    let player_tiles: u32 = manager
        .owner_map
        .values()
        .filter(|f| f == &&player_faction)
        .count() as u32;
    eprintln!("E2E4: 玩家最终占地数 = {}", player_tiles);
    assert_eq!(
        player_tiles, 4,
        "玩家占地应 = 1 主城 + 3 派兵落地 = 4 格, got {}",
        player_tiles
    );

    // 迷雾：玩家主城 + 3 邻接 + 邻接的 6 邻域 = 至少 1 + 3 + ~12 = ~16 格 visible
    let mut total_visible = 0u32;
    for chunk in fog.chunks.values() {
        for &v in &chunk.data {
            if v == FOG_VISIBLE {
                total_visible += 1;
            }
        }
    }
    eprintln!("E2E4: 全图迷雾 visible = {} 格", total_visible);
    assert!(
        total_visible >= 7,
        "至少玩家主城 + 6 邻域 = 7 格 visible, got {}",
        total_visible
    );

    eprintln!("E2E4: ✅ 完整 player turn pipeline 跑通：派兵 + 行军 + 占地 + 揭雾");
}
