//! slg-app: 《天下策》应用壳
//!
//! 负责窗口/入口、模式切换（游玩⇄编辑）、插件组装、崩溃日志。
//! 将所有已实现的子系统集成，实现从启动到可玩的完整流程。

use std::collections::BTreeMap;

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};
use slg_core::clock::*;
use slg_core::gen::{generate_map, GenerationPreset};
use slg_core::map::grid::HexCoord;
use slg_core::map::loader::*;
use slg_core::map::territory::TerritoryManager;
use slg_core::map::tile::TerrainType;
use slg_core::resource::*;
use slg_data::ids::{FactionId, TileKey};
use slg_engine::camera::{HexClickEvent, HexRightClickEvent};
use slg_engine::render::{
    build_chunk_mesh_with_transitions, chunk_world_offset, ChunkData as EngineChunkData,
};
use slg_engine::systems::GameClockResource;
use slg_ui::panels::game_over::{GameOverAction, GameOverState};
use slg_ui::panels::main_menu::{MainMenuAction, MainMenuState};
use slg_ui::panels::new_game::{Difficulty, GameSetupConfig, NewGameAction, NewGameState};

// ---------------------------------------------------------------------------
// 渲染/交互用 Resource（slg-core 不依赖 Bevy，所以这些在 slg-app 里定义）
// ---------------------------------------------------------------------------

/// FactionId → 势力色索引（0..=6）的映射
///
/// u8 索引用于 ChunkData.owners 数组的填充，对应 `atlas::faction_color` 的颜色：
/// - 0 = 无主
/// - 1..5 = 5 个 AI 势力（魏蜀吴辽东南中）
/// - 6 = 玩家（黄金）
///
/// 由 start_new_game 在加载地图时构建。
#[derive(Debug, Clone, Default, Resource)]
pub struct FactionIdMap {
    pub map: BTreeMap<FactionId, u8>,
}

impl FactionIdMap {
    pub fn get(&self, fid: &FactionId) -> u8 {
        self.map.get(fid).copied().unwrap_or(0)
    }
}

/// 全局地形图（TileKey → TerrainType），供 can_occupy 等逻辑查询
#[derive(Debug, Clone, Default, Resource)]
pub struct TerrainMapResource {
    pub map: BTreeMap<TileKey, TerrainType>,
}

/// 全局行军管理器 Resource：所有活跃 MarchOrder + ID 分配器
///
/// 派兵 / 推进 / 取消 / 锁定查询都走这个。
/// 纯 slg-core 数据 + Bevy Resource 包装，零引擎逻辑。
#[derive(Debug, Clone, Default, Resource)]
pub struct MarchManagerResource {
    pub manager: slg_core::military::MarchManager,
}

/// 全局资源格 Resource：TileKey → ResourceType
///
/// 从 `load_result.tile_resources` 填充，process_tick_phases 的
/// ResourceProduction 阶段用这个查每格的资源类型 → 算产量。
#[derive(Debug, Clone, Default, Resource)]
pub struct TileResourceMap {
    pub map: std::collections::BTreeMap<TileKey, slg_core::map::tile::ResourceType>,
}

/// 行军视觉 component：每个活跃 MarchOrder 对应一个 entity
///
/// sprite 位置 = lerp(from, to, march_manager.orders[id].progress(current_tick))
/// march_id 用来从 MarchManager 查 order
#[derive(Component)]
struct MarchVisual {
    march_id: u64,
}

// ---------------------------------------------------------------------------
// Steam 初始化
// ---------------------------------------------------------------------------

/// Steam 初始化
#[cfg(feature = "steam")]
pub fn init_steam() -> Result<(), String> {
    match steamworks::Client::init() {
        Ok(_client) => {
            info!("Steam 初始化成功");
            // 注册 Steam 资源
            Ok(())
        }
        Err(e) => {
            warn!("Steam 初始化失败（无 SDK 时正常）: {}", e);
            Ok(()) // 不阻塞游戏启动
        }
    }
}

/// 非 Steam 模式的空实现
#[cfg(not(feature = "steam"))]
pub fn init_steam() -> Result<(), String> {
    Ok(())
}

/// 《天下策》主插件
///
/// 组装所有子插件：引擎、UI、编辑器。
/// 注册游戏状态 Resource 和游戏循环系统。
pub struct SlgAppPlugin;

impl Plugin for SlgAppPlugin {
    fn build(&self, app: &mut App) {
        app
            // 注册子插件
            .add_plugins((
                slg_engine::SlgEnginePlugin,
                slg_ui::SlgUiPlugin,
                slg_editor::SlgEditorPlugin,
            ))
            // 注册游戏状态 Resource
            .init_resource::<GameState>()
            .init_resource::<FactionStoreResource>()
            .init_resource::<FogOfWarResource>()
            .init_resource::<TerritoryManagerResource>()
            .init_resource::<FactionIdMap>()
            .init_resource::<TerrainMapResource>()
            .init_resource::<MarchManagerResource>()
            .init_resource::<TileResourceMap>()
            // 启动系统：生成地图、初始化势力
            .add_systems(Startup, setup_game)
            // 主菜单动作处理 + 地图点击
            .add_systems(
                Update,
                (
                    handle_main_menu_actions,
                    handle_new_game_actions,
                    handle_game_over_actions,
                    handle_hex_click,
                    handle_hex_right_click,
                    render_editor_return,
                    render_map_debug,
                ),
            )
            // 输入链路诊断系统（设置 HEX_PICK_DEBUG=1 启用）
            .add_systems(Update, input_diagnostics)
            // 点击涟漪生命周期
            .add_systems(Update, update_click_rings)
            // 行军 sprite 插值移动（每帧）
            .add_systems(Update, march_sprite_system)
            // 胜利/失败检查（每帧 game phase = Playing 时跑）
            .add_systems(Update, check_victory_system)
            // 游戏循环系统：在 tick_dispatcher 之后运行
            .add_systems(
                Update,
                (process_tick_phases, update_ui_state).after(slg_engine::systems::tick_dispatcher),
            );
    }
}

// ---------------------------------------------------------------------------
// 游戏状态 Resource
// ---------------------------------------------------------------------------

/// 点击涟漪：每次左/右键点击地图时 spawn 一个白色圆环，1 秒后淡出消失。
/// 作用：给玩家**视觉反馈**，证明点击已被检测到——即使点到了地图外（hex 越界），
/// 也能看到圆环出现在点击位置，方便玩家理解坐标系。
#[derive(Component)]
struct ClickRing {
    /// 剩余生命（秒）
    lifetime: f32,
}

/// 每帧更新点击涟漪的 alpha 与生命周期
fn update_click_rings(
    mut commands: Commands,
    time: Res<Time>,
    mut query: Query<(Entity, &mut ClickRing, &mut Sprite)>,
) {
    for (entity, mut ring, mut sprite) in query.iter_mut() {
        ring.lifetime -= time.delta_secs();
        if ring.lifetime <= 0.0 {
            commands.entity(entity).despawn();
            continue;
        }
        // 剩余生命 0~1.0 → 线性 alpha 1.0→0.0
        let alpha = ring.lifetime.clamp(0.0, 1.0);
        sprite.color = Color::srgba(1.0, 1.0, 1.0, alpha);
    }
}

/// 主城 marker 组件：spawn 在每个势力主城的特殊 sprite（颜色 = 势力色）
#[derive(Component)]
struct MainCityMarker {
    #[allow(dead_code)] // 未来用于 hover/click → 弹出城池信息面板
    pub faction_id: FactionId,
    /// 主城 hex 坐标（debug 用：与 marker 的 world position 对比，确认是 hex 还是转换出错）
    pub hex: HexCoord,
}

/// 在点击位置 spawn 一个白色圆环
fn spawn_click_ring(commands: &mut Commands, world_pos: Vec2) {
    // 圆环大小需要跟相机 scale 反向关联：scale 越大（看得越远），圆环也越大
    // 简化方案：固定 20 世界单位（在 scale=1.0 下 20 像素，scale=3.0 下 60 像素，足够醒目）
    let size = 20.0_f32;
    // Bevy 0.15：直接 spawn 组件，不再用 SpriteBundle（已 deprecated）
    // Sprite 组件会自动插入默认的 Transform + GlobalTransform + Visibility
    commands.spawn((
        Sprite {
            color: Color::srgba(1.0, 1.0, 1.0, 1.0),
            custom_size: Some(Vec2::new(size, size)),
            ..default()
        },
        Transform::from_translation(Vec3::new(world_pos.x, world_pos.y, 2.0)),
        ClickRing { lifetime: 1.0 },
    ));
}

/// 游戏状态
#[derive(Resource)]
pub struct GameState {
    pub phase: GamePhase,
    pub tick: u64,
    /// 上一次处理的 tick，用于检测新 tick
    pub last_processed_tick: u64,
    /// 玩家势力 ID
    pub player_faction_id: String,
    /// 当前难度
    pub difficulty: Difficulty,
}

impl Default for GameState {
    fn default() -> Self {
        Self {
            phase: GamePhase::Menu,
            tick: 0,
            last_processed_tick: 0,
            player_faction_id: String::new(),
            difficulty: Difficulty::Normal,
        }
    }
}

/// 游戏阶段
#[derive(Default, PartialEq, Eq, Debug, Clone, Copy)]
pub enum GamePhase {
    #[default]
    Menu,
    NewGameSetup,
    Playing,
    Paused,
    Editor,
    GameOver,
}

// ---------------------------------------------------------------------------
// 势力存储 Resource
// ---------------------------------------------------------------------------

/// 势力存储 Bevy Resource 包装
#[derive(Resource)]
pub struct FactionStoreResource {
    pub store: FactionStore,
}

impl Default for FactionStoreResource {
    fn default() -> Self {
        Self {
            store: FactionStore {
                factions: BTreeMap::new(),
            },
        }
    }
}

// ---------------------------------------------------------------------------
// 领地管理 Resource
// ---------------------------------------------------------------------------

/// 领地管理 Bevy Resource 包装
#[derive(Resource)]
pub struct TerritoryManagerResource {
    pub manager: TerritoryManager,
}

impl Default for TerritoryManagerResource {
    fn default() -> Self {
        Self {
            manager: TerritoryManager::new(256 * 256),
        }
    }
}

// ---------------------------------------------------------------------------
// 迷雾 Resource
// ---------------------------------------------------------------------------

/// 迷雾 Bevy Resource 包装
#[derive(Resource)]
pub struct FogOfWarResource {
    pub fog: slg_core::fog::FogOfWar,
}

impl Default for FogOfWarResource {
    fn default() -> Self {
        Self {
            fog: slg_core::fog::FogOfWar::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// 启动系统
// ---------------------------------------------------------------------------

/// 启动游戏：仅显示主菜单，等待玩家选择
fn setup_game(
    mut clock_res: ResMut<GameClockResource>,
    mut menu_state: ResMut<MainMenuState>,
) {
    // 时钟暂停，等待玩家开始游戏
    clock_res.clock.speed = Speed::Paused;

    // 显示主菜单
    menu_state.show = true;

    info!("游戏启动完成，显示主菜单");
}

/// 根据配置生成地图并初始化游戏世界
#[allow(clippy::too_many_arguments)] // Bevy 系统的标准 pattern：start_new_game 由 handle_new_game_actions 在 NewGameAction::StartGame 时调用，参数均为系统 resource
fn start_new_game(
    config: &GameSetupConfig,
    game_state: &mut GameState,
    clock_res: &mut GameClockResource,
    faction_res: &mut FactionStoreResource,
    territory_res: &mut TerritoryManagerResource,
    faction_id_map: &mut FactionIdMap,
    terrain_map: &mut TerrainMapResource,
    fog_res: &mut FogOfWarResource,
    tile_res: &mut TileResourceMap,
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    existing_chunks: &Query<Entity, With<EngineChunkData>>,
    camera_query: &mut Query<&mut Transform, With<Camera2d>>,
    projection_query: &mut Query<&mut Projection, With<Camera2d>>,
    windows: &Query<&Window>,
) {
    // 根据剧本选择地图参数
    let (preset, seed) = match config.scenario_id.as_str() {
        "sanguo_dl" => {
            let p = GenerationPreset {
                name: "三国鼎立".to_string(),
                description: "魏蜀吴三分天下".to_string(),
                width: 128,
                height: 128,
                seed: 42,
                terrain_style: 0.5,
                richness: 0.6,
                num_factions: 6,
                tags: vec!["标准".to_string(), "三国".to_string()],
            };
            (p, 42u64)
        }
        _ => {
            // 沙盒模式 / 默认
            let p = GenerationPreset {
                name: "沙盒模式".to_string(),
                description: "自由探索".to_string(),
                width: 128,
                height: 128,
                seed: 123,
                terrain_style: 0.5,
                richness: 0.5,
                num_factions: 6,
                tags: vec!["沙盒".to_string()],
            };
            (p, 123u64)
        }
    };

    info!("生成地图：{} ({}x{}, seed={})", preset.name, preset.width, preset.height, seed);

    // 生成地图
    let doc = generate_map(seed, &preset);

    // 加载地图到运行时
    let load_result = load_map(&doc);

    // 清空旧数据
    faction_res.store.factions.clear();
    territory_res.manager = TerritoryManager::new((preset.width * preset.height) as usize);

    // 删除旧的 Chunk 实体
    for entity in existing_chunks.iter() {
        commands.entity(entity).despawn();
    }

    // 注册所有地块到 TerritoryManager
    for r in 0..(preset.height as i32) {
        for q in 0..(preset.width as i32) {
            territory_res.manager.register_tile(HexCoord::new(q, r));
        }
    }

    // 初始化势力
    let mut player_faction_id = String::new();
    for (id, state) in load_result.factions {
        // 第一个势力作为玩家势力（可后续根据 config 映射）
        if player_faction_id.is_empty() {
            player_faction_id = id.clone();
        }
        faction_res.store.factions.insert(id, state);
    }

    // 根据配置中的势力名映射玩家势力
    // 如果配置了具体的势力名，尝试匹配
    if !config.player_faction_name.is_empty() {
        for fid in faction_res.store.factions.keys() {
            // 简单匹配：faction_N 中 N 匹配配置名中的数字，或直接用第一个
            if fid.contains(&config.player_faction_name) {
                player_faction_id = fid.clone();
                break;
            }
        }
    }

    // 构建 FactionIdMap：玩家=6（黄金），AI 按 iteration 顺序分配 1..5
    // 这个映射决定 ChunkData.owners[i] 里写什么 u8，对应 atlas::faction_color 的颜色
    faction_id_map.map.clear();
    let mut ai_idx = 1u8;
    for fid in faction_res.store.factions.keys() {
        let color_idx = if fid == &player_faction_id {
            6 // 玩家
        } else {
            let v = ai_idx;
            ai_idx += 1;
            v
        };
        faction_id_map.map.insert(fid.clone(), color_idx);
    }
    info!(
        "FactionIdMap: player={}→6, AI 共 {} 个",
        player_faction_id,
        ai_idx - 1
    );

    // 初始化领地
    for (key, entity) in &load_result.entity_placements {
        if let Some(ref faction_id) = entity.faction_id {
            territory_res
                .manager
                .owner_map
                .insert(*key, faction_id.clone());
        }
    }

    // 从出生点设置势力主城
    for (key, entity) in &load_result.entity_placements {
        if entity.entity_type == "spawn" {
            if let Some(ref faction_id) = entity.faction_id {
                let coord = HexCoord::from_tile_key(*key);
                territory_res.manager.set_main_city(faction_id, coord);

                if let Some(faction) = faction_res.store.factions.get_mut(faction_id) {
                    faction.main_city = Some(coord);
                }
            }
        }
    }

    // 初始化迷雾：全黑，玩家主城 + 6 邻域揭开
    let cities: Vec<(HexCoord, FactionId)> = territory_res
        .manager
        .main_cities
        .iter()
        .map(|(fid, c)| (*c, fid.clone()))
        .collect();
    fog_res.fog = slg_core::fog::FogOfWar::init_with_cities(
        preset.width,
        preset.height,
        &cities,
        &player_faction_id,
    );
    info!("迷雾初始化: chunks={}, 玩家主城周围已揭开", fog_res.fog.chunks.len());

    // 更新游戏状态
    game_state.player_faction_id = player_faction_id.clone();
    game_state.difficulty = config.difficulty;
    game_state.tick = 0;
    game_state.last_processed_tick = 0;
    game_state.phase = GamePhase::Playing;

    // 启动时钟
    clock_res.clock.speed = Speed::X1;
    clock_res.clock.current_tick = 0;
    clock_res.clock.accumulator = 0.0;

    // 生成并渲染 Chunk 实体
    let chunk_count = load_result.chunk_data.len();

    // 先把 terrain 全部灌进 TerrainMap（供 can_occupy 查询）
    terrain_map.map.clear();
    for core_chunk in &load_result.chunk_data {
        let cx = core_chunk.chunk_x;
        let cy = core_chunk.chunk_y;
        for ly in 0..32u32 {
            for lx in 0..32u32 {
                let x = cx * 32 + lx;
                let y = cy * 32 + ly;
                let key = ((y as u64) << 32) | (x as u64);
                if let Some(t) = TerrainType::from_u8(core_chunk.terrains[(ly * 32 + lx) as usize]) {
                    terrain_map.map.insert(key, t);
                }
            }
        }
    }

    // 把 load_result.tile_resources 灌进 TileResourceMap（经济系统用）
    tile_res.map.clear();
    for (key, rt) in &load_result.tile_resources {
        tile_res.map.insert(*key, *rt);
    }

    for core_chunk in &load_result.chunk_data {
        // 关键修复：把 load_result.tile_owners 灌进每个 chunk 的 owners 数组
        // 之前 owners_chunk 永远是 [0; 1024]，导致地图上完全没势力色
        let mut owners = core_chunk.owners;
        let cx = core_chunk.chunk_x;
        let cy = core_chunk.chunk_y;
        for ly in 0..32u32 {
            for lx in 0..32u32 {
                let x = cx * 32 + lx;
                let y = cy * 32 + ly;
                let key = ((y as u64) << 32) | (x as u64);
                if let Some(fid) = load_result.tile_owners.get(&key) {
                    owners[(ly * 32 + lx) as usize] = faction_id_map.get(fid);
                }
            }
        }

        // 把 fog 灌进 chunk.fog 数组
        let mut fog_arr = [slg_core::fog::FOG_FOGGED; 1024];
        for ly in 0..32u32 {
            for lx in 0..32u32 {
                let x = (cx * 32 + lx) as i32;
                let y = (cy * 32 + ly) as i32;
                fog_arr[(ly * 32 + lx) as usize] = fog_res.fog.get(x, y);
            }
        }

        let engine_chunk = EngineChunkData {
            chunk_x: core_chunk.chunk_x as i32,
            chunk_y: core_chunk.chunk_y as i32,
            terrains: core_chunk.terrains,
            owners, // 已填充归属
            levels: core_chunk.levels,
            fog: fog_arr, // 已填充迷雾
            dirty: true,
            current_lod: 0,
        };

        let mesh = build_chunk_mesh_with_transitions(&core_chunk.terrains, &owners, &fog_arr);
        let mesh_handle = meshes.add(mesh);
        let material_handle = materials.add(ColorMaterial::default());

        let offset = chunk_world_offset(engine_chunk.chunk_x, engine_chunk.chunk_y);

        commands.spawn((
            engine_chunk,
            Mesh2d(mesh_handle),
            MeshMaterial2d(material_handle),
            Transform::from_translation(Vec3::new(offset.x, offset.y, 0.0)),
            Visibility::default(),
            GlobalTransform::default(),
        ));
    }

    // 居中相机到地图中心
    // 地图最后一个 chunk 的位置
    let chunks_x = preset.width.div_ceil(32);
    let chunks_y = preset.height.div_ceil(32);
    let chunk_w = 32.0 * slg_engine::render::chunk_mesh::COL_SPACING;
    let chunk_h = 32.0 * slg_engine::render::chunk_mesh::ROW_SPACING;
    let map_total_w = chunks_x as f32 * chunk_w;
    let map_total_h = chunks_y as f32 * chunk_h;
    let map_center_x = map_total_w * 0.5;
    let map_center_y = map_total_h * 0.5;
    if let Ok(mut transform) = camera_query.get_single_mut() {
        transform.translation = Vec3::new(map_center_x, map_center_y, 0.0);
    }

    // 生成主城 marker：每个势力的主城位置画一个"外黑内白"的双色 sprite
    //
    // 关键：直接用势力色 + alpha 0.7 在小 scale 下会跟同色地形融化（玩家金色基地
    // 放在金色地块上根本看不出），所以用**黑色外框 + 白色内核**的对比组合：
    //   - 外框 (2.4x hex, 黑色) 在任何地形色上都清晰
    //   - 内核 (1.4x hex, 白色) 在黑框内醒目，能告诉玩家"这是特殊位置"
    // 玩家一眼看到自己的白点 = 基地。
    for fid in faction_res.store.factions.keys() {
        if let Some(faction) = faction_res.store.factions.get(fid) {
            if let Some(main_coord) = faction.main_city {
                let center = slg_engine::camera::hex_world_position(main_coord);
                let hex_size = slg_engine::render::chunk_mesh::HEX_SIZE;
                // 外框（黑）—— z=1.4
                commands.spawn((
                    Sprite {
                        color: Color::srgba(0.0, 0.0, 0.0, 0.95),
                        custom_size: Some(Vec2::new(hex_size * 2.4, hex_size * 2.4)),
                        ..default()
                    },
                    Transform::from_translation(Vec3::new(center.x, center.y, 1.4)),
                ));
                // 内核（白）—— z=1.5，盖在外框之上
                commands.spawn((
                    Sprite {
                        color: Color::srgba(1.0, 1.0, 1.0, 1.0),
                        custom_size: Some(Vec2::new(hex_size * 1.4, hex_size * 1.4)),
                        ..default()
                    },
                    Transform::from_translation(Vec3::new(center.x, center.y, 1.5)),
                    MainCityMarker { faction_id: fid.clone(), hex: main_coord },
                ));
            }
        }
    }
    // 自适应缩放：根据窗口实际尺寸让地图填满视口（margin=1.0 → 整张地图贴边显示）。
    // 旧代码硬编码 scale=1.0 + 80% margin：720p 下视口=1280×720 世界单位，
    // 128×128 hex 地图（≈222×192 世界单位）只占约 1/5 屏幕，大量空白
    // 玩家很容易点到地图外（hex 越界被 handler 静默 continue 掉，看着像"没反应"）。
    // margin=1.0 让地图短边贴边，y 方向铺满窗口；左右仍有少量空白（地图比窗口更方）
    // 但配合 spawn_click_ring 视觉反馈，玩家不会再困惑"点没点上"。
    let window_size = windows.single().resolution.size();
    let fit_scale =
        slg_engine::camera::compute_fit_ortho_scale(map_total_w, map_total_h, window_size.x, window_size.y, 1.0);
    if let Ok(mut projection) = projection_query.get_single_mut() {
        if let Projection::Orthographic(ref mut ortho) = *projection {
            ortho.scale = fit_scale;
        }
    }

    info!(
        "新游戏已开始：剧本={}, 玩家势力={}, 难度={:?}, {} 个势力, {} 个 Chunk, 相机居中({}, {}), scale={:.3} (window={}x{})",
        config.scenario_id,
        player_faction_id,
        config.difficulty,
        faction_res.store.factions.len(),
        chunk_count,
        map_center_x,
        map_center_y,
        fit_scale,
        window_size.x,
        window_size.y,
    );
}

// ---------------------------------------------------------------------------
// 主菜单动作处理
// ---------------------------------------------------------------------------

/// 处理主菜单动作事件，切换游戏阶段
fn handle_main_menu_actions(
    mut action_events: EventReader<MainMenuAction>,
    mut game_state: ResMut<GameState>,
    mut menu_state: ResMut<MainMenuState>,
    mut new_game_state: ResMut<NewGameState>,
) {
    for action in action_events.read() {
        match action {
            MainMenuAction::NewGame => {
                game_state.phase = GamePhase::NewGameSetup;
                menu_state.show = false;
                new_game_state.show = true;
                new_game_state.step = Default::default();
                new_game_state.config = Default::default();
                info!("玩家选择：新游戏");
            }
            MainMenuAction::ContinueGame => {
                // TODO: 扫描存档并显示列表
                info!("玩家选择：继续游戏（暂无存档功能）");
            }
            MainMenuAction::Editor => {
                game_state.phase = GamePhase::Editor;
                menu_state.show = false;
                info!("玩家选择：编辑器");
            }
            MainMenuAction::Settings => {
                // TODO: 打开设置面板
                info!("玩家选择：设置（尚未实现）");
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 新游戏动作处理
// ---------------------------------------------------------------------------

/// 处理新游戏动作事件，生成地图并开始游戏
#[allow(clippy::too_many_arguments)] // Bevy 系统的标准 pattern
#[allow(clippy::explicit_auto_deref)] // &mut *res 是把 ResMut<T> 借成 &mut T 的惯用写法，自动解引用不会在这里发生
fn handle_new_game_actions(
    mut commands: Commands,
    mut action_events: EventReader<NewGameAction>,
    mut game_state: ResMut<GameState>,
    mut clock_res: ResMut<GameClockResource>,
    mut faction_res: ResMut<FactionStoreResource>,
    mut territory_res: ResMut<TerritoryManagerResource>,
    mut faction_id_map: ResMut<FactionIdMap>,
    mut terrain_map: ResMut<TerrainMapResource>,
    mut fog_res: ResMut<FogOfWarResource>,
    mut tile_res: ResMut<TileResourceMap>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    existing_chunks: Query<Entity, With<EngineChunkData>>,
    mut camera_query: Query<&mut Transform, With<Camera2d>>,
    mut projection_query: Query<&mut Projection, With<Camera2d>>,
    windows: Query<&Window>,
) {
    for action in action_events.read() {
        match action {
            NewGameAction::StartGame(config) => {
                start_new_game(
                    config,
                    &mut *game_state,
                    &mut *clock_res,
                    &mut *faction_res,
                    &mut *territory_res,
                    &mut *faction_id_map,
                    &mut *terrain_map,
                    &mut *fog_res,
                    &mut *tile_res,
                    &mut commands,
                    &mut meshes,
                    &mut materials,
                    &existing_chunks,
                    &mut camera_query,
                    &mut projection_query,
                    &windows,
                );
            }
            NewGameAction::BackToMenu => {
                game_state.phase = GamePhase::Menu;
                info!("返回主菜单");
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 游戏结束动作处理
// ---------------------------------------------------------------------------

/// 处理游戏结束画面的动作事件
fn handle_game_over_actions(
    mut action_events: EventReader<GameOverAction>,
    mut game_state: ResMut<GameState>,
    mut game_over_state: ResMut<GameOverState>,
    mut menu_state: ResMut<MainMenuState>,
    mut new_game_state: ResMut<NewGameState>,
) {
    for action in action_events.read() {
        match action {
            GameOverAction::NewGame => {
                game_over_state.show = false;
                game_state.phase = GamePhase::NewGameSetup;
                game_state.player_faction_id.clear();
                new_game_state.show = true;
                new_game_state.step = Default::default();
                new_game_state.config = Default::default();
                info!("再来一局");
            }
            GameOverAction::MainMenu => {
                game_over_state.show = false;
                game_state.phase = GamePhase::Menu;
                menu_state.show = true;
                info!("返回主菜单");
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 游戏循环系统
// ---------------------------------------------------------------------------

/// 处理每个 tick 的各阶段
///
/// 在 tick_dispatcher 之后运行，检测新 tick 并执行游戏逻辑。
/// tick_dispatcher 负责时钟推进，本系统负责游戏逻辑处理。
#[allow(clippy::too_many_arguments)] // Bevy 系统的标准 pattern：每个 Resource / Query 一个参数
fn process_tick_phases(
    clock_res: Res<GameClockResource>,
    mut faction_res: ResMut<FactionStoreResource>,
    mut command_res: ResMut<slg_engine::systems::CommandQueueResource>,
    mut territory_res: ResMut<TerritoryManagerResource>,
    mut march_res: ResMut<MarchManagerResource>,
    mut fog_res: ResMut<FogOfWarResource>,
    faction_id_map: Res<FactionIdMap>,
    terrain_map: Res<TerrainMapResource>,
    tile_res: Res<TileResourceMap>,
    mut game_state: ResMut<GameState>,
    mut chunk_query: Query<&mut EngineChunkData>,
) {
    // 只在游戏进行中处理 tick
    if game_state.phase != GamePhase::Playing {
        return;
    }

    let current_tick = clock_res.clock.current_tick;

    // 只在新 tick 时处理
    if current_tick == game_state.last_processed_tick {
        return;
    }

    // 处理所有累积的 tick
    while game_state.last_processed_tick < current_tick {
        game_state.last_processed_tick += 1;
        game_state.tick = game_state.last_processed_tick;

        for phase in TICK_PHASES {
            match phase {
                TickPhase::TickStart => {
                    // 注入暂停时入队的指令
                    while let Some(cmd) = command_res.queue.commands.pop_front() {
                        execute_command(cmd, &mut faction_res, &mut territory_res);
                    }
                }
                TickPhase::ResourceProduction => {
                    // 圈地产资源：遍历所有圈地按地形 + 资源格算产量
                    // 之前是每个势力 +10 gold +5 food（硬编码，与圈地无关）
                    // 现在按 slg-core::economy 真实计算
                    let production = slg_core::economy::tick_resource_production(
                        &terrain_map.map,
                        &tile_res.map,
                        &territory_res.manager.owner_map,
                    );
                    for (faction_id, prod) in &production {
                        if let Some(faction) = faction_res.store.factions.get_mut(faction_id) {
                            slg_core::economy::apply_production(&mut faction.resources, prod);
                        }
                    }
                }
                TickPhase::MarchAdvance => {
                    // 行军推进：检查到达、触发 occupy、清理已完成的
                    let arrivals = march_res.manager.advance_all(game_state.tick);
                    for arrival in arrivals {
                        // 到达：再 check 一次 can_occupy（行军期间可能被 NPC 抢了）
                        let can = territory_res.manager.can_occupy(
                            arrival.to,
                            &arrival.faction_id,
                            &terrain_map.map,
                        );
                        if can {
                            territory_res.manager.occupy(arrival.to, &arrival.faction_id);
                            // 同步 chunk owner
                            let cx = arrival.to.q / 32;
                            let cy = arrival.to.r / 32;
                            let lx = (arrival.to.q % 32) as usize;
                            let ly = (arrival.to.r % 32) as usize;
                            let local_idx = ly * 32 + lx;
                            let color_idx = faction_id_map.get(&arrival.faction_id);
                            for mut chunk in chunk_query.iter_mut() {
                                if chunk.chunk_x == cx && chunk.chunk_y == cy {
                                    chunk.owners[local_idx] = color_idx;
                                    chunk.dirty = true;
                                    break;
                                }
                            }
                            // 揭开到达格 + 6 邻域（永久：探索到 = 看到邻接）
                            let mut to_reveal = vec![arrival.to];
                            to_reveal.extend(arrival.to.ring(1));
                            reveal_coords_and_sync_chunks(
                                &mut fog_res,
                                &mut chunk_query,
                                &to_reveal,
                            );
                            info!(
                                "[MarchAdvance] ✅ 到达占地: ({}, {}) → {}, 揭开邻域",
                                arrival.to.q, arrival.to.r, arrival.faction_id
                            );
                        } else {
                            info!(
                                "[MarchAdvance] ❌ 到达但无法占地（被先占）: ({}, {})",
                                arrival.to.q, arrival.to.r
                            );
                            march_res.manager.fail(arrival.id);
                        }
                    }
                    march_res.manager.cleanup_finished();
                }
                TickPhase::AIDecision => {
                    // AI 决策（错峰）
                    for (i, (_faction_id, _faction)) in
                        faction_res.store.factions.iter_mut().enumerate()
                    {
                        if should_ai_decide(game_state.tick, i as u8) {
                            // 简化 AI：后续填充完整实现
                            // 完整实现在 slg-core::ai::tick_ai
                        }
                    }
                }
                _ => {
                    // 其他阶段：BuildQueue, Recruitment, CombatResolution,
                    // TerritoryUpdate, TickEnd
                    // 简化实现，后续迭代填充
                }
            }
        }
    }
}

/// 执行玩家指令
fn execute_command(
    cmd: PlayerCommand,
    _faction_res: &mut FactionStoreResource,
    territory_res: &mut TerritoryManagerResource,
) {
    match cmd {
        PlayerCommand::OccupyTile(coord, faction_id) => {
            // 简化：直接占领
            territory_res.manager.occupy(coord, &faction_id);
        }
        _ => {
            // 其他指令后续实现
        }
    }
}

/// 更新 UI 状态
///
/// 将游戏状态同步到 UI 面板。
fn update_ui_state(
    game_state: Res<GameState>,
    clock_res: Res<GameClockResource>,
    faction_res: Res<FactionStoreResource>,
    march_res: Res<MarchManagerResource>,
    mut top_bar: ResMut<slg_ui::panels::top_bar::TopBarState>,
) {
    // 控制顶栏显示/隐藏
    top_bar.show = game_state.phase == GamePhase::Playing;

    if !top_bar.show {
        return;
    }

    // 读取玩家势力资源
    if let Some(player_faction) = faction_res.store.factions.get(&game_state.player_faction_id) {
        top_bar.gold = player_faction.resources.gold;
        top_bar.food = player_faction.resources.food;
        top_bar.wood = player_faction.resources.wood;
        top_bar.iron = player_faction.resources.iron;
        top_bar.stone = player_faction.resources.stone;
    }
    top_bar.tick = game_state.tick;
    top_bar.speed = format!("{:?}", clock_res.clock.speed);
    top_bar.marching_count = march_res.manager.active().count() as u32;
}

/// 检查胜利/失败条件
///
/// 每 tick 调用：检查玩家主城是否被推、玩家占地比例是否达阈值。
/// 触发时设 `game_state.phase = GamePhase::GameOver` + 设置 GameOverState。
fn check_victory_system(
    mut game_state: ResMut<GameState>,
    faction_res: Res<FactionStoreResource>,
    territory_res: Res<TerritoryManagerResource>,
    terrain_map: Res<TerrainMapResource>,
    mut game_over_state: ResMut<slg_ui::panels::game_over::GameOverState>,
    clock_res: Res<slg_engine::systems::GameClockResource>,
) {
    if game_state.phase != GamePhase::Playing {
        return;
    }

    let player_faction = game_state.player_faction_id.clone();
    let Some(faction) = faction_res.store.factions.get(&player_faction) else {
        return;
    };
    let Some(main_city) = faction.main_city else {
        return;
    };

    let reason = slg_core::victory::check_victory_and_defeat(
        main_city,
        &territory_res.manager.owner_map,
        &terrain_map.map,
        &player_faction,
        slg_core::victory::DEFAULT_VICTORY_RATIO,
    );

    if let Some(reason) = reason {
        let is_victory = reason.is_victory();
        let reason_text = reason.reason_text();
        // 只在第一次触发时 log
        if !game_over_state.show {
            info!(
                "🎮 GameOver 触发: {} (tick={})",
                reason_text, clock_res.clock.current_tick
            );
        }
        game_over_state.show = true;
        game_over_state.is_victory = is_victory;
        game_over_state.reason = reason_text;
        // 玩家占地数填 statistics
        let tiles_occupied: u32 = territory_res
            .manager
            .owner_map
            .values()
            .filter(|f| f == &&player_faction)
            .count() as u32;
        game_over_state.statistics.tiles_occupied = tiles_occupied;
        game_state.phase = GamePhase::GameOver;
    }
}

/// 揭开若干 hex 并同步到对应 chunk 的 fog 数组 + 标 dirty
///
/// 流程：
/// 1. fog manager 揭开（持久数据）
/// 2. 每个 coord 找到对应 chunk，更新 `chunk.fog[idx] = 1`，设 dirty
///
/// 用于：
/// - 派兵时揭开路径（handle_hex_click）
/// - 到达时揭开邻域（process_tick_phases）
/// - 玩家主城初始化（start_new_game 直接填 chunk.fog，不走这里）
#[allow(clippy::too_many_arguments)]
fn reveal_coords_and_sync_chunks(
    fog_res: &mut FogOfWarResource,
    chunk_query: &mut Query<&mut EngineChunkData>,
    coords: &[HexCoord],
) {
    use slg_core::fog::{FOG_VISIBLE, CHUNK_SIZE};
    for coord in coords {
        fog_res.fog.reveal_one(*coord);
        if coord.q < 0 || coord.r < 0 {
            continue;
        }
        let q = coord.q as u32;
        let r = coord.r as u32;
        let cx = q / CHUNK_SIZE;
        let cy = r / CHUNK_SIZE;
        let lx = (q % CHUNK_SIZE) as usize;
        let ly = (r % CHUNK_SIZE) as usize;
        let local_idx = ly * CHUNK_SIZE as usize + lx;
        for mut chunk in chunk_query.iter_mut() {
            if chunk.chunk_x == cx as i32 && chunk.chunk_y == cy as i32 {
                chunk.fog[local_idx] = FOG_VISIBLE;
                chunk.dirty = true;
                break;
            }
        }
    }
}

/// 更新行军 sprite 位置（每帧插值）
///
/// 从 MarchManager 读 progress，把 MarchVisual entity 的 transform.translation
/// 插值到 lerp(from, to, progress)。
///
/// 到达 / 失败 / 取消的 MarchOrder 会被 `march_advance_system` 或 cancel 改成
/// 非 Marching 状态，这里查到 status != Marching 就 despawn sprite。
fn march_sprite_system(
    clock_res: Res<GameClockResource>,
    march_res: Res<MarchManagerResource>,
    mut query: Query<(Entity, &MarchVisual, &mut Transform)>,
    mut commands: Commands,
) {
    let current_tick = clock_res.clock.current_tick;
    for (entity, visual, mut transform) in query.iter_mut() {
        // 从 manager 查 order 状态
        let order_info = march_res.manager.orders.get(&visual.march_id);
        match order_info {
            Some(order) if order.status == slg_core::military::MarchStatus::Marching => {
                // 插值位置 = lerp(from_world, to_world, progress)
                let from_world = slg_engine::camera::hex_world_position(order.from);
                let to_world = slg_engine::camera::hex_world_position(order.to);
                let t = order.progress(current_tick);
                transform.translation.x = from_world.x + (to_world.x - from_world.x) * t;
                transform.translation.y = from_world.y + (to_world.y - from_world.y) * t;
            }
            _ => {
                // 到达 / 取消 / 失败：despawn sprite
                commands.entity(entity).despawn();
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 编辑器返回主菜单
// ---------------------------------------------------------------------------

/// 编辑器模式下显示返回主菜单按钮
fn render_editor_return(
    mut contexts: EguiContexts,
    mut game_state: ResMut<GameState>,
    mut menu_state: ResMut<MainMenuState>,
    mut editor_state: ResMut<slg_editor::scenario_editor::ScenarioEditorState>,
    mut rule_state: ResMut<slg_editor::rule_editor::RuleEditorState>,
) {
    if game_state.phase != GamePhase::Editor {
        return;
    }

    let ctx = contexts.ctx_mut();

    egui::TopBottomPanel::top("editor_toolbar").show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.label("🖌 编辑器模式");
            ui.separator();
            if ui.button("返回主菜单").clicked() {
                editor_state.show = false;
                rule_state.show = false;
                game_state.phase = GamePhase::Menu;
                menu_state.show = true;
                info!("退出编辑器，返回主菜单");
            }
        });
    });
}

/// 地图调试信息（临时，用于排查渲染问题）
#[allow(clippy::too_many_arguments)] // 调试面板：参数都是只读 Resource / Query，独立运行时无副作用
fn render_map_debug(
    mut contexts: EguiContexts,
    game_state: Res<GameState>,
    chunk_query: Query<&EngineChunkData>,
    camera_query: Query<&Transform, With<Camera2d>>,
    projection_query: Query<&Projection, With<Camera2d>>,
    pick_result: Res<slg_engine::camera::HexPickResult>,
    mouse_button: Res<ButtonInput<MouseButton>>,
    main_city_markers: Query<(&MainCityMarker, &Transform)>,
    faction_res: Res<FactionStoreResource>,
) {
    if game_state.phase != GamePhase::Playing {
        return;
    }

    let ctx = contexts.ctx_mut();

    // 改用 SidePanel 而不是 Window：
    // - Window 是 floating，可能盖住中央；title bar / 边框实际 region 与 fixed_size 行为不可控
    // - SidePanel 物理上贴边，绝对不会盖住中央地图
    egui::SidePanel::right("debug_panel")
        .default_width(320.0)
        .resizable(false)
        .show(ctx, |ui| {
            // 相机
            if let Ok(t) = camera_query.get_single() {
                ui.label(format!("相机: ({:.1}, {:.1})", t.translation.x, t.translation.y));
            }
            if let Ok(Projection::Orthographic(ortho)) = projection_query.get_single() {
                let zoom = 1.0 / ortho.scale;
                ui.label(format!("缩放: {:.2}, zoom: {:.4}", ortho.scale, zoom));
            }

            // Chunk
            let count = chunk_query.iter().count();
            ui.label(format!("Chunk 数: {}", count));

            // 地形统计
            let mut terrain_counts = [0u32; 8];
            for chunk in chunk_query.iter() {
                for &t in &chunk.terrains {
                    if (t as usize) < 8 {
                        terrain_counts[t as usize] += 1;
                    }
                }
            }
            ui.label(format!(
                "平原={} 水={} 山={} 森={} 沙={} 沼={} 丘={} 关={}",
                terrain_counts[0], terrain_counts[2], terrain_counts[1],
                terrain_counts[3], terrain_counts[4], terrain_counts[5],
                terrain_counts[6], terrain_counts[7]
            ));

            // 第一个chunk LOD
            if let Some(chunk) = chunk_query.iter().next() {
                ui.label(format!("Chunk(0,0) LOD={}, dirty={}", chunk.current_lod, chunk.dirty));
            }

            ui.separator();
            ui.label("─── 主城 ───");
            // 主城 marker 数 + 每个的坐标（用来核对"只看到 5 个"是哪 5 个）
            let mut city_lines: Vec<String> = Vec::new();
            city_lines.push(format!("主城 marker 总数: {} (期望 6)", main_city_markers.iter().count()));
            for (marker, t) in main_city_markers.iter() {
                // 打印 hex (q, r) + world (x, y)：立刻能看出是 hex 本身为负
                // 还是 hex→world 转换出错
                city_lines.push(format!(
                    "  • {} hex=({}, {}) world=({:.1}, {:.1})",
                    marker.faction_id,
                    marker.hex.q,
                    marker.hex.r,
                    t.translation.x,
                    t.translation.y
                ));
            }
            // 玩家势力 + 主城坐标（期望玩家 ID = FactionIdMap.get(6) 的 key）
            if let Some(player_faction) = faction_res.store.factions.get(&game_state.player_faction_id) {
                if let Some(mc) = player_faction.main_city {
                    city_lines.push(format!(
                        "玩家 {} 主城 hex=({}, {})",
                        game_state.player_faction_id,
                        mc.q,
                        mc.r
                    ));
                }
            }
            for line in city_lines {
                ui.label(line);
            }

            ui.separator();
            ui.label("─── 迷雾 ───");
            // 统计全图 fog 状态：可见 / 全部 / 百分比
            // 用来核对 fog init 是否生效（玩家主城周围应该有 ~7 格可见）
            let mut visible = 0u32;
            let mut total = 0u32;
            for chunk in chunk_query.iter() {
                for &f in &chunk.fog {
                    total += 1;
                    if f == 1 {
                        visible += 1;
                    }
                }
            }
            let pct = if total > 0 { visible / (total / 100).max(1) } else { 0 };
            ui.label(format!("可见: {}/{} ({}%)", visible, total, pct));

            ui.separator();
            ui.label("─── 鼠标/拾取 ───");

            // hex 拾取结果
            let hex_str = match pick_result.coord {
                Some(c) => format!("({}, {})", c.q, c.r),
                None => "None".to_string(),
            };
            ui.label(format!("hex 拾取: {}", hex_str));

            // 鼠标按键状态
            ui.label(format!(
                "鼠标: L={} L_just={} R={} R_just={}",
                mouse_button.pressed(MouseButton::Left),
                mouse_button.just_pressed(MouseButton::Left),
                mouse_button.pressed(MouseButton::Right),
                mouse_button.just_pressed(MouseButton::Right),
            ));

            // egui 拦截状态
            ui.label(format!("egui 拦截: {}", ctx.is_using_pointer()));
        });
}

// ---------------------------------------------------------------------------
// 地图点击事件处理
// ---------------------------------------------------------------------------

/// 处理地图地块点击事件
///
/// 当用户在非 egui 区域左键点击地图时，CameraPlugin 的 hex_click 系统
/// 会发送 HexClickEvent。本系统读取事件并根据游戏阶段分发处理。
///
/// **注意**：spawn_click_ring **在越界检查之前**就执行，保证玩家在地图外点击
/// 也能看到圆环反馈，方便他们理解坐标系。
#[allow(clippy::too_many_arguments)]
fn handle_hex_click(
    mut commands: Commands,
    mut click_events: EventReader<HexClickEvent>,
    game_state: Res<GameState>,
    mut territory_res: ResMut<TerritoryManagerResource>,
    mut march_res: ResMut<MarchManagerResource>,
    mut fog_res: ResMut<FogOfWarResource>,
    terrain_map: Res<TerrainMapResource>,
    clock_res: Res<slg_engine::systems::GameClockResource>,
    mut chunk_query: Query<&mut EngineChunkData>,
) {
    for event in click_events.read() {
        let coord = event.coord;

        // 总是先 spawn 涟漪，让玩家看到点击已检测
        spawn_click_ring(&mut commands, event.world_pos);

        // 坐标合法性检查：必须在地图范围内
        // TODO: 从地图元数据获取实际尺寸，当前硬编码 128x128
        if coord.q < 0 || coord.r < 0 || coord.q >= 128 || coord.r >= 128 {
            info!(
                "[handle_hex_click] 忽略越界点击: ({}, {})",
                coord.q, coord.r
            );
            continue;
        }

        match game_state.phase {
            GamePhase::Playing => {
                // **核心 SLG 循环：左键 → 派兵（行军）→ 到达 → 占地**
                // 之前是直接 occupy（瞬时），现在改成派兵走完才落地。
                let player_fid = game_state.player_faction_id.clone();
                let can = territory_res
                    .manager
                    .can_occupy(coord, &player_fid, &terrain_map.map);
                if !can {
                    let owner = territory_res.manager.owner_map.get(&coord.to_tile_key());
                    info!(
                        "[Playing] ❌ 不能派兵: ({}, {}), 当前归属={:?}",
                        coord.q, coord.r, owner
                    );
                    continue;
                }

                // 目标格是否已被另一支行军锁住？
                if march_res.manager.is_target_locked(coord) {
                    info!(
                        "[Playing] ❌ 目标已被行军锁住: ({}, {})",
                        coord.q, coord.r
                    );
                    continue;
                }

                // 派出：从最近的主城出发
                // MVP 简化：从玩家主城出发；M1 改成从当前最前线的己方格出发
                let from = match territory_res.manager.main_cities.get(&player_fid) {
                    Some(&c) => c,
                    None => {
                        warn!("[Playing] 玩家 {} 没有主城，无法派兵", player_fid);
                        continue;
                    }
                };

                let order = march_res.manager.dispatch(
                    player_fid.clone(),
                    from,
                    coord,
                    slg_core::military::TROOPS_PER_MARCH,
                    clock_res.clock.current_tick,
                );

                // 同步更新目标格 chunks：先 lock（用玩家色 + 标记行军中）
                // MVP 简化：先不变色，等到达再 occupy。
                // 后续加：目标格显示"行军中"半透明覆盖。

                // spawn 行军视觉（sprite 从 from 沿路径插值飞到 to）
                let from_world = slg_engine::camera::hex_world_position(from);
                let visual_entity = commands.spawn((
                    Sprite {
                        color: Color::srgba(1.0, 0.84, 0.0, 1.0), // 黄金 = 玩家
                        custom_size: Some(Vec2::new(
                            slg_engine::render::chunk_mesh::HEX_SIZE * 0.8,
                            slg_engine::render::chunk_mesh::HEX_SIZE * 0.8,
                        )),
                        ..default()
                    },
                    Transform::from_translation(Vec3::new(from_world.x, from_world.y, 2.0)),
                    MarchVisual { march_id: order.id },
                )).id();

                // 揭开行军路径上的所有 hex（探索 = 派兵 = 揭迷雾）
                reveal_coords_and_sync_chunks(&mut fog_res, &mut chunk_query, &order.path);

                info!(
                    "[Playing] 🪖 派兵: id={} from=({},{}) to=({},{}) arrive_tick={}, 揭开 {} 格",
                    order.id, from.q, from.r, coord.q, coord.r, order.arrive_tick, order.path.len()
                );

                // 暂时 unused warning 防止：visual_entity 留作后续
                let _ = visual_entity;
            }
            GamePhase::Editor => {
                info!("[Editor] 点击地块 ({}, {})", coord.q, coord.r);
                // TODO: 编辑器工具响应（笔刷/填充/选择等）
            }
            _ => {
                // 其他阶段忽略地图点击
            }
        }
    }
}

/// 处理地图地块右键点击事件
///
/// 右键用于：显示地块详情、部队指令菜单、上下文操作等。
fn handle_hex_right_click(
    mut commands: Commands,
    mut click_events: EventReader<HexRightClickEvent>,
    game_state: Res<GameState>,
    territory_res: Res<TerritoryManagerResource>,
) {
    for event in click_events.read() {
        let coord = event.coord;

        // 总是先 spawn 涟漪
        spawn_click_ring(&mut commands, event.world_pos);

        if coord.q < 0 || coord.r < 0 || coord.q >= 128 || coord.r >= 128 {
            info!(
                "[handle_hex_right_click] 忽略越界右键: ({}, {})",
                coord.q, coord.r
            );
            continue;
        }

        match game_state.phase {
            GamePhase::Playing => {
                let tile_key = coord.to_tile_key();
                let owner = territory_res.manager.owner_map.get(&tile_key);
                info!(
                    "[Playing] 右键地块 ({}, {}), 归属={:?}",
                    coord.q, coord.r, owner
                );
                // TODO: 显示右键上下文菜单（部队指令、地块详情等）
            }
            GamePhase::Editor => {
                info!("[Editor] 右键地块 ({}, {})", coord.q, coord.r);
                // TODO: 编辑器右键操作（吸管工具、属性查看等）
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// 输入链路诊断系统
// ---------------------------------------------------------------------------

/// 输入链路诊断：每帧打印 egui 指针状态 + 鼠标按键 + 坐标
///
/// 启用方式：设置环境变量 `HEX_PICK_DEBUG=1` 后运行。
/// 输出信息：
/// - egui 是否正在使用指针（is_using_pointer）
/// - 指针是否在 egui 区域上方（is_pointer_over_area）
/// - 鼠标左键/右键 pressed / just_pressed 状态
/// - egui 视口内的鼠标位置
///
/// 用途快速判断：
/// - 「事件被吞」：is_using_pointer=true 时左键 just_pressed=false
/// - 「坐标错误」：egui 坐标与 hex_pick 坐标不一致
#[allow(clippy::manual_is_multiple_of)] // 调试用系统：保留 % 60 让语义对开发者更直观
fn input_diagnostics(
    mut contexts: EguiContexts,
    mouse_button: Res<ButtonInput<MouseButton>>,
    pick_result: Res<slg_engine::camera::HexPickResult>,
    mut frame_counter: Local<u32>,
) {
    // 每 60 帧输出一次，避免日志刷屏
    *frame_counter += 1;
    if *frame_counter % 60 != 0 {
        return;
    }

    // 仅在调试模式下运行
    if std::env::var("HEX_PICK_DEBUG").as_deref() != Ok("1") {
        return;
    }

    let ctx = contexts.ctx_mut();

    // egui 指针状态
    let is_using_pointer = ctx.is_using_pointer();
    let pointer_over_area = ctx.is_pointer_over_area();

    // egui 视口内的鼠标位置
    let egui_pointer_pos = ctx.input(|i| i.pointer.latest_pos());
    let egui_pos_str = match egui_pointer_pos {
        Some(pos) => format!("({:.1}, {:.1})", pos.x, pos.y),
        None => "None".to_string(),
    };

    // Bevy 侧鼠标按键状态
    let left_pressed = mouse_button.pressed(MouseButton::Left);
    let left_just = mouse_button.just_pressed(MouseButton::Left);
    let right_pressed = mouse_button.pressed(MouseButton::Right);
    let right_just = mouse_button.just_pressed(MouseButton::Right);

    // hex_pick 结果
    let hex_str = match pick_result.coord {
        Some(c) => format!("({}, {})", c.q, c.r),
        None => "None".to_string(),
    };

    info!(
        "[DIAG] egui: using_ptr={}, over_area={}, pos={} | mouse: L={} L'={} R={} R'={} | hex={}",
        is_using_pointer, pointer_over_area, egui_pos_str,
        left_pressed, left_just, right_pressed, right_just,
        hex_str
    );
}

// ---------------------------------------------------------------------------
// Bevy 集成测试：headless 模式跑 handle_hex_click + process_tick_phases
//
// 用 MinimalPlugins（无 window / render / egui）创建 App，手动 init 资源，
// 注入 HexClickEvent，验证 MarchManager / 占地 / 揭雾 等真实系统行为。
//
// 这些测试才是用户问题的"真在 Bevy 跑"——之前的 slg-core 单元测试只验证
// 算法，集成测试验证 Bevy system 链路。
// ---------------------------------------------------------------------------

#[cfg(test)]
mod bevy_tests {
    use super::*;
    use bevy::app::App;
    use slg_core::entity::faction::{FactionPersonality, FactionResources, FactionState};
    use slg_core::map::tile::TerrainType;

    /// 创建带 MinimalPlugins + 必要 resources 的 App
    fn make_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        // Sprite / Mesh2d 等渲染 component 不在 MinimalPlugins 里；
        // 但 handle_hex_click 的 commands.spawn((Sprite, ...)) 是 deferred，
        // 不会在 system 执行时立刻注册 component，所以测试 1 帧内不会 panic。
        app.add_event::<HexClickEvent>();
        app.add_event::<HexRightClickEvent>();
        app.init_resource::<GameState>();
        app.init_resource::<FactionStoreResource>();
        app.init_resource::<TerritoryManagerResource>();
        app.init_resource::<FactionIdMap>();
        app.init_resource::<TerrainMapResource>();
        app.init_resource::<MarchManagerResource>();
        app.init_resource::<FogOfWarResource>();
        app.init_resource::<TileResourceMap>();
        app.init_resource::<slg_engine::systems::GameClockResource>();
        app.init_resource::<slg_engine::systems::CommandQueueResource>();
        app.init_resource::<slg_ui::panels::game_over::GameOverState>();
        app.init_resource::<slg_engine::camera::HexPickResult>();
        app
    }

    /// 手动 init 一个最小可玩状态：
    /// - game phase = Playing
    /// - 玩家 = faction_1
    /// - 玩家主城 = (68, 65)
    /// - 整张 128x128 = plains
    fn init_playing_state(world: &mut World) {
        // GameState
        let mut gs = world.resource_mut::<GameState>();
        gs.phase = GamePhase::Playing;
        gs.player_faction_id = "faction_1".to_string();
        gs.tick = 0;

        // TerritoryManager: register 128x128 + 玩家主城 + occupy
        let main_city = HexCoord::new(68, 65);
        {
            let mut tm = world.resource_mut::<TerritoryManagerResource>();
            for r in 0..128i32 {
                for q in 0..128i32 {
                    tm.manager.register_tile(HexCoord::new(q, r));
                }
            }
            tm.manager.set_main_city(&"faction_1".to_string(), main_city);
            tm.manager.occupy(main_city, &"faction_1".to_string());
        }

        // TerrainMap: 全部 plains（保证 can_occupy 通过）
        {
            let mut terrain = world.resource_mut::<TerrainMapResource>();
            for q in 0..128i32 {
                for r in 0..128i32 {
                    terrain
                        .map
                        .insert(((r as u64) << 32) | (q as u64), TerrainType::Plains);
                }
            }
        }

        // FactionStore: faction_1
        {
            let mut fs = world.resource_mut::<FactionStoreResource>();
            fs.store.factions.insert(
                "faction_1".to_string(),
                FactionState {
                    resources: FactionResources::default(),
                    personality: FactionPersonality {
                        aggression: 0.5,
                        expansion: 0.5,
                        diplomacy: 0.5,
                        caution: 0.5,
                    },
                    main_city: Some(main_city),
                    diplomacy: Default::default(),
                },
            );
        }

        // FactionIdMap
        {
            let mut fim = world.resource_mut::<FactionIdMap>();
            fim.map.insert("faction_1".to_string(), 6);
        }

        // FogOfWar: 全黑 + 玩家主城周围 7 格揭开
        {
            let mut fog = world.resource_mut::<FogOfWarResource>();
            let cities = vec![(main_city, "faction_1".to_string())];
            fog.fog = slg_core::fog::FogOfWar::init_with_cities(
                128,
                128,
                &cities,
                &"faction_1".to_string(),
            );
        }
    }

    /// 测试 1：handle_hex_click 在 HexClickEvent 注入后 dispatch march
    #[test]
    fn bevy_handle_hex_click_dispatches_march() {
        let mut app = make_app();
        init_playing_state(app.world_mut());
        app.add_systems(Update, handle_hex_click);

        let target = HexCoord::new(69, 65); // 玩家主城 (68,65) 的东邻
        app.world_mut().send_event(HexClickEvent {
            coord: target,
            world_pos: Vec2::new(0.0, 0.0),
        });

        app.update();

        // 验证：MarchManager 收到 1 个 order
        let march = app.world().resource::<MarchManagerResource>();
        assert_eq!(
            march.manager.orders.len(),
            1,
            "派兵后 MarchManager 应有 1 个 order, got {}",
            march.manager.orders.len()
        );
        let order = march.manager.orders.values().next().unwrap();
        assert_eq!(order.from, HexCoord::new(68, 65), "from 应是玩家主城");
        assert_eq!(order.to, target, "to 应是点击的 target");
        assert_eq!(
            order.troops, slg_core::military::TROOPS_PER_MARCH,
            "兵数应是固定 TROOPS_PER_MARCH"
        );
        eprintln!(
            "TEST1 ✅: 派兵 1 队 from=({},{}) to=({},{}) arrive_tick={}",
            order.from.q, order.from.r, order.to.q, order.to.r, order.arrive_tick
        );
    }

    /// 测试 2：派兵后揭开路径
    #[test]
    fn bevy_dispatch_reveals_fog_path() {
        let mut app = make_app();
        init_playing_state(app.world_mut());
        app.add_systems(Update, handle_hex_click);

        // 派兵到 1 hex 邻接（handle_hex_click 用 can_occupy，只能派 1 hex 邻接）
        let target = HexCoord::new(69, 65); // 东邻 1 hex (玩家主城 (68,65))
        app.world_mut().send_event(HexClickEvent {
            coord: target,
            world_pos: Vec2::new(0.0, 0.0),
        });
        app.update();

        // 验证：path = [from, to] 2 格都揭开
        let march = app.world().resource::<MarchManagerResource>();
        assert_eq!(march.manager.orders.len(), 1, "派兵应成功");
        let order = march.manager.orders.values().next().unwrap();
        eprintln!(
            "TEST2 DEBUG: order.from=({},{}), to=({},{}), path={:?}",
            order.from.q, order.from.r, order.to.q, order.to.r, order.path
        );

        let fog = app.world().resource::<FogOfWarResource>();
        for c in &order.path {
            assert_eq!(
                fog.fog.get(c.q, c.r),
                slg_core::fog::FOG_VISIBLE,
                "path 节点 ({}, {}) 应该揭开, actual fog = {}",
                c.q,
                c.r,
                fog.fog.get(c.q, c.r)
            );
        }
        eprintln!(
            "TEST2 ✅: 派兵 1 hex 邻接 from=({},{}) to=({},{}), {} 格全揭开",
            order.from.q,
            order.from.r,
            order.to.q,
            order.to.r,
            order.path.len()
        );
    }

    /// 测试 3：目标格被锁定后不能双派
    #[test]
    fn bevy_double_dispatch_blocked() {
        let mut app = make_app();
        init_playing_state(app.world_mut());
        app.add_systems(Update, handle_hex_click);

        let target = HexCoord::new(69, 65);
        // 第一次派兵
        app.world_mut()
            .send_event(HexClickEvent { coord: target, world_pos: Vec2::new(0.0, 0.0) });
        app.update();
        // 第二次派兵到同格
        app.world_mut()
            .send_event(HexClickEvent { coord: target, world_pos: Vec2::new(0.0, 0.0) });
        app.update();

        let march = app.world().resource::<MarchManagerResource>();
        assert_eq!(
            march.manager.orders.len(),
            1,
            "目标锁住时第二次派兵应被拒绝, got {} orders",
            march.manager.orders.len()
        );
        eprintln!("TEST3 ✅: 双派被目标锁定阻止");
    }

    /// 测试 4：完整端到端：派兵 → 推进 tick → 到达 → 占地
    ///
    /// 直接调 process_tick_phases 多次（每次 +1 tick），覆盖 MarchAdvance 阶段
    #[test]
    fn bevy_full_march_arrive_occupy() {
        let mut app = make_app();
        init_playing_state(app.world_mut());

        // 手动设置 tick = 100，clock 进入第 100 tick
        {
            let mut clock = app.world_mut().resource_mut::<slg_engine::systems::GameClockResource>();
            clock.clock.current_tick = 100;
        }

        // 注册 dispatch + 推进系统
        app.add_systems(Update, (handle_hex_click, process_tick_phases).chain());

        // 派兵
        let target = HexCoord::new(69, 65);
        app.world_mut().send_event(HexClickEvent {
            coord: target,
            world_pos: Vec2::new(0.0, 0.0),
        });
        app.update();

        // 推进 5 tick（行军 5 tick 到 1 hex）
        for _ in 0..5 {
            // 直接 advance clock + 跑 process_tick_phases
            {
                let mut clock = app
                    .world_mut()
                    .resource_mut::<slg_engine::systems::GameClockResource>();
                clock.clock.current_tick += 1;
            }
            app.update();
        }

        // 验证：target 已被玩家占地
        let tm = app.world().resource::<TerritoryManagerResource>();
        let target_key = target.to_tile_key();
        assert_eq!(
            tm.manager.owner_map.get(&target_key),
            Some(&"faction_1".to_string()),
            "到达后 target 应归玩家"
        );
        eprintln!("TEST4 ✅: 派兵 → 5 tick → 占地成功");
    }

    /// 测试 5：圈地后每 tick 资源增长
    ///
    /// 验证 process_tick_phases 的 ResourceProduction 阶段：
    /// 圈地 plains -> 每 tick +5 food
    #[test]
    fn bevy_resource_production_per_tick() {
        let mut app = make_app();
        init_playing_state(app.world_mut());

        // 玩家主城 (68, 65) 是 Plains -> 每 tick 产 5 food
        let initial_food = app
            .world()
            .resource::<FactionStoreResource>()
            .store
            .factions
            .get("faction_1")
            .unwrap()
            .resources
            .food;
        eprintln!("TEST5 DEBUG: 初始 food = {}", initial_food);

        // 跑 1 tick
        app.add_systems(Update, process_tick_phases);
        {
            let mut clock =
                app.world_mut()
                    .resource_mut::<slg_engine::systems::GameClockResource>();
            clock.clock.current_tick = 1;
        }
        app.update();

        // 验证：food 增长 5
        let after_food = app
            .world()
            .resource::<FactionStoreResource>()
            .store
            .factions
            .get("faction_1")
            .unwrap()
            .resources
            .food;
        eprintln!("TEST5 DEBUG: 1 tick 后 food = {} (期望 initial+5)", initial_food);
        assert_eq!(
            after_food,
            initial_food + 5,
            "圈地 1 格 Plains, 1 tick 后 food 应 +5"
        );

        // 再跑 9 tick，验证累计
        for _ in 0..9 {
            {
                let mut clock =
                    app.world_mut()
                        .resource_mut::<slg_engine::systems::GameClockResource>();
                clock.clock.current_tick += 1;
            }
            app.update();
        }
        let after_10 = app
            .world()
            .resource::<FactionStoreResource>()
            .store
            .factions
            .get("faction_1")
            .unwrap()
            .resources
            .food;
        assert_eq!(
            after_10,
            initial_food + 50,
            "10 tick 后 food 应 = initial + 5*10 = initial+50, got initial+{}",
            after_10 - initial_food
        );
        eprintln!("TEST5 ✅: 10 tick 圈地 1 Plains = food +50");
    }

    /// 测试 6：派兵后新圈地的格也开始产资源
    ///
    /// 验证：玩家初始占 1 格 (Plains, food+5/tick)，
    /// 派兵 1 hex 邻接（假设 Plains）落地后变成 2 格，应该 food +10/tick
    #[test]
    fn bevy_new_territory_adds_production() {
        let mut app = make_app();
        init_playing_state(app.world_mut());

        // clock tick 0
        {
            let mut clock =
                app.world_mut()
                    .resource_mut::<slg_engine::systems::GameClockResource>();
            clock.clock.current_tick = 0;
        }

        app.add_systems(Update, (handle_hex_click, process_tick_phases).chain());

        // 派兵 1 hex
        let target = HexCoord::new(69, 65);
        app.world_mut().send_event(HexClickEvent {
            coord: target,
            world_pos: Vec2::new(0.0, 0.0),
        });
        app.update();

        // 推进 5 tick 让兵到达
        for _ in 0..5 {
            {
                let mut clock =
                    app.world_mut()
                        .resource_mut::<slg_engine::systems::GameClockResource>();
                clock.clock.current_tick += 1;
            }
            app.update();
        }

        // 占地 2 格都应该是 Plains -> food +10/tick
        // 记录占地后的 food
        let food_after_arrival = app
            .world()
            .resource::<FactionStoreResource>()
            .store
            .factions
            .get("faction_1")
            .unwrap()
            .resources
            .food;
        eprintln!("TEST6 DEBUG: 占地完成时 food = {}", food_after_arrival);

        // 再跑 1 tick
        {
            let mut clock =
                app.world_mut()
                    .resource_mut::<slg_engine::systems::GameClockResource>();
            clock.clock.current_tick += 1;
        }
        app.update();

        let food_next_tick = app
            .world()
            .resource::<FactionStoreResource>()
            .store
            .factions
            .get("faction_1")
            .unwrap()
            .resources
            .food;
        let delta = food_next_tick - food_after_arrival;
        eprintln!("TEST6 DEBUG: 占地后再 1 tick food +{}", delta);
        assert_eq!(
            delta, 10,
            "2 格 Plains 1 tick 应产 10 food (5+5), got {}",
            delta
        );
        eprintln!("TEST6 ✅: 派兵落地后圈地扩大, 资源产出翻倍");
    }

    /// 测试 7：玩家主城被 NPC 推 → check_victory_system 触发 Defeat
    #[test]
    fn bevy_check_defeat_triggers_gameover() {
        let mut app = make_app();
        init_playing_state(app.world_mut());

        // 把玩家主城 (68, 65) 的 owner 改成 faction_2
        {
            let mut tm = app.world_mut().resource_mut::<TerritoryManagerResource>();
            tm.manager
                .owner_map
                .insert(HexCoord::new(68, 65).to_tile_key(), "faction_2".to_string());
        }

        app.add_systems(Update, check_victory_system);
        app.update();

        // 验证：game_state.phase = GameOver
        let gs = app.world().resource::<GameState>();
        assert_eq!(
            gs.phase,
            GamePhase::GameOver,
            "主城被推应触发 GameOver"
        );
        let gos = app.world().resource::<slg_ui::panels::game_over::GameOverState>();
        assert!(gos.show, "GameOverState.show 应为 true");
        assert!(!gos.is_victory, "主城被推是 Defeat");
        assert!(gos.reason.contains("主城"), "reason 应提到主城: {}", gos.reason);
        eprintln!("TEST7 ✅: 主城被推触发 Defeat GameOver");
    }

    /// 测试 8：玩家占地 ≥ 50% → check_victory_system 触发 Victory
    #[test]
    fn bevy_check_victory_triggers_gameover() {
        let mut app = make_app();
        init_playing_state(app.world_mut());

        // 让玩家占所有 non-water 格：先 clear owner_map，再 occupy 10000 个 plains
        // 简单做法：直接 fake：把 threshold 调到极低 (0.01) 让 1 格就胜利
        // 但 check_victory_system 用 DEFAULT_VICTORY_RATIO (0.5)，改不了
        // 改用：让玩家占多数
        // terrain_map 有 128*128 = 16384 格，玩家主城 1 格 -> 0.006% < 50%
        // 改 terrain_map 只剩 2 格，玩家占 1 格 = 50% 胜利
        {
            let mut tm = app.world_mut().resource_mut::<TerrainMapResource>();
            tm.map.clear();
            tm.map.insert(HexCoord::new(0, 0).to_tile_key(), TerrainType::Plains);
            tm.map.insert(HexCoord::new(1, 0).to_tile_key(), TerrainType::Plains);
        }
        // 玩家占 2 格 (主城 + 邻接)
        {
            let mut ttm = app.world_mut().resource_mut::<TerritoryManagerResource>();
            ttm.manager
                .owner_map
                .insert(HexCoord::new(0, 0).to_tile_key(), "faction_1".to_string());
            ttm.manager
                .owner_map
                .insert(HexCoord::new(1, 0).to_tile_key(), "faction_1".to_string());
        }

        app.add_systems(Update, check_victory_system);
        app.update();

        let gs = app.world().resource::<GameState>();
        assert_eq!(
            gs.phase,
            GamePhase::GameOver,
            "玩家 100% 占地应触发 GameOver"
        );
        let gos = app.world().resource::<slg_ui::panels::game_over::GameOverState>();
        assert!(gos.show);
        assert!(gos.is_victory, "100% 占地是 Victory");
        assert!(gos.reason.contains("统一天下"), "reason: {}", gos.reason);
        eprintln!("TEST8 ✅: 100% 占地触发 Victory GameOver");
    }

    /// 测试 9：没触发条件时 game phase 保持 Playing
    #[test]
    fn bevy_no_victory_no_defeat_phase_unchanged() {
        let mut app = make_app();
        init_playing_state(app.world_mut());

        // 玩家只占主城 1 格，terrain 128x128 -> 0.006% < 50%
        // 主城没被推
        app.add_systems(Update, check_victory_system);
        app.update();

        let gs = app.world().resource::<GameState>();
        assert_eq!(gs.phase, GamePhase::Playing, "应保持 Playing");
        let gos = app.world().resource::<slg_ui::panels::game_over::GameOverState>();
        assert!(!gos.show, "GameOverState.show 应为 false");
        eprintln!("TEST9 ✅: 无条件触发时 phase 不变");
    }

    /// 测试 10：hex_click 在 mock MouseButtonInput + HexPickResult 下发 HexClickEvent
    ///
    /// 模拟 input layer: 鼠标左键 + 拾取结果 → 发 HexClickEvent → handle_hex_click 派兵
    /// 这是 input layer 的端到端（除 egui 拦截外）
    #[test]
    fn bevy_input_layer_click_dispatches_march() {
        use slg_engine::camera::{HexClickEvent, HexPickResult};

        let mut app = make_app();
        init_playing_state(app.world_mut());

        // 注册 hex_click + handle_hex_click
        // 注意: hex_click 需要 EguiContexts, MinimalPlugins 没有, 这条链测不到
        // 我们直接发 HexClickEvent 模拟 hex_click 的输出
        app.add_systems(Update, handle_hex_click);

        // 模拟 hex_pick 输出: pick_result.coord = 玩家主城邻接
        let target = HexCoord::new(69, 65);
        {
            let mut pr = app.world_mut().resource_mut::<HexPickResult>();
            pr.coord = Some(target);
            pr.world_pos = Some(Vec2::new(0.0, 0.0));
        }

        // 发 MouseButtonInput: left pressed
        // 这一步是 hex_click 系统的输入; 但我们跳过 hex_click 直接发 HexClickEvent
        app.world_mut().send_event(HexClickEvent {
            coord: target,
            world_pos: Vec2::new(0.0, 0.0),
        });
        app.update();

        // 验证: MarchManager 收到派兵
        let march = app.world().resource::<MarchManagerResource>();
        assert_eq!(march.manager.orders.len(), 1, "input layer 派兵应成功");
        eprintln!("TEST10 ✅: HexClickEvent 注入 -> 派兵成功");
    }

    /// 测试 11：hex_click 的 egui 拦截分支
    ///
    /// 验证：当 egui is_using_pointer = true 时, hex_click 不发 HexClickEvent
    /// 这需要 mock egui 状态; 用一个简化的方法: 直接验证 handle_hex_click 的
    /// 守卫逻辑（用 HexClickEvent 多次发送, 验证只有符合条件的才派兵）
    #[test]
    fn bevy_input_layer_oob_click_ignored() {
        use slg_engine::camera::{HexClickEvent, HexPickResult};

        let mut app = make_app();
        init_playing_state(app.world_mut());

        app.add_systems(Update, handle_hex_click);

        // 越界点击: q = 200, r = 200
        app.world_mut().resource_mut::<HexPickResult>().coord = Some(HexCoord::new(200, 200));
        app.world_mut().send_event(HexClickEvent {
            coord: HexCoord::new(200, 200),
            world_pos: Vec2::new(0.0, 0.0),
        });
        app.update();

        // 验证: 越界点击不派兵
        let march = app.world().resource::<MarchManagerResource>();
        assert_eq!(march.manager.orders.len(), 0, "越界点击应被忽略");
        eprintln!("TEST11 ✅: 越界点击不派兵");
    }

    /// 测试 12：hex_click 拒绝已 own 的格（不能攻占自己的）
    #[test]
    fn bevy_input_layer_cannot_occupy_own() {
        use slg_engine::camera::{HexClickEvent, HexPickResult};

        let mut app = make_app();
        init_playing_state(app.world_mut());

        app.add_systems(Update, handle_hex_click);

        // 点玩家自己的主城 (68, 65) - 已 own
        app.world_mut().resource_mut::<HexPickResult>().coord = Some(HexCoord::new(68, 65));
        app.world_mut().send_event(HexClickEvent {
            coord: HexCoord::new(68, 65),
            world_pos: Vec2::new(0.0, 0.0),
        });
        app.update();

        // 验证: 不能攻占自己的格
        let march = app.world().resource::<MarchManagerResource>();
        assert_eq!(march.manager.orders.len(), 0, "已 own 的格不能再次攻占");
        eprintln!("TEST12 ✅: 已 own 的格不能再次攻占");
    }
}


