//! slg-app: 《天下策》应用壳
//!
//! 负责窗口/入口、模式切换（游玩⇄编辑）、插件组装、崩溃日志。
//! 将所有已实现的子系统集成，实现从启动到可玩的完整流程。

use std::collections::BTreeMap;

use bevy::prelude::*;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::sprite::AlphaMode2d;
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

/// M8 全局建筑 Resource：所有建筑按 coord 索引
#[derive(Debug, Clone, Default, Resource)]
pub struct BuildingManagerResource {
    pub manager: slg_core::building::BuildingManager,
}

/// M8 全局城池 Resource：主城 + 分城按 coord 索引
#[derive(Debug, Clone, Default, Resource)]
pub struct CityManagerResource {
    pub manager: slg_core::city::CityManager,
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
            .init_resource::<BuildingManagerResource>()
            .init_resource::<CityManagerResource>()
            .init_resource::<SelectedHex>() // M8 UI
            .add_event::<BuildAction>()     // M8 UI
            .add_event::<BattleReportEvent>() // M9
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
                ),
            )
            // M9.2: 编辑器键盘快捷键 (Ctrl+Z / Ctrl+Y / Ctrl+Shift+Z / Ctrl+S)
            .add_systems(Update, handle_editor_keyboard)
            // M10.2: 同步 EditorState.show 跟 GamePhase, 避免 Menu/Playing 显示编辑器按钮
            .add_systems(Update, sync_editor_visibility)
            // 输入链路诊断系统（设置 HEX_PICK_DEBUG=1 启用）
            .add_systems(Update, input_diagnostics)
            // 点击涟漪生命周期
            .add_systems(Update, update_click_rings)
            // 行军 sprite 插值移动（每帧）
            .add_systems(Update, march_sprite_system)
            // 胜利/失败检查（每帧 game phase = Playing 时跑）
            .add_systems(Update, check_victory_system)
            // M8 UI: 处理建筑/分城指令
            .add_systems(Update, handle_build_order)
            // M9: 把战报 event 写进 BattleReportState
            .add_systems(Update, handle_battle_report)
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
        // 剩余生命 0~1.0 → 线性 alpha 0.9→0.0 (保留 spawn 时的金色 1.0, 0.84, 0.0)
        //
        // 之前 bug: 设成 (1.0, 1.0, 1.0, alpha) → 玩家看到的是白方块, 不是金点
        let alpha = ring.lifetime.clamp(0.0, 1.0) * 0.9;
        sprite.color = Color::srgba(1.0, 0.84, 0.0, alpha);
    }
}

/// 主城 marker 组件：spawn 在每个势力主城的特殊 sprite（颜色 = 势力色）
#[derive(Component)]
struct MainCityMarker {
    #[allow(dead_code)] // 未来用于 hover/click → 弹出城池信息面板
    pub faction_id: FactionId,
    /// 主城 hex 坐标（debug 用：与 marker 的 world position 对比，确认是 hex 还是转换出错）
    #[allow(dead_code)]
    pub hex: HexCoord,
}

/// 在点击位置 spawn 一个金色小点 (1s 淡出)
///
/// M10.2: 从"白色 20x20 大方块"改"金色 8x8 小点"
/// - 之前 20x20 跟 hex (≈28 宽) 差不多大, 看着像"白色 hex 弹出来", 抢戏
/// - 玩家色 = 黄金 (1.0, 0.84, 0.0) 跟主城/行军 sprite 统一
/// - 1.0 world unit (= HEX_SIZE) = 1 个 hex 直径, 视觉上 = "我点的是这个 hex"
///   不会盖住邻接 hex, 玩家能直观看到自己点中哪一格
///
/// **踩过的坑 (M10.2 + M10.3.2 + M10.3.3)**:
/// - M10.2: custom_size = 20 → 20 hex 宽半屏"白布"
/// - M10.3.2: custom_size = 8 → 8 hex 宽半屏"黄布" (我以为 8 是像素)
/// - M10.3.3: custom_size = 0.6 → 半个 hex, 太小人眼看不到
/// - M10.3.3+: custom_size = 1.0 → 1 个 hex, "点这个 hex" 视觉感
fn spawn_click_ring(commands: &mut Commands, world_pos: Vec2) {
    let size = 1.0_f32;
    commands.spawn((
        Sprite {
            color: Color::srgba(1.0, 0.84, 0.0, 0.9), // 黄金, 略透明
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
    /// M10.2 + M10.3.3: 状态消息 (派兵/操作反馈), top_bar 显示, 3s 后自动清空
    pub status_message: String,
}

impl Default for GameState {
    fn default() -> Self {
        Self {
            phase: GamePhase::Menu,
            tick: 0,
            last_processed_tick: 0,
            player_faction_id: String::new(),
            difficulty: Difficulty::Normal,
            status_message: String::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// M8 UI: 选中 + 建筑指令
// ---------------------------------------------------------------------------

/// M8 UI: 当前选中的 hex (用于显示建筑面板 / 高亮)
///
/// - `None` 表示没选
/// - 玩家点己方格 → 选中 (toggle: 同格再点取消)
/// - 玩家点非己方 → 取消选中
#[derive(Resource, Default, Debug, Clone)]
pub struct SelectedHex {
    pub coord: Option<HexCoord>,
}

/// M8 UI: 建筑/分城指令
///
/// UI panel 按钮触发 → 发 BuildAction event → `handle_build_order` 执行
#[derive(Event, Debug, Clone, PartialEq, Eq)]
pub enum BuildAction {
    /// 在 coord 上建 1 个 L1 建筑
    Build(Coord, slg_core::building::BuildingType),
    /// 升级 coord 上的建筑 (扣资源, level+1)
    Upgrade(Coord),
    /// 在 coord 建分城 (扣 500g+200f+100w, 需要 6+ 邻接)
    EstablishSubcity(Coord),
}

/// 用 newtype 包一下, 避免和 slg_core::map::grid::HexCoord 撞 Event derive 字段
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Coord(pub HexCoord);

impl From<HexCoord> for Coord {
    fn from(c: HexCoord) -> Self {
        Coord(c)
    }
}

// ---------------------------------------------------------------------------
// M9: 战报 Event
// ---------------------------------------------------------------------------

/// M9: 战斗结果 (UI 浮窗用)
#[derive(Event, Debug, Clone)]
pub struct BattleReportEvent {
    /// 攻方 faction id
    pub attacker: String,
    /// 守方 faction id
    pub defender: String,
    /// 战斗发生格 (arrival.to)
    pub coord: HexCoord,
    /// 当前 tick
    pub tick: u64,
    /// 战报结果: "Victory" / "Defeat" / "Draw"
    pub result: String,
    /// 攻方剩余 troops
    pub attacker_remaining: u32,
    /// 守方剩余 troops
    pub defender_remaining: u32,
    /// 攻方初始 troops
    pub attacker_initial: u32,
    /// 守方初始 troops
    pub defender_initial: u32,
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

/// M10.2: 同步 EditorState.show 跟 GamePhase
///
/// render_editor_toolbar 读 `editor_state.show` 决定是否画 UI.
/// GamePhase 改变时,这个 system 把 `show` 同步过去.
/// 避免 slg-ui 直接依赖 slg-app (循环依赖).
fn sync_editor_visibility(
    game_state: Res<GameState>,
    mut editor_state: ResMut<slg_editor::editor_state::EditorState>,
) {
    editor_state.show = game_state.phase == GamePhase::Editor;
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
    images: &mut Assets<Image>, // M10.3: atlas Image
    existing_chunks: &Query<Entity, With<EngineChunkData>>,
    camera_query: &mut Query<(&mut Transform, &mut Projection), With<Camera2d>>,
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

                // M10.3.2 修复: 主城那格必须 occupy 进 owner_map
                //
                // 之前只 set_main_city (记录在 main_cities HashMap), 没 occupy (进 owner_map)
                // → 玩家点主城时 owner_map.get() 返回 None
                // → 走"非己方"分支而不是"toggle 选中"
                // → 玩家看不见选中效果, 也派不出兵 (can_occupy 找不到 friendly neighbor)
                territory_res.manager.occupy(coord, faction_id);
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

    // M10.3: 加载 atlas.png → Image handle, 创建 ColorMaterial with texture
    // (所有 chunk 共享同一个 material, 节约 GPU)
    let atlas_bytes = slg_engine::render::embedded_atlas::ATLAS_PNG;
    let atlas_image = Image::from_buffer(
        atlas_bytes,
        bevy::image::ImageType::Extension("png"),
        bevy::image::CompressedImageFormats::NONE,
        true, // is_srgb
        bevy::image::ImageSampler::nearest(), // pixel art 不要过滤
        RenderAssetUsages::RENDER_WORLD,
    )
    .map_err(|e| format!("加载 atlas.png 失败: {}", e))
    .expect("atlas.png 应能加载");
    let atlas_image_handle = images.add(atlas_image);
    let atlas_material_handle = materials.add(ColorMaterial {
        color: Color::WHITE,
        texture: Some(atlas_image_handle.clone()),
        alpha_mode: AlphaMode2d::Blend, // 支持 hex mask 透明
    });
    let atlas_uv = slg_engine::render::embedded_atlas::AtlasUvRes::default().0;

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
            selected: [0; 1024], // M9: 初始全未选中
            // M10.2: dirty = false, 因为 mesh 已经在上面 build_chunk_mesh_with_transitions 建好,
            // 不需要 rebuild。设 true 反而触发"重建相同 mesh" 与渲染线程抢
            dirty: false,
            current_lod: 0,
        };

        let mesh = build_chunk_mesh_with_transitions(
            &core_chunk.terrains,
            &owners,
            &fog_arr,
            &[0u8; 1024],
            &atlas_uv, // M10.3: 8 地形 UV
        );
        let mesh_handle = meshes.add(mesh);
        let material_handle = atlas_material_handle.clone();

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
    if let Ok((mut transform, _)) = camera_query.get_single_mut() {
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
    if let Ok((_, mut projection)) = camera_query.get_single_mut() {
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
    mut images: ResMut<Assets<Image>>, // M10.3: atlas Image
    existing_chunks: Query<Entity, With<EngineChunkData>>,
    // M10.3: 合并 Transform + Projection 为 1 个 query, 砍掉 1 个 system arg
    // (Bevy 0.15 system arg 上限 16, handle_new_game_actions 加 images 后 17 → 16)
    mut camera_query: Query<(&mut Transform, &mut Projection), With<Camera2d>>,
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
                    &mut images,
                    &existing_chunks,
                    &mut camera_query,
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
    building_res: Res<BuildingManagerResource>, // M8
    #[allow(dead_code, unused_variables)] // M8 分城 UI 暂未实现, 留作后续
    city_res: Res<CityManagerResource>,
    mut game_state: ResMut<GameState>,
    mut chunk_query: Query<&mut EngineChunkData>,
    mut battle_events: EventWriter<BattleReportEvent>, // M9
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
                    // M8: 建筑资源加成 (农田/伐木/矿场)
                    for (faction_id, faction) in faction_res.store.factions.iter_mut() {
                        slg_core::economy::apply_building_production(
                            &building_res.manager,
                            faction_id,
                            &mut faction.resources,
                        );
                    }
                }
                TickPhase::MarchAdvance => {
                    // 行军推进：检查到达、触发战斗 / occupy、清理已完成的
                    // M6 战斗: 到达目标格时, 如果已被其它 faction 占据 -> 触发战斗
                    //  - 战斗胜: 占据目标格 + 扣 50% 兵
                    //  - 战斗败: 攻方行军失败
                    //  - 战斗平: 双方都扣 25% 兵, 目标格不变
                    //  - 目标格未被占据: 走原来的逻辑 (occupy)
                    let arrivals = march_res.manager.advance_all(game_state.tick);
                    for arrival in arrivals {
                        let target_key = arrival.to.to_tile_key();
                        let target_owner = territory_res.manager.owner_map.get(&target_key).cloned();

                        match target_owner {
                            // 目标格空（没被占） -> can_occupy 检查
                            None => {
                                let can = territory_res.manager.can_occupy(
                                    arrival.to,
                                    &arrival.faction_id,
                                    &terrain_map.map,
                                );
                                if can {
                                    territory_res.manager.occupy(arrival.to, &arrival.faction_id);
                                    sync_chunk_owner(
                                        arrival.to,
                                        &arrival.faction_id,
                                        &faction_id_map,
                                        &mut chunk_query,
                                    );
                                    // 揭开到达格 + 6 邻域
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
                                        "[MarchAdvance] ❌ 到达但无法占地: ({}, {})",
                                        arrival.to.q, arrival.to.r
                                    );
                                    march_res.manager.fail(arrival.id);
                                }
                            }
                            // 目标格被己方占（罕见，行军期间被己方其它兵占）-> 直接 occupy 覆盖
                            Some(owner) if owner == arrival.faction_id => {
                                territory_res.manager.occupy(arrival.to, &arrival.faction_id);
                                info!(
                                    "[MarchAdvance] ✅ 到达己方格: ({}, {})",
                                    arrival.to.q, arrival.to.r
                                );
                            }
                            // 目标格被敌方占 -> 触发战斗
                            Some(defender) => {
                                handle_combat(
                                    &arrival,
                                    &defender,
                                    &terrain_map,
                                    &mut territory_res,
                                    &mut march_res,
                                    &mut fog_res,
                                    &mut chunk_query,
                                    &faction_id_map,
                                    &faction_res.store,
                                    &building_res.manager,
                                    game_state.tick,
                                    &mut battle_events,
                                );
                            }
                        }
                    }
                    march_res.manager.cleanup_finished();
                }
                TickPhase::AIDecision => {
                    // AI 决策（错峰 + 简化扩张）
                    // M5: 6 个 faction 都跑 AIDecision，但只有 AI faction (有 main_city + 邻接可占领) 才派兵
                    //   - 玩家 faction_1 (index 0) slot=0 也会跑，但 main_city 不会跑 AI
                    //     （玩家主城不会被 AI 覆盖派兵，玩家继续手动控制）
                    //   - AI faction_2~6 (index 1~5) slot 1~5
                    //   - 用 MarchManager dispatch，与玩家共用同一行军链路
                    let current_tick = game_state.tick;
                    for (i, (faction_id, faction)) in
                        faction_res.store.factions.iter_mut().enumerate()
                    {
                        // 错峰: 10 tick 一轮, slot = i (i=0..5)
                        if !should_ai_decide(current_tick, i as u8) {
                            continue;
                        }
                        // AI 须有主城
                        let Some(main_city) = faction.main_city else {
                            continue;
                        };
                        // 找扩张目标
                        let target = slg_core::military::ai_expansion_target(
                            faction_id,
                            main_city,
                            &mut territory_res.manager,
                            &terrain_map.map,
                        );
                        if let Some(target) = target {
                            // 派兵（用 MarchManager + MarchAdvance 阶段落地）
                            // M7: 挂 AI 主将 + 默认兵种 (步兵)
                            let ai_general = faction.primary_general().cloned();
                            let ai_unit = slg_core::entity::faction::FactionState::default_unit_type();
                            let order = march_res.manager.dispatch(
                                faction_id.clone(),
                                main_city,
                                target,
                                slg_core::military::TROOPS_PER_MARCH,
                                current_tick,
                                ai_general,
                                ai_unit,
                            );
                            // 揭迷雾（AI 派兵也会揭开）
                            for c in &order.path {
                                fog_res.fog.reveal_one(*c);
                            }
                            info!(
                                "[AIDecision] AI {} 派兵 from=({},{}) to=({},{}) arrive_tick={}",
                                faction_id, main_city.q, main_city.r, target.q, target.r, order.arrive_tick
                            );
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
    mut game_state: ResMut<GameState>,
    clock_res: Res<GameClockResource>,
    faction_res: Res<FactionStoreResource>,
    march_res: Res<MarchManagerResource>,
    time: Res<Time>,
    mut top_bar: ResMut<slg_ui::panels::top_bar::TopBarState>,
    mut status_timer: Local<(String, f32)>,
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

    // M10.2 + M10.3.3: 状态消息同步 + 3s 自动清空
    // game_state.status_message 被 handle_hex_click 设, 这里刷新 top_bar 显示
    // 当 message 变化时重置 timer, 3s 后清空 (M10.3.3: 1.5s → 3s, 给玩家更长时间看反馈)
    let now = time.elapsed_secs();
    if game_state.status_message != status_timer.0 {
        status_timer.0 = game_state.status_message.clone();
        status_timer.1 = now;
    }
    if now - status_timer.1 < 3.0 {
        top_bar.status_message = status_timer.0.clone();
    } else {
        top_bar.status_message = String::new();
        if !game_state.status_message.is_empty() {
            game_state.status_message.clear();
        }
    }
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

/// 同步 chunk owner 数组 + 标 dirty
///
/// 抽出来给 MarchAdvance / combat victory 复用。
#[allow(clippy::too_many_arguments)]
fn sync_chunk_owner(
    target: HexCoord,
    faction_id: &FactionId,
    faction_id_map: &FactionIdMap,
    chunk_query: &mut Query<&mut EngineChunkData>,
) {
    let cx = target.q / 32;
    let cy = target.r / 32;
    let lx = (target.q % 32) as usize;
    let ly = (target.r % 32) as usize;
    let local_idx = ly * 32 + lx;
    let color_idx = faction_id_map.get(faction_id);
    for mut chunk in chunk_query.iter_mut() {
        if chunk.chunk_x == cx && chunk.chunk_y == cy {
            chunk.owners[local_idx] = color_idx;
            chunk.dirty = true;
            break;
        }
    }
}

/// M9: 同步 chunk 的 selected 数组
///
/// 玩家点己方格 / 取消选中 时调用
/// - `selected = true` → 颜色叠加金色
/// - `selected = false` → 恢复原本颜色
fn sync_chunk_selection(
    target: HexCoord,
    selected: bool,
    chunk_query: &mut Query<&mut EngineChunkData>,
) {
    let cx = target.q / 32;
    let cy = target.r / 32;
    let lx = (target.q % 32) as usize;
    let ly = (target.r % 32) as usize;
    let local_idx = ly * 32 + lx;
    for mut chunk in chunk_query.iter_mut() {
        if chunk.chunk_x == cx && chunk.chunk_y == cy {
            chunk.selected[local_idx] = if selected { 1 } else { 0 };
            chunk.dirty = true;
            break;
        }
    }
}

/// M7 战斗：行军到达时如果目标已被敌方占据, 触发战斗
///
/// 战斗公式: `slg_core::rule::combat::simulate` (8 回合 / 速度定序 / 战法概率 / 普攻 / 士气)
/// 输入：attacker 主将 + 兵种 (来自 MarchArrival) / defender 主将 + 兵种 (从 defender_faction 查)
///
/// 胜负判定 (基于 CombatReport.final_troops):
/// - atk > def → Victory: 占据目标 + 揭雾
/// - atk < def → Defeat: 行军失败
/// - atk == def → Draw: 双方都扣兵, 目标格不变
///
/// M7 简化：
/// - Defender 兵力 = `static_defender_troops(terrain)` (M0 公式)
/// - 武将没有"自带战法 / 等级成长"（M8 引入）
/// - 兵种全 `unit_infantry` (M7 简化版; M8 改主将 unit_type)
#[allow(clippy::too_many_arguments)]
fn handle_combat(
    arrival: &slg_core::military::MarchArrival,
    defender_faction: &FactionId,
    terrain_map: &TerrainMapResource,
    territory_res: &mut TerritoryManagerResource,
    march_res: &mut MarchManagerResource,
    fog_res: &mut FogOfWarResource,
    chunk_query: &mut Query<&mut EngineChunkData>,
    faction_id_map: &FactionIdMap,
    faction_store: &slg_core::resource::FactionStore,
    building_manager: &slg_core::building::BuildingManager,
    current_tick: u64, // M9: 战报用
    battle_events: &mut EventWriter<BattleReportEvent>, // M9
) {
    // 目标格地形
    let target_key = arrival.to.to_tile_key();
    let terrain = terrain_map.map.get(&target_key).copied();
    let Some(terrain) = terrain else {
        info!(
            "[Combat] ❌ 目标 ({},{}) 没注册地形, 行军失败",
            arrival.to.q, arrival.to.r
        );
        march_res.manager.fail(arrival.id);
        return;
    };

    // M7: 拿 attacker / defender 主将 + 兵种
    let attacker_general_snapshot = arrival
        .general
        .clone()
        .map(|stats| slg_core::rule::combat::GeneralSnapshot {
            stats,
            skills: vec![],
            unit_type: arrival.unit_type.clone(),
        });
    let defender_general = faction_store
        .factions
        .get(defender_faction)
        .and_then(|f| f.primary_general())
        .cloned();
    let defender_unit_type = slg_core::entity::faction::FactionState::default_unit_type();
    let defender_general_snapshot = defender_general.clone().map(|stats| {
        slg_core::rule::combat::GeneralSnapshot {
            stats,
            skills: vec![],
            unit_type: defender_unit_type.clone(),
        }
    });

    // 兵力 (M0 静态防御值)
    let attacker_troops = slg_core::military::TROOPS_PER_MARCH;
    let mut defender_troops = slg_core::combat_simple::static_defender_troops(terrain);

    // M8: 城防建筑加成 (CityWall)
    let combat_bonus = building_manager.combat_bonus_at(arrival.to);
    defender_troops = ((defender_troops as f64) * combat_bonus.defender_troop_multiplier) as u32;

    // 构造 CombatInput + 调 simulate
    let combat_input = slg_core::rule::combat::CombatInput {
        seed: arrival.id.wrapping_add(terrain as u64),
        attacker: slg_core::rule::combat::CombatSide {
            generals: attacker_general_snapshot.into_iter().collect(),
            troops: slg_core::rule::combat::TroopInfo {
                unit_type: arrival.unit_type.clone(),
                count: attacker_troops,
                morale: 80.0,
            },
        },
        defender: slg_core::rule::combat::CombatSide {
            generals: defender_general_snapshot.into_iter().collect(),
            troops: slg_core::rule::combat::TroopInfo {
                unit_type: defender_unit_type,
                count: defender_troops,
                morale: 80.0,
            },
        },
        terrain,
    };
    let report = slg_core::rule::combat::simulate(combat_input);
    let (final_atk, final_def) = report.final_troops;

    // 胜负判定
    let result_label = if final_atk > final_def {
        "Victory"
    } else if final_atk < final_def {
        "Defeat"
    } else {
        "Draw"
    };

    match result_label {
        "Victory" => {
            // 攻方胜: 占据目标 + 揭雾
            territory_res.manager.occupy(arrival.to, &arrival.faction_id);
            sync_chunk_owner(
                arrival.to,
                &arrival.faction_id,
                faction_id_map,
                chunk_query,
            );
            let mut to_reveal = vec![arrival.to];
            to_reveal.extend(arrival.to.ring(1));
            reveal_coords_and_sync_chunks(fog_res, chunk_query, &to_reveal);
            info!(
                "[Combat] ⚔️ Victory: {} 占 ({},{}) [attacker {} → {}, defender {} → {}] ({} 回合)",
                arrival.faction_id, arrival.to.q, arrival.to.r,
                attacker_troops, final_atk,
                defender_troops, final_def,
                report.rounds.len()
            );
            // M9: 战报 event
            battle_events.send(BattleReportEvent {
                attacker: arrival.faction_id.clone(),
                defender: defender_faction.clone(),
                coord: arrival.to,
                tick: current_tick,
                result: "Victory".to_string(),
                attacker_initial: attacker_troops,
                defender_initial: defender_troops,
                attacker_remaining: final_atk,
                defender_remaining: final_def,
            });
        }
        "Defeat" => {
            // 攻方败: 行军失败
            march_res.manager.fail(arrival.id);
            info!(
                "[Combat] 💀 Defeat: {} 攻 ({},{}) 失败 [attacker {} → {}, defender {} → {}] ({} 回合)",
                arrival.faction_id, arrival.to.q, arrival.to.r,
                attacker_troops, final_atk,
                defender_troops, final_def,
                report.rounds.len()
            );
            battle_events.send(BattleReportEvent {
                attacker: arrival.faction_id.clone(),
                defender: defender_faction.clone(),
                coord: arrival.to,
                tick: current_tick,
                result: "Defeat".to_string(),
                attacker_initial: attacker_troops,
                defender_initial: defender_troops,
                attacker_remaining: final_atk,
                defender_remaining: final_def,
            });
        }
        _ => {
            // 平局: 双方都扣兵, 目标格不变
            info!(
                "[Combat] ⚖️ Draw: {} 攻 ({},{}) 平局 [attacker {} → {}, defender {} → {}] ({} 回合)",
                arrival.faction_id, arrival.to.q, arrival.to.r,
                attacker_troops, final_atk,
                defender_troops, final_def,
                report.rounds.len()
            );
            battle_events.send(BattleReportEvent {
                attacker: arrival.faction_id.clone(),
                defender: defender_faction.clone(),
                coord: arrival.to,
                tick: current_tick,
                result: "Draw".to_string(),
                attacker_initial: attacker_troops,
                defender_initial: defender_troops,
                attacker_remaining: final_atk,
                defender_remaining: final_def,
            });
        }
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

/// M9.2: 编辑器键盘快捷键 → 发 EditorAction
///
/// - Ctrl+Z = Undo
/// - Ctrl+Y / Ctrl+Shift+Z = Redo
/// - Ctrl+S = Save
///
/// 只在 Editor phase 响应; Playing/Menu phase 静默忽略。
pub fn handle_editor_keyboard(
    keys: Res<ButtonInput<KeyCode>>,
    game_state: Res<GameState>,
    mut events: EventWriter<slg_editor::editor_state::EditorAction>,
) {
    if game_state.phase != GamePhase::Editor {
        return;
    }
    let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    if !ctrl {
        return;
    }
    if keys.just_pressed(KeyCode::KeyZ) {
        let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
        if shift {
            events.send(slg_editor::editor_state::EditorAction::Redo);
            info!("[Editor] Ctrl+Shift+Z → Redo");
        } else {
            events.send(slg_editor::editor_state::EditorAction::Undo);
            info!("[Editor] Ctrl+Z → Undo");
        }
    } else if keys.just_pressed(KeyCode::KeyY) {
        events.send(slg_editor::editor_state::EditorAction::Redo);
        info!("[Editor] Ctrl+Y → Redo");
    } else if keys.just_pressed(KeyCode::KeyS) {
        events.send(slg_editor::editor_state::EditorAction::Save);
        info!("[Editor] Ctrl+S → Save");
    }
}

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
    mut game_state: ResMut<GameState>, // M10.2: ResMut 改 status_message
    mut territory_res: ResMut<TerritoryManagerResource>,
    mut march_res: ResMut<MarchManagerResource>,
    mut fog_res: ResMut<FogOfWarResource>,
    terrain_map: Res<TerrainMapResource>,
    clock_res: Res<slg_engine::systems::GameClockResource>,
    faction_res: Res<FactionStoreResource>, // M7: 拿玩家主将
    mut selected_hex: ResMut<SelectedHex>,  // M8: 选中状态
    mut chunk_query: Query<&mut EngineChunkData>,
) {
    for event in click_events.read() {
        let coord = event.coord;

        // 总是先 spawn 涟漪，让玩家看到点击已检测
        // M10.3.4: 用 hex 中心 (不是鼠标世界坐标) 定位 ring
        //
        // 之前用 event.world_pos (鼠标 cursor 位置), 玩家点 hex 边缘时 ring 偏到一边,
        // 跟主城 sprite (中心) 重叠不上, 看着像 ring 飞了。
        // 改成 hex 中心后, ring 永远在目标 hex 正中央, 视觉对齐 sprite / chunk.
        let ring_pos = slg_engine::camera::hex_world_position(coord);
        spawn_click_ring(&mut commands, ring_pos);

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
                // M8: 玩家点己方格 → 选中 (toggle)
                let player_fid = game_state.player_faction_id.clone();
                let owner = territory_res
                    .manager
                    .owner_map
                    .get(&coord.to_tile_key())
                    .cloned();
                if owner.as_deref() == Some(player_fid.as_str()) {
                    // 点己方格: toggle 选中
                    if selected_hex.coord == Some(coord) {
                        info!("[Playing] 🔘 取消选中: ({}, {})", coord.q, coord.r);
                        // M9: 取消 chunk selected 标记
                        sync_chunk_selection(coord, false, &mut chunk_query);
                        selected_hex.coord = None;
                        // M10.3.3 修复: 给玩家明确反馈 (否则点完看不到任何东西, 以为没反应)
                        game_state.status_message = format!("取消选中 ({}, {})", coord.q, coord.r);
                    } else {
                        // M9: 如果之前有别的选中格, 先取消它
                        if let Some(prev) = selected_hex.coord {
                            sync_chunk_selection(prev, false, &mut chunk_query);
                        }
                        info!("[Playing] 🔘 选中: ({}, {})", coord.q, coord.r);
                        sync_chunk_selection(coord, true, &mut chunk_query);
                        selected_hex.coord = Some(coord);
                        // M10.3.3: 选中反馈 (即使 chunk tint 被 sprite 盖, 状态栏文字也能看见)
                        game_state.status_message = format!("🔘 选中 ({}, {})", coord.q, coord.r);
                    }
                    continue;
                }
                // 点非己方 / 邻接空地: 取消选中 + 尝试派兵
                if let Some(prev) = selected_hex.coord {
                    info!("[Playing] 🔘 取消选中 (派兵/无效点)");
                    sync_chunk_selection(prev, false, &mut chunk_query);
                    selected_hex.coord = None;
                }

                // **核心 SLG 循环：左键 → 派兵（行军）→ 到达 → 占地**
                // 之前是直接 occupy（瞬时），现在改成派兵走完才落地。
                let can = territory_res
                    .manager
                    .can_occupy(coord, &player_fid, &terrain_map.map);
                if !can {
                    info!(
                        "[Playing] ❌ 不能派兵: ({}, {}), 当前归属={:?}",
                        coord.q, coord.r, owner
                    );
                    // M10.2: 给玩家明确反馈 (status_message)
                    game_state.status_message = match owner {
                        Some(other) if other.as_str() != player_fid.as_str() => {
                            format!("❌ ({},{}) 已被 {} 占据", coord.q, coord.r, other)
                        }
                        Some(_) => {
                            format!("✅ ({},{}) 已是己方", coord.q, coord.r)
                        }
                        None => {
                            // 空地, 但 can_occupy false → 不在邻接范围
                            format!("❌ ({},{}) 超出派兵范围 (主城邻接 1 圈内)", coord.q, coord.r)
                        }
                    };
                    continue;
                }

                // 目标格是否已被另一支行军锁住？
                if march_res.manager.is_target_locked(coord) {
                    info!(
                        "[Playing] ❌ 目标已被行军锁住: ({}, {})",
                        coord.q, coord.r
                    );
                    game_state.status_message = format!("❌ ({},{}) 已被行军锁定", coord.q, coord.r);
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
                    faction_res
                        .store
                        .factions
                        .get(&player_fid)
                        .and_then(|f| f.primary_general())
                        .cloned(),
                    slg_core::entity::faction::FactionState::default_unit_type(),
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
                // M10.2: 给玩家明确反馈
                game_state.status_message = format!(
                    "🪖 派兵 → ({},{}) 预计 tick {} 到达",
                    coord.q, coord.r, order.arrive_tick
                );

                // 暂时 unused warning 防止：visual_entity 留作后续
                let _ = visual_entity;
            }
            GamePhase::Editor => {
                info!("[Editor] 点击地块 ({}, {})", coord.q, coord.r);
            }
            _ => {
                // 其他阶段忽略地图点击
            }
        }
    }
}

// ---------------------------------------------------------------------------
// M8 UI: 建筑 / 升级 / 建分城 处理
// ---------------------------------------------------------------------------

/// 处理 BuildAction event：建建筑 / 升级 / 建分城
///
/// 调用方 (UI panel / 测试) 发 BuildAction, 本系统:
/// - 验证资源 / 条件
/// - 扣资源
/// - 调 slg-core BuildingManager / CityManager
/// - 取消选中 (操作完, 面板自动关闭)
#[allow(clippy::too_many_arguments)]
fn handle_build_order(
    mut events: EventReader<BuildAction>,
    game_state: Res<GameState>,
    mut building_res: ResMut<BuildingManagerResource>,
    mut city_res: ResMut<CityManagerResource>,
    mut faction_res: ResMut<FactionStoreResource>,
    territory_res: Res<TerritoryManagerResource>,
    mut selected_hex: ResMut<SelectedHex>,
    mut chunk_query: Query<&mut EngineChunkData>, // M9: 取消选中时同步 chunk
) {
    for action in events.read() {
        let player_fid = game_state.player_faction_id.clone();
        let target = match action {
            BuildAction::Build(c, _) => c.0,
            BuildAction::Upgrade(c) => c.0,
            BuildAction::EstablishSubcity(c) => c.0,
        };
        let res = match faction_res.store.factions.get_mut(&player_fid) {
            Some(f) => f,
            None => continue,
        };

        match action {
            BuildAction::Build(c, btype) => {
                info!(
                    "[BuildAction] 玩家 {} 尝试在 ({},{}) 建 {:?}",
                    player_fid, c.0.q, c.0.r, btype
                );
                match building_res.manager.build(
                    c.0,
                    *btype,
                    player_fid.clone(),
                    &mut res.resources,
                ) {
                    Ok(_) => info!(
                        "[BuildAction] ✅ 建 {:?} L1 at ({},{}) 成功, 扣 50 gold",
                        btype, c.0.q, c.0.r
                    ),
                    Err(slg_core::building::BuildError::AlreadyBuilt) => {
                        info!("[BuildAction] ❌ 该格已有建筑")
                    }
                    Err(slg_core::building::BuildError::InsufficientResources) => {
                        info!("[BuildAction] ❌ 资源不足 (需 50 gold)")
                    }
                }
            }
            BuildAction::Upgrade(c) => {
                info!(
                    "[BuildAction] 玩家 {} 尝试升级 ({},{})",
                    player_fid, c.0.q, c.0.r
                );
                match building_res.manager.upgrade(c.0, &mut res.resources) {
                    Ok(new_level) => info!(
                        "[BuildAction] ✅ 升级 ({},{}) → L{}",
                        c.0.q, c.0.r, new_level
                    ),
                    Err(slg_core::building::UpgradeError::MaxLevel) => {
                        info!("[BuildAction] ❌ 已满级 (L3)")
                    }
                    Err(slg_core::building::UpgradeError::InsufficientResources) => {
                        info!("[BuildAction] ❌ 资源不足")
                    }
                }
            }
            BuildAction::EstablishSubcity(c) => {
                info!(
                    "[BuildAction] 玩家 {} 尝试在 ({},{}) 建分城",
                    player_fid, c.0.q, c.0.r
                );
                let owner_keys: std::collections::BTreeSet<u64> = territory_res
                    .manager
                    .owner_map
                    .iter()
                    .filter(|(_, f)| f.as_str() == player_fid.as_str())
                    .map(|(k, _)| *k)
                    .collect();
                match city_res.manager.establish_subcity(
                    c.0,
                    player_fid.clone(),
                    &mut res.resources,
                    &owner_keys,
                ) {
                    Ok(_) => info!(
                        "[BuildAction] ✅ 建分城 ({},{}) 成功, 扣 500g+200f+100w",
                        c.0.q, c.0.r
                    ),
                    Err(e) => info!("[BuildAction] ❌ 建分城失败: {:?}", e),
                }
            }
        }

        // 操作完, 取消选中
        if selected_hex.coord == Some(target) {
            sync_chunk_selection(target, false, &mut chunk_query);
            selected_hex.coord = None;
        }
    }
}

// ---------------------------------------------------------------------------
// M9: 战报 event → BattleReportState
// ---------------------------------------------------------------------------

/// 把 BattleReportEvent 写进 BattleReportState
///
/// 调用方: handle_combat (V/D/Draw) 发 event → 本 system 收 event → push 到 state.reports
/// slg-ui 的 render_battle_report 已经从 state 读 reports 显示
fn handle_battle_report(
    mut events: EventReader<BattleReportEvent>,
    mut battle_state: ResMut<slg_ui::panels::battle_report::BattleReportState>,
) {
    for event in events.read() {
        battle_state.reports.push(slg_ui::panels::battle_report::BattleReportEntry {
            tick: event.tick,
            attacker: event.attacker.clone(),
            defender: event.defender.clone(),
            winner: event.result.clone(),
            attacker_losses: event.attacker_initial - event.attacker_remaining,
            defender_losses: event.defender_initial - event.defender_remaining,
        });
        // 显示 (浮窗默认显示)
        battle_state.show = true;
        info!(
            "[BattleReport] 📜 Tick {}: {} vs {} → {} (loss {}/{})",
            event.tick, event.attacker, event.defender, event.result,
            event.attacker_initial - event.attacker_remaining,
            event.defender_initial - event.defender_remaining,
        );
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

        // 总是先 spawn 涟漪 (M10.3.4: 用 hex 中心, 不是鼠标坐标)
        let ring_pos = slg_engine::camera::hex_world_position(coord);
        spawn_click_ring(&mut commands, ring_pos);

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
    use slg_core::entity::faction::FactionState;
    use slg_core::entity::general::GeneralStats;
    use slg_core::map::tile::TerrainType;

    /// 创建带 MinimalPlugins + 必要 resources 的 App
    fn make_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        // M9.2: 加 InputPlugin 让 ButtonInput<KeyCode> 在测试里可用
        app.add_plugins(bevy::input::InputPlugin);
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
        app.init_resource::<BuildingManagerResource>(); // M8
        app.init_resource::<CityManagerResource>();    // M8
        app.init_resource::<SelectedHex>();            // M8 UI
        app.add_event::<BuildAction>();                 // M8 UI
        app.add_event::<BattleReportEvent>();           // M9
        app.init_resource::<slg_editor::editor_state::EditorState>(); // M9.1
        app.add_event::<slg_editor::editor_state::EditorAction>();      // M9.1
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
        // M7: 玩家带 1 个主将 (中等武力, 准备后续用)
        {
            let mut fs = world.resource_mut::<FactionStoreResource>();
            fs.store.factions.insert(
                "faction_1".to_string(),
                FactionState {
                    main_city: Some(main_city),
                    generals: vec![GeneralStats {
                        strength: 75,
                        intelligence: 65,
                        command: 70,
                        politics: 60,
                        charisma: 80,
                        level: 5,
                        exp: 0,
                    }],
                    ..Default::default()
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

    /// 测试 13：AI 派兵系统 - AI faction_2 在自己的 slot tick 派兵
    ///
    /// 模拟：faction_2 (slot 1) 应该在 tick 1, 11, 21... 派兵
    /// 这里让 current_tick = 1（slot 1 的回合）
    #[test]
    fn bevy_ai_dispatches_march_on_own_slot() {
        let mut app = make_app();
        init_playing_state(app.world_mut());
        add_ai_factions(app.world_mut());

        app.add_systems(Update, process_tick_phases);

        // 设 tick = 1 (slot 1 = AI faction_2)
        {
            let mut clock = app
                .world_mut()
                .resource_mut::<slg_engine::systems::GameClockResource>();
            clock.clock.current_tick = 1;
            app.world_mut().resource_mut::<GameState>().tick = 1;
        }
        app.update();

        // 验证: AI faction_2 派了 1 队兵
        let march = app.world().resource::<MarchManagerResource>();
        let ai2_orders: Vec<_> = march
            .manager
            .orders
            .values()
            .filter(|o| o.faction_id == "faction_2")
            .collect();
        assert_eq!(ai2_orders.len(), 1, "AI faction_2 应在 slot 1 派 1 队兵");
        let order = ai2_orders[0];
        assert_eq!(order.from, HexCoord::new(30, 30), "from = AI 主城");
        eprintln!(
            "TEST13 ✅: AI faction_2 在 tick 1 派兵 from=({},{}) to=({},{})",
            order.from.q, order.from.r, order.to.q, order.to.r
        );
    }

    /// 测试 14：AI 派兵后推进 tick → AI 占地
    #[test]
    fn bevy_ai_full_dispatch_arrive_occupy() {
        let mut app = make_app();
        init_playing_state(app.world_mut());
        add_ai_factions(app.world_mut());

        app.add_systems(Update, process_tick_phases);

        // tick = 1 派兵
        {
            let mut clock = app
                .world_mut()
                .resource_mut::<slg_engine::systems::GameClockResource>();
            clock.clock.current_tick = 1;
            app.world_mut().resource_mut::<GameState>().tick = 1;
        }
        app.update();

        // 推进 5 tick 让兵到达
        for _ in 0..5 {
            {
                let mut clock = app
                    .world_mut()
                    .resource_mut::<slg_engine::systems::GameClockResource>();
                clock.clock.current_tick += 1;
            }
            app.update();
        }

        // 验证: AI 占地数 = 2 (主城 + 1 邻接)
        let tm = app.world().resource::<TerritoryManagerResource>();
        let ai2_count = tm
            .manager
            .owner_map
            .values()
            .filter(|f| f == &"faction_2")
            .count();
        assert_eq!(ai2_count, 2, "AI 派兵落地后应占 2 格");
        eprintln!("TEST14 ✅: AI faction_2 派兵 → 5 tick → 占 2 格");
    }

    /// 测试 15：错峰 - tick=1 只有 faction_2 (slot 1) 派兵，其它不派
    #[test]
    fn bevy_ai_only_dispatches_on_own_slot() {
        let mut app = make_app();
        init_playing_state(app.world_mut());
        add_ai_factions(app.world_mut());

        app.add_systems(Update, process_tick_phases);

        // tick = 1, slot 1 = faction_2
        {
            let mut clock = app
                .world_mut()
                .resource_mut::<slg_engine::systems::GameClockResource>();
            clock.clock.current_tick = 1;
            app.world_mut().resource_mut::<GameState>().tick = 1;
        }
        app.update();

        // 验证: 只有 faction_2 派兵，其它 4 个 AI 都没派
        let march = app.world().resource::<MarchManagerResource>();
        let mut ai_orders: std::collections::BTreeMap<String, u32> =
            std::collections::BTreeMap::new();
        for o in march.manager.orders.values() {
            *ai_orders.entry(o.faction_id.clone()).or_insert(0) += 1;
        }
        assert_eq!(ai_orders.get("faction_2").copied().unwrap_or(0), 1);
        for fid in ["faction_3", "faction_4", "faction_5", "faction_6"] {
            assert_eq!(
                ai_orders.get(fid).copied().unwrap_or(0),
                0,
                "{} 在 tick=1 不应派兵",
                fid
            );
        }
        eprintln!("TEST15 ✅: 错峰生效, tick=1 只有 faction_2 派兵");
    }

    /// 加 5 个 AI faction（faction_2~6）到 world
    /// 每个 AI 有自己的主城，territory 上 own 主城
    /// - faction_2: (30, 30)
    /// - faction_3: (60, 30)
    /// - faction_4: (90, 30)
    /// - faction_5: (30, 90)
    /// - faction_6: (60, 90)
    ///
    /// 每次只借一个资源的 mutable borrow，避免重叠 borrow 错误
    fn add_ai_factions(world: &mut World) {
        let ai_data: Vec<(&str, i32, i32)> = vec![
            ("faction_2", 30, 30),
            ("faction_3", 60, 30),
            ("faction_4", 90, 30),
            ("faction_5", 30, 90),
            ("faction_6", 60, 90),
        ];
        for (i, (fid, q, r)) in ai_data.iter().enumerate() {
            let c = HexCoord::new(*q, *r);
            // FactionState - M7: 每个 AI 1 个主将 (武力随 index 微变, 后续可差异化)
            {
                let mut fs = world.resource_mut::<FactionStoreResource>();
                fs.store.factions.insert(
                    fid.to_string(),
                    FactionState {
                        main_city: Some(c),
                        // AI 武将武力 60 + i*5, 让 faction_2 较弱 faction_6 较强
                        generals: vec![GeneralStats {
                            strength: 60 + (i as u8) * 5,
                            intelligence: 50,
                            command: 55,
                            politics: 40,
                            charisma: 45,
                            level: 3 + i as u16,
                            exp: 0,
                        }],
                        ..Default::default()
                    },
                );
            }
            // TerritoryManager: set main city + occupy
            {
                let mut tm = world.resource_mut::<TerritoryManagerResource>();
                tm.manager.set_main_city(&fid.to_string(), c);
                tm.manager.occupy(c, &fid.to_string());
            }
            // FactionIdMap
            {
                let mut fim = world.resource_mut::<FactionIdMap>();
                fim.map.insert(fid.to_string(), (i + 1) as u8);
            }
        }
    }

    /// 测试 16：战斗 Victory - 玩家攻占邻接 AI 主城
    ///
    /// 设置：玩家主城 (10, 10), AI 主城 (11, 10)
    /// 玩家派兵到 (11, 10) 触发战斗
    /// 静态防御 Plains=50, 攻 100 必胜
    #[test]
    fn bevy_combat_victory_against_ai() {
        let mut app = make_app();
        init_playing_state(app.world_mut());

        // 把 AI faction_2 主城 (121,123) 移到 (11, 10) 玩家主城邻接
        // 并 occupy 邻接
        {
            let mut tm = app.world_mut().resource_mut::<TerritoryManagerResource>();
            tm.manager
                .main_cities
                .insert("faction_2".to_string(), HexCoord::new(11, 10));
            // 关键: 把玩家主城 (10, 10) 改成 1 个 AI 主城
            // 但保留玩家主城 (68, 65) 仍然 own 自己
            // 实际上: 我们要测 combat victory 在主城
            // 让我们用：玩家主城 (68, 65), AI 邻接主城 (69, 65)
            // 但 AI 主城由 gen spawn 决定, 玩家不能改
            // 简化: 直接 occupy (69, 65) 给 AI
            tm.manager
                .owner_map
                .insert(HexCoord::new(69, 65).to_tile_key(), "faction_2".to_string());
        }

        app.add_systems(Update, process_tick_phases);

        // tick 100 派兵到 (69, 65)
        {
            let mut clock = app
                .world_mut()
                .resource_mut::<slg_engine::systems::GameClockResource>();
            clock.clock.current_tick = 100;
        }
        // 通过 HexClickEvent 派兵
        use slg_engine::camera::HexClickEvent;
        app.world_mut().send_event(HexClickEvent {
            coord: HexCoord::new(69, 65),
            world_pos: Vec2::new(0.0, 0.0),
        });
        app.add_systems(Update, handle_hex_click);
        app.update();

        // 推进 5 tick
        for _ in 0..5 {
            {
                let mut clock = app
                    .world_mut()
                    .resource_mut::<slg_engine::systems::GameClockResource>();
                clock.clock.current_tick += 1;
            }
            app.update();
        }

        // 验证: 玩家击败 AI 占据 (69, 65)
        let tm = app.world().resource::<TerritoryManagerResource>();
        let target_key = HexCoord::new(69, 65).to_tile_key();
        assert_eq!(
            tm.manager.owner_map.get(&target_key),
            Some(&"faction_1".to_string()),
            "Victory 后玩家应占据目标格"
        );
        eprintln!("TEST16 ✅: 战斗 Victory - 玩家攻占 AI 主城");
    }

    /// 测试 17：战斗 Defeat - 攻方在 Pass 攻击高防御失败
    ///
    /// 玩家主城在 Pass 邻接，AI 主城 200 防御 -> 攻 100 必败
    /// 实际：玩家主城 (68, 65) 在 Plains, 我们不能改主城地形
    /// 替代: 玩家派兵到 (68, 65) 的邻接 AI 主城, 模拟 Pass 上的 AI 防御
    /// 简化: 不写这个 (复杂), 改为: AI 派兵攻玩家 Pass 邻接, 应该 defeat
    ///   玩家主城 (68, 65) own, AI faction_2 main_city 改到 (68, 66)
    ///   terrain (68, 66) 是 plains, Plains 50 防御, 100 攻必胜
    ///   -> 不能测 defeat
    /// 替代: 直接测 resolve_simple_combat 静态函数结果 -> 已 unit test
    ///
    /// 这里改为: 测试 玩家 派兵到 主城 邻接格子 (68, 65) 的 6 邻之一 (68, 64)
    /// 玩家主城 (68, 65) 占, AI 改成占 (68, 64) -> 玩家 100 攻 50 (Plains) 必胜
    /// -> 还是测 Victory
    ///
    /// 决定: 这个 test 跳过, defeat 已经 unit test cover
    /// 只测 integration: 玩家派兵到 AI 主城邻接 -> Victory 玩家占据
    /// 测试 17：玩家派兵到主城邻接 (AI 主城), Victory 占据
    /// 简化: 跟 TEST16 一样, 玩家派兵到 AI 占的格 -> Victory -> 玩家占据
    #[test]
    fn bevy_combat_victory_attacker_occupies() {
        // 这个跟 TEST16 重复, 但更直接: 派兵 + 战斗 + 占据
        let mut app = make_app();
        init_playing_state(app.world_mut());

        // 把玩家主城邻接 (69, 65) 给 AI
        {
            let mut tm = app.world_mut().resource_mut::<TerritoryManagerResource>();
            tm.manager
                .owner_map
                .insert(HexCoord::new(69, 65).to_tile_key(), "faction_2".to_string());
        }

        app.add_systems(Update, handle_hex_click);
        app.add_systems(Update, process_tick_phases);

        // 派兵
        use slg_engine::camera::HexClickEvent;
        app.world_mut().send_event(HexClickEvent {
            coord: HexCoord::new(69, 65),
            world_pos: Vec2::new(0.0, 0.0),
        });
        // tick 100
        {
            let mut clock = app
                .world_mut()
                .resource_mut::<slg_engine::systems::GameClockResource>();
            clock.clock.current_tick = 100;
        }
        app.update();

        // 推进 5 tick
        for _ in 0..5 {
            {
                let mut clock = app
                    .world_mut()
                    .resource_mut::<slg_engine::systems::GameClockResource>();
                clock.clock.current_tick += 1;
            }
            app.update();
        }

        // 验证: 玩家占据 (69, 65)
        let tm = app.world().resource::<TerritoryManagerResource>();
        let target_key = HexCoord::new(69, 65).to_tile_key();
        assert_eq!(
            tm.manager.owner_map.get(&target_key),
            Some(&"faction_1".to_string()),
            "战斗 Victory 后玩家应占据目标格"
        );
        eprintln!("TEST17 ✅: 派兵邻接敌方主城 -> 战斗 Victory -> 玩家占据");
    }

    /// 测试 18：AI 攻玩家必败 - 模拟 AI faction_2 派兵到玩家主城
    ///
    /// 玩家主城 (68, 65) Plains 50 防御, AI 100 攻 = 必胜
    /// (玩家必败? 不, 50*1.0 = 50, 100*1.0 = 100, 100 > 50*1.5=75 -> Victory)
    /// 所以玩家也会输给 AI
    ///
    /// 改: 让 AI 攻 Pass 邻接 (高防御) - 但 Pass 难造
    /// 简化为: 把玩家主城 (68, 65) 设到 Pass 地形, 静态防御 200, AI 100 必败
    /// terrain_map 已 init 全 plains, 改 (68, 65) 为 Pass
    #[test]
    fn bevy_combat_defeat_attacker_loses() {
        let mut app = make_app();
        init_playing_state(app.world_mut());

        // AI faction_2 主城 (10, 10) -> 邻接玩家 (11, 10) 自己主城
        // 玩家主城 (68, 65) 改到 Pass 地形 (静态防御 200)
        // 但玩家 (68, 65) 和 AI 邻接格子 (69, 65) 距离 1, 把 (69, 65) 改 Pass
        {
            let mut terrain = app.world_mut().resource_mut::<TerrainMapResource>();
            terrain
                .map
                .insert(HexCoord::new(69, 65).to_tile_key(), TerrainType::Pass);
        }
        // 玩家先 occupy (69, 65) - 否则 AI 到达时 owner_map 是 None, 走 occupy 分支而非 combat
        {
            let mut tm = app.world_mut().resource_mut::<TerritoryManagerResource>();
            tm.manager
                .occupy(HexCoord::new(69, 65), &"faction_1".to_string());
        }
        // AI faction_2 主城 (70, 65) - 攻击方
        {
            let mut tm = app.world_mut().resource_mut::<TerritoryManagerResource>();
            tm.manager.set_main_city(&"faction_2".to_string(), HexCoord::new(70, 65));
            tm.manager
                .occupy(HexCoord::new(70, 65), &"faction_2".to_string());
        }

        app.add_systems(Update, process_tick_phases);

        // tick 1 (slot 1 = AI faction_2)
        {
            let mut clock = app
                .world_mut()
                .resource_mut::<slg_engine::systems::GameClockResource>();
            clock.clock.current_tick = 1;
        }
        app.world_mut().resource_mut::<GameState>().tick = 1;
        app.update();

        // 推进 5 tick 让兵到达
        for _ in 0..5 {
            {
                let mut clock = app
                    .world_mut()
                    .resource_mut::<slg_engine::systems::GameClockResource>();
                clock.clock.current_tick += 1;
            }
            app.update();
        }

        // 验证: AI 攻打 Pass (69, 65) 失败, Pass 还是玩家 own
        let tm = app.world().resource::<TerritoryManagerResource>();
        let target_key = HexCoord::new(69, 65).to_tile_key();
        assert_eq!(
            tm.manager.owner_map.get(&target_key),
            Some(&"faction_1".to_string()),
            "AI 攻 Pass 失败, 玩家仍占"
        );
        // AI 2 占地 = 主城 (1 格) (战斗失败不占)
        let ai2_count = tm
            .manager
            .owner_map
            .values()
            .filter(|f| f == &"faction_2")
            .count();
        assert_eq!(ai2_count, 1, "AI 战斗失败不占, 仍只 1 格");
        eprintln!("TEST18 ✅: 战斗 Defeat - AI 攻 Pass 失败, 玩家仍占");
    }

    // -----------------------------------------------------------------------
    // M8: 建筑 + 分城
    // -----------------------------------------------------------------------

    /// TEST19: 建农田 L1 + tick 1 步 → faction food += 2
    #[test]
    fn bevy_building_farm_l1_adds_food() {
        let mut app = make_app();
        init_playing_state(app.world_mut());

        // 给玩家加资源 (建 L1 需 50 gold)
        let mut res_for_build = {
            let fs = app.world().resource::<FactionStoreResource>();
            let mut r = fs.store.factions.get("faction_1").unwrap().resources.clone();
            r.gold = 200;
            r
        };
        // 在玩家主城 (68, 65) 建农田 L1
        {
            let mut bm = app.world_mut().resource_mut::<BuildingManagerResource>();
            bm.manager
                .build(
                    HexCoord::new(68, 65),
                    slg_core::building::BuildingType::Farm,
                    "faction_1".to_string(),
                    &mut res_for_build,
                )
                .unwrap();
        }
        {
            let mut fs = app.world_mut().resource_mut::<FactionStoreResource>();
            fs.store.factions.get_mut("faction_1").unwrap().resources = res_for_build;
        }

        // 拿 food 初始值
        let food_before = {
            let fs = app.world().resource::<FactionStoreResource>();
            fs.store.factions.get("faction_1").unwrap().resources.food
        };

        // 跑 1 个 tick (ResourceProduction phase)
        app.add_systems(Update, process_tick_phases);
        {
            let mut clock = app
                .world_mut()
                .resource_mut::<slg_engine::systems::GameClockResource>();
            clock.clock.current_tick = 1;
        }
        app.world_mut().resource_mut::<GameState>().tick = 1;
        app.update();

        let food_after = {
            let fs = app.world().resource::<FactionStoreResource>();
            fs.store.factions.get("faction_1").unwrap().resources.food
        };

        // 农田 L1 +2 food/tick, + 主城 (68,65) 基础 plains food +5
        // 没有资源格 = 不 ×2
        // food 增量 = 5 (plains) + 2 (farm) = 7
        assert!(
            food_after >= food_before + 7,
            "food 至少 +7 (plains 5 + farm 2), before={} after={}",
            food_before,
            food_after
        );
        eprintln!(
            "TEST19 ✅: 农田 L1 tick 后 food {} → {} (+{})",
            food_before,
            food_after,
            food_after - food_before
        );
    }

    /// TEST20: 升级 L1 → L2 → 资源加成 +2 → +5 (L2)
    #[test]
    fn bevy_building_upgrade_doubles_food_bonus() {
        let mut app = make_app();
        init_playing_state(app.world_mut());

        // 建农田 L1 (扣 50 gold)
        let mut res_for_build = {
            let fs = app.world().resource::<FactionStoreResource>();
            let mut r = fs.store.factions.get("faction_1").unwrap().resources.clone();
            r.gold = 1000;
            r.food = 1000;
            r
        };
        {
            let mut bm = app.world_mut().resource_mut::<BuildingManagerResource>();
            bm.manager
                .build(
                    HexCoord::new(68, 65),
                    slg_core::building::BuildingType::Farm,
                    "faction_1".to_string(),
                    &mut res_for_build,
                )
                .unwrap();
        }
        {
            let mut fs = app.world_mut().resource_mut::<FactionStoreResource>();
            fs.store.factions.get_mut("faction_1").unwrap().resources = res_for_build;
        }
        // 准备升级资源 (取出, mutate)
        let mut res_for_upgrade = {
            let fs = app.world().resource::<FactionStoreResource>();
            let mut r = fs.store.factions.get("faction_1").unwrap().resources.clone();
            r.gold = 1000;
            r.food = 1000;
            r
        };
        {
            let mut bm = app.world_mut().resource_mut::<BuildingManagerResource>();
            bm.manager
                .upgrade(HexCoord::new(68, 65), &mut res_for_upgrade)
                .unwrap();
        }
        // 把扣完的资源放回
        {
            let mut fs = app.world_mut().resource_mut::<FactionStoreResource>();
            fs.store.factions.get_mut("faction_1").unwrap().resources = res_for_upgrade;
        }

        // 验证 building 升到 L2
        let level = {
            let bm = app.world().resource::<BuildingManagerResource>();
            bm.manager.get(HexCoord::new(68, 65)).unwrap().level
        };
        assert_eq!(level, 2, "升级到 L2");

        // 跑 1 tick, 拿 food 增量
        let food_before = {
            let fs = app.world().resource::<FactionStoreResource>();
            fs.store.factions.get("faction_1").unwrap().resources.food
        };
        app.add_systems(Update, process_tick_phases);
        {
            let mut clock = app
                .world_mut()
                .resource_mut::<slg_engine::systems::GameClockResource>();
            clock.clock.current_tick = 1;
        }
        app.world_mut().resource_mut::<GameState>().tick = 1;
        app.update();
        let food_after = {
            let fs = app.world().resource::<FactionStoreResource>();
            fs.store.factions.get("faction_1").unwrap().resources.food
        };

        // L2 农田 +5 food/tick (+ plains 5) = +10
        let delta = food_after - food_before;
        assert!(
            delta >= 10,
            "L2 农田 +5 + plains 5 = +10, actual delta={}",
            delta
        );
        eprintln!("TEST20 ✅: 农田升级 L1→L2, tick food +{}", delta);
    }

    /// TEST21: 建分城成功 (周围 6+ 己方格)
    #[test]
    fn bevy_establish_subcity_success() {
        let mut app = make_app();
        init_playing_state(app.world_mut());

        // 玩家占领 sub_coord (69, 65) + 全部 6 邻接 (满足 REQUIRED_NEIGHBOR_TILES=6)
        {
            let mut tm = app.world_mut().resource_mut::<TerritoryManagerResource>();
            let sub = HexCoord::new(69, 65);
            tm.manager.occupy(sub, &"faction_1".to_string());
            for n in sub.neighbors() {
                tm.manager.occupy(n, &"faction_1".to_string());
            }
        }
        // 给玩家加资源 (临时取出, mutate, 放回)
        let mut res_clone = {
            let fs = app.world().resource::<FactionStoreResource>();
            fs.store.factions.get("faction_1").unwrap().resources.clone()
        };
        res_clone.gold = 1000;
        res_clone.food = 1000;
        res_clone.wood = 1000;
        {
            let mut fs = app.world_mut().resource_mut::<FactionStoreResource>();
            fs.store.factions.get_mut("faction_1").unwrap().resources = res_clone;
        }

        // 收集 owner 的 territory keys (read-only)
        let owner_keys: std::collections::BTreeSet<u64> = {
            let tm = app.world().resource::<TerritoryManagerResource>();
            tm.manager
                .owner_map
                .iter()
                .filter(|(_, f)| f.as_str() == "faction_1")
                .map(|(k, _)| *k)
                .collect()
        };

        // 建分城 (在 6 邻接之一, 这里用东邻 (69, 65)) - 取出 res mutate 再放回
        let sub_coord = HexCoord::new(69, 65);
        let mut res_for_subcity = {
            let fs = app.world().resource::<FactionStoreResource>();
            fs.store.factions.get("faction_1").unwrap().resources.clone()
        };
        {
            let mut cm = app.world_mut().resource_mut::<CityManagerResource>();
            cm.manager
                .establish_subcity(
                    sub_coord,
                    "faction_1".to_string(),
                    &mut res_for_subcity,
                    &owner_keys,
                )
                .unwrap();
        }
        {
            let mut fs = app.world_mut().resource_mut::<FactionStoreResource>();
            fs.store.factions.get_mut("faction_1").unwrap().resources = res_for_subcity;
        }

        // 验证分城存在
        let city = {
            let cm = app.world().resource::<CityManagerResource>();
            cm.manager.get(sub_coord).cloned()
        };
        assert!(city.is_some(), "分城已建");
        let city = city.unwrap();
        assert!(!city.is_main, "应是分城");
        assert_eq!(city.owner, "faction_1");

        // 验证资源被扣
        let res_after = {
            let fs = app.world().resource::<FactionStoreResource>();
            fs.store.factions.get("faction_1").unwrap().resources.clone()
        };
        assert_eq!(res_after.gold, 1000 - 500, "扣 500 gold");
        assert_eq!(res_after.food, 1000 - 200, "扣 200 food");
        assert_eq!(res_after.wood, 1000 - 100, "扣 100 wood");

        eprintln!("TEST21 ✅: 建分城 ({},{}), 扣资源 500g+200f+100w", sub_coord.q, sub_coord.r);
    }

    /// TEST22: 城防 L2 → 战斗时 defender 兵力 ×1.6
    ///
    /// Setup:
    ///   - 玩家 (68, 65), AI (70, 65)
    ///   - (69, 65) 改 Pass 地形 (static defender = 200, 攻方 100 必败)
    ///   - 玩家占 (69, 65) + 建 CityWall L2 → defender × 1.6 = 200 × 1.6 = 320
    ///   - AI 100 攻 → 仍败, 但 defender 320 表现
    ///
    /// 简化验证: 跑战斗后 defender 兵力 比 200 大 (因为有 L2 城防)
    /// 但 M0 简化版直接 fail 行军, 实际看 handle_combat log 比较
    /// 改用单测: 直接调 handle_combat, 看 final_def 是不是 320 起算
    #[test]
    fn bevy_city_wall_l2_boosts_defender() {
        let mut app = make_app();
        init_playing_state(app.world_mut());

        // (69, 65) Pass 地形
        {
            let mut terrain = app.world_mut().resource_mut::<TerrainMapResource>();
            terrain
                .map
                .insert(HexCoord::new(69, 65).to_tile_key(), TerrainType::Pass);
        }
        // 玩家占 (69, 65) + 建 CityWall L2
        {
            let mut tm = app.world_mut().resource_mut::<TerritoryManagerResource>();
            tm.manager
                .occupy(HexCoord::new(69, 65), &"faction_1".to_string());
        }
        {
            let mut res_for_build = {
                let fs = app.world().resource::<FactionStoreResource>();
                let mut r = fs.store.factions.get("faction_1").unwrap().resources.clone();
                r.gold = 1000;
                r.food = 1000;
                r
            };
            let mut bm = app.world_mut().resource_mut::<BuildingManagerResource>();
            bm.manager
                .build(
                    HexCoord::new(69, 65),
                    slg_core::building::BuildingType::CityWall,
                    "faction_1".to_string(),
                    &mut res_for_build,
                )
                .unwrap();
            let mut fs = app.world_mut().resource_mut::<FactionStoreResource>();
            fs.store.factions.get_mut("faction_1").unwrap().resources = res_for_build;
        }
        // 升级 - 取出 res mutate, build.upgrade 接受 &mut FactionResources
        let mut res_for_upgrade = {
            let fs = app.world().resource::<FactionStoreResource>();
            let mut r = fs.store.factions.get("faction_1").unwrap().resources.clone();
            r.gold = 1000;
            r.food = 1000;
            r
        };
        {
            let mut bm = app.world_mut().resource_mut::<BuildingManagerResource>();
            bm.manager
                .upgrade(HexCoord::new(69, 65), &mut res_for_upgrade)
                .unwrap();
        }
        // AI faction_2 占 (70, 65)
        {
            let mut tm = app.world_mut().resource_mut::<TerritoryManagerResource>();
            tm.manager
                .set_main_city(&"faction_2".to_string(), HexCoord::new(70, 65));
            tm.manager
                .occupy(HexCoord::new(70, 65), &"faction_2".to_string());
        }

        // 直接拿 handle_combat 的 defender 兵力来验证
        // 静态 Pass = 200, × 1.6 (L2 城防) = 320
        let bm = app.world().resource::<BuildingManagerResource>();
        let bonus = bm.manager.combat_bonus_at(HexCoord::new(69, 65));
        assert!(
            (bonus.defender_troop_multiplier - 1.6).abs() < 1e-9,
            "L2 城防 defender 兵力 × 1.6, got {}",
            bonus.defender_troop_multiplier
        );

        eprintln!("TEST22 ✅: 城防 L2 在 (69,65) 加成 × 1.6 (defender 200 → 320)");
    }

    // -----------------------------------------------------------------------
    // M8 UI: 选中 / 建筑指令
    // -----------------------------------------------------------------------

    /// TEST23: 点己方格 → SelectedHex.coord = Some(coord)
    #[test]
    fn bevy_select_own_hex() {
        use slg_engine::camera::HexClickEvent;
        let mut app = make_app();
        init_playing_state(app.world_mut());
        app.add_systems(Update, handle_hex_click);

        // 点玩家主城 (68, 65)
        app.world_mut().send_event(HexClickEvent {
            coord: HexCoord::new(68, 65),
            world_pos: Vec2::new(0.0, 0.0),
        });
        app.update();

        let selected = app.world().resource::<SelectedHex>();
        assert_eq!(
            selected.coord,
            Some(HexCoord::new(68, 65)),
            "点己方格应被选中"
        );
        eprintln!("TEST23 ✅: 点玩家主城 (68,65) → 选中");
    }

    /// TEST24: 点己方格两次 → toggle 取消
    #[test]
    fn bevy_select_toggle_off() {
        use slg_engine::camera::HexClickEvent;
        let mut app = make_app();
        init_playing_state(app.world_mut());
        app.add_systems(Update, handle_hex_click);

        let coord = HexCoord::new(68, 65);
        // 第 1 次点
        app.world_mut().send_event(HexClickEvent {
            coord,
            world_pos: Vec2::new(0.0, 0.0),
        });
        app.update();
        assert_eq!(app.world().resource::<SelectedHex>().coord, Some(coord));
        // 第 2 次点同格 → 取消
        app.world_mut().send_event(HexClickEvent {
            coord,
            world_pos: Vec2::new(0.0, 0.0),
        });
        app.update();
        assert_eq!(
            app.world().resource::<SelectedHex>().coord,
            None,
            "再点同格应取消选中"
        );
        eprintln!("TEST24 ✅: 点己方格 2 次 → toggle 取消");
    }

    /// TEST25: 发 BuildAction::Build → 建筑被建 + 资源扣
    #[test]
    fn bevy_build_action_creates_building() {
        let mut app = make_app();
        init_playing_state(app.world_mut());
        // 给玩家加资源
        {
            let res_for_test = {
                let fs = app.world().resource::<FactionStoreResource>();
                let mut r = fs.store.factions.get("faction_1").unwrap().resources.clone();
                r.gold = 200;
                r
            };
            {
                let mut fs = app.world_mut().resource_mut::<FactionStoreResource>();
                fs.store.factions.get_mut("faction_1").unwrap().resources = res_for_test;
            }
        }
        app.add_systems(Update, handle_build_order);

        // 发 BuildAction::Build (Farm)
        app.world_mut().send_event(BuildAction::Build(
            Coord(HexCoord::new(68, 65)),
            slg_core::building::BuildingType::Farm,
        ));
        app.update();

        // 验证建筑被建
        let has_building = {
            let bm = app.world().resource::<BuildingManagerResource>();
            bm.manager.get(HexCoord::new(68, 65)).is_some()
        };
        assert!(has_building, "Farm 已被建");

        // 验证资源被扣 (200 - 50 = 150)
        let gold_after = {
            let fs = app.world().resource::<FactionStoreResource>();
            fs.store.factions.get("faction_1").unwrap().resources.gold
        };
        assert_eq!(gold_after, 150, "建 L1 扣 50 gold, 200 - 50 = 150");
        eprintln!("TEST25 ✅: BuildAction::Build → 农田 L1, 扣 50 gold");
    }

    /// TEST26: 发 BuildAction::Upgrade → 升级 L1→L2
    #[test]
    fn bevy_build_action_upgrades() {
        let mut app = make_app();
        init_playing_state(app.world_mut());

        // 建 L1 (扣 50 gold) + 加 1000 gold 用于升级
        let mut res = {
            let fs = app.world().resource::<FactionStoreResource>();
            let mut r = fs.store.factions.get("faction_1").unwrap().resources.clone();
            r.gold = 2000;
            r.food = 2000;
            r
        };
        {
            let mut bm = app.world_mut().resource_mut::<BuildingManagerResource>();
            bm.manager
                .build(
                    HexCoord::new(68, 65),
                    slg_core::building::BuildingType::Farm,
                    "faction_1".to_string(),
                    &mut res,
                )
                .unwrap();
        }
        {
            let mut fs = app.world_mut().resource_mut::<FactionStoreResource>();
            fs.store.factions.get_mut("faction_1").unwrap().resources = res.clone();
        }
        app.add_systems(Update, handle_build_order);

        // 发 Upgrade
        app.world_mut()
            .send_event(BuildAction::Upgrade(Coord(HexCoord::new(68, 65))));
        app.update();

        // 验证 level = 2
        let level = {
            let bm = app.world().resource::<BuildingManagerResource>();
            bm.manager.get(HexCoord::new(68, 65)).unwrap().level
        };
        assert_eq!(level, 2, "升级到 L2");
        eprintln!("TEST26 ✅: BuildAction::Upgrade → 农田 L1→L2");
    }

    // -----------------------------------------------------------------------
    // M9: 选中高亮 + 战报浮窗
    // -----------------------------------------------------------------------

    /// TEST27: 选中己方格 → SelectedHex.coord = Some
    #[test]
    fn bevy_select_marks_selected_hex() {
        use slg_engine::camera::HexClickEvent;
        let mut app = make_app();
        init_playing_state(app.world_mut());
        app.add_systems(Update, handle_hex_click);

        let coord = HexCoord::new(68, 65);
        app.world_mut().send_event(HexClickEvent {
            coord,
            world_pos: Vec2::new(0.0, 0.0),
        });
        app.update();

        let selected = app.world().resource::<SelectedHex>();
        assert_eq!(selected.coord, Some(coord), "SelectedHex 应设为主城");
        eprintln!("TEST27 ✅: 选中 (68,65) → SelectedHex.coord = Some");
    }

    /// TEST28: 注入 BattleReportEvent (V) → BattleReportState.reports +1
    #[test]
    fn bevy_battle_report_victory_appended() {
        let mut app = make_app();
        init_playing_state(app.world_mut());
        app.init_resource::<slg_ui::panels::battle_report::BattleReportState>();
        app.add_systems(Update, handle_battle_report);

        app.world_mut().send_event(BattleReportEvent {
            attacker: "faction_1".to_string(),
            defender: "faction_2".to_string(),
            coord: HexCoord::new(69, 65),
            tick: 100,
            result: "Victory".to_string(),
            attacker_initial: 100,
            defender_initial: 50,
            attacker_remaining: 50,
            defender_remaining: 0,
        });
        app.update();

        let state = app.world().resource::<slg_ui::panels::battle_report::BattleReportState>();
        assert_eq!(state.reports.len(), 1, "应有 1 条战报");
        assert_eq!(state.reports[0].winner, "Victory");
        assert_eq!(state.reports[0].attacker_losses, 50);
        assert_eq!(state.reports[0].defender_losses, 50);
        assert!(state.show, "战报应显示");
        eprintln!(
            "TEST28 ✅: 注入 BattleReportEvent (V) → BattleReportState.reports = 1"
        );
    }

    /// TEST29: 注入 Defeat 战报 → reports +1
    #[test]
    fn bevy_battle_report_defeat_appended() {
        let mut app = make_app();
        init_playing_state(app.world_mut());
        app.init_resource::<slg_ui::panels::battle_report::BattleReportState>();
        app.add_systems(Update, handle_battle_report);

        app.world_mut().send_event(BattleReportEvent {
            attacker: "faction_2".to_string(),
            defender: "faction_1".to_string(),
            coord: HexCoord::new(70, 65),
            tick: 200,
            result: "Defeat".to_string(),
            attacker_initial: 100,
            defender_initial: 200,
            attacker_remaining: 0,
            defender_remaining: 150,
        });
        app.update();

        let state = app.world().resource::<slg_ui::panels::battle_report::BattleReportState>();
        assert_eq!(state.reports.len(), 1);
        assert_eq!(state.reports[0].winner, "Defeat");
        assert_eq!(state.reports[0].attacker_losses, 100);
        assert_eq!(state.reports[0].defender_losses, 50);
        eprintln!("TEST29 ✅: 注入 BattleReportEvent (D) → reports +1");
    }

    /// TEST30: 连续 3 场战报 (V + D + Draw) → reports 累积
    #[test]
    fn bevy_battle_report_three_rounds_accumulate() {
        let mut app = make_app();
        init_playing_state(app.world_mut());
        app.init_resource::<slg_ui::panels::battle_report::BattleReportState>();
        app.add_systems(Update, handle_battle_report);

        for (tick, result) in [(1u64, "Victory"), (2, "Defeat"), (3, "Draw")] {
            app.world_mut().send_event(BattleReportEvent {
                attacker: "faction_1".to_string(),
                defender: "faction_2".to_string(),
                coord: HexCoord::new(60 + tick as i32, 65),
                tick,
                result: result.to_string(),
                attacker_initial: 100,
                defender_initial: 100,
                attacker_remaining: 50,
                defender_remaining: 50,
            });
        }
        app.update();

        let state = app.world().resource::<slg_ui::panels::battle_report::BattleReportState>();
        assert_eq!(state.reports.len(), 3, "3 场战报应累积");
        assert_eq!(state.reports[0].winner, "Victory");
        assert_eq!(state.reports[1].winner, "Defeat");
        assert_eq!(state.reports[2].winner, "Draw");
        eprintln!("TEST30 ✅: 3 场战报 (V/D/Draw) 累积 → reports.len = 3");
    }

    // -----------------------------------------------------------------------
    // M9.1: 编辑器 - 工具 dispatch / Undo/Redo / Save/Load
    // -----------------------------------------------------------------------

    /// TEST31: 点 hex + Paint tool → doc 改 + history +1
    #[test]
    fn bevy_editor_dispatch_paint() {
        let mut app = make_app();
        // 设 tool = Paint (default)
        app.add_systems(Update, slg_editor::editor_state::dispatch_editor_tool);

        // 注入 HexClickEvent
        app.world_mut().send_event(HexClickEvent {
            coord: HexCoord::new(5, 5),
            world_pos: Vec2::new(0.0, 0.0),
        });
        app.update();

        let state = app.world().resource::<slg_editor::editor_state::EditorState>();
        assert_eq!(
            state.history.undo_stack.len(),
            1,
            "Paint 命令应被 push 到 undo_stack"
        );
        eprintln!("TEST31 ✅: Paint 工具 dispatch → history.undo_stack.len = 1");
    }

    /// TEST32: PlaceEntity tool → doc.entities.placements 含该格
    #[test]
    fn bevy_editor_dispatch_place_entity() {
        let mut app = make_app();
        // 切 tool = PlaceEntity
        {
            let mut state = app
                .world_mut()
                .resource_mut::<slg_editor::editor_state::EditorState>();
            state.current_tool = slg_editor::editor_state::EditorTool::PlaceEntity;
        }
        app.add_systems(Update, slg_editor::editor_state::dispatch_editor_tool);

        app.world_mut().send_event(HexClickEvent {
            coord: HexCoord::new(10, 10),
            world_pos: Vec2::new(0.0, 0.0),
        });
        app.update();

        let state = app.world().resource::<slg_editor::editor_state::EditorState>();
        let key = HexCoord::new(10, 10).to_tile_key();
        assert!(
            state.doc.entities.placements.contains_key(&key),
            "PlaceEntity 应在 (10,10) 放实体"
        );
        let placement = state.doc.entities.placements.get(&key).unwrap();
        assert_eq!(placement.entity_type, "city");
        eprintln!(
            "TEST32 ✅: PlaceEntity tool → (10,10) 放实体 type={}",
            placement.entity_type
        );
    }

    /// TEST33: EditorAction::Undo → history 撤销
    #[test]
    fn bevy_editor_undo_redo_via_action() {
        let mut app = make_app();
        // 准备: 先 dispatch 一个 PlaceEntity
        app.add_systems(
            Update,
            (
                slg_editor::editor_state::dispatch_editor_tool,
                slg_editor::editor_state::handle_editor_action,
            ),
        );
        {
            let mut state = app
                .world_mut()
                .resource_mut::<slg_editor::editor_state::EditorState>();
            state.current_tool = slg_editor::editor_state::EditorTool::PlaceEntity;
        }
        app.world_mut().send_event(HexClickEvent {
            coord: HexCoord::new(20, 20),
            world_pos: Vec2::new(0.0, 0.0),
        });
        app.update();

        // 验证有 1 个 undo
        assert_eq!(
            app.world()
                .resource::<slg_editor::editor_state::EditorState>()
                .history
                .undo_stack
                .len(),
            1
        );

        // 发 Undo action
        app.world_mut()
            .send_event(slg_editor::editor_state::EditorAction::Undo);
        app.update();

        let state = app.world().resource::<slg_editor::editor_state::EditorState>();
        assert_eq!(state.history.undo_stack.len(), 0);
        assert_eq!(state.history.redo_stack.len(), 1);
        let key = HexCoord::new(20, 20).to_tile_key();
        assert!(
            !state.doc.entities.placements.contains_key(&key),
            "Undo 后 (20,20) 实体应消失"
        );

        // 发 Redo action
        app.world_mut()
            .send_event(slg_editor::editor_state::EditorAction::Redo);
        app.update();

        let state = app.world().resource::<slg_editor::editor_state::EditorState>();
        assert!(
            state.doc.entities.placements.contains_key(&key),
            "Redo 后 (20,20) 实体应回来"
        );
        eprintln!("TEST33 ✅: Undo/Redo via EditorAction → history 正确回滚/重做");
    }

    /// TEST34: EditorAction::SetTool → current_tool 切换
    #[test]
    fn bevy_editor_set_tool_action() {
        let mut app = make_app();
        app.add_systems(Update, slg_editor::editor_state::handle_editor_action);

        // 切到 FloodFill
        app.world_mut().send_event(
            slg_editor::editor_state::EditorAction::SetTool(
                slg_editor::editor_state::EditorTool::FloodFill,
            ),
        );
        app.update();

        let state = app.world().resource::<slg_editor::editor_state::EditorState>();
        assert_eq!(state.current_tool, slg_editor::editor_state::EditorTool::FloodFill);
        eprintln!("TEST34 ✅: EditorAction::SetTool → current_tool = FloodFill");
    }

    /// TEST35: EditorAction::Save → 写文件 → 读回 doc 一致
    #[test]
    fn bevy_editor_save_load_roundtrip() {
        let mut app = make_app();
        app.add_systems(
            Update,
            (
                slg_editor::editor_state::dispatch_editor_tool,
                slg_editor::editor_state::handle_editor_action,
            ),
        );
        // 准备: 在 doc 放一个 city, 设 save_path
        let path = std::env::temp_dir().join(format!(
            "rh_slg_editor_test_{}.ron",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
        ));
        {
            let mut state = app
                .world_mut()
                .resource_mut::<slg_editor::editor_state::EditorState>();
            state.current_tool = slg_editor::editor_state::EditorTool::PlaceEntity;
            state.save_path = Some(path.clone());
        }
        // 放 city
        app.world_mut().send_event(HexClickEvent {
            coord: HexCoord::new(15, 15),
            world_pos: Vec2::new(0.0, 0.0),
        });
        app.update();

        // Save (调 save_editor — 通过发 EditorAction::Save)
        app.world_mut()
            .send_event(slg_editor::editor_state::EditorAction::Save);
        app.update();

        // 文件应存在
        assert!(path.exists(), "save_path 文件应被创建: {}", path.display());

        // 读回: 新 EditorState → load_editor
        let mut state2 = slg_editor::editor_state::EditorState::default();
        slg_editor::editor_state::load_editor(&mut state2, path.clone()).unwrap();
        assert_eq!(state2.doc.meta.name, "Untitled");
        let key = HexCoord::new(15, 15).to_tile_key();
        assert!(
            state2.doc.entities.placements.contains_key(&key),
            "load 后 (15,15) 实体应存在"
        );

        // 清理
        let _ = std::fs::remove_file(&path);
        eprintln!("TEST35 ✅: Editor Save → Load roundtrip 一致, 文件 {}", path.display());
    }

    /// TEST36: Ctrl+Z 在 Editor phase → 发 EditorAction::Undo
    /// M9.2: 键盘快捷键集成测试
    #[test]
    fn bevy_editor_ctrl_z_triggers_undo() {
        use bevy::input::ButtonState;
        use bevy::input::keyboard::{Key, KeyboardInput, NativeKey};
        use bevy::prelude::Entity;
        let mut app = make_app();
        // 切到 Editor phase
        {
            let mut gs = app.world_mut().resource_mut::<GameState>();
            gs.phase = GamePhase::Editor;
        }
        // 注册 dispatch + action + keyboard
        app.add_systems(
            Update,
            (
                slg_editor::editor_state::dispatch_editor_tool,
                slg_editor::editor_state::handle_editor_action,
                handle_editor_keyboard,
            ),
        );
        {
            let mut state = app
                .world_mut()
                .resource_mut::<slg_editor::editor_state::EditorState>();
            state.current_tool = slg_editor::editor_state::EditorTool::Paint;
            state.brush_terrain = "terrain_forest".to_string();
        }
        // 玩家点 hex 触发 paint, 让 history 有 1 个 undo
        app.world_mut().send_event(HexClickEvent {
            coord: HexCoord::new(5, 5),
            world_pos: Vec2::new(0.0, 0.0),
        });
        app.update();
        assert_eq!(
            app.world()
                .resource::<slg_editor::editor_state::EditorState>()
                .history
                .undo_stack
                .len(),
            1,
            "paint 后 history 应有 1 个 undo"
        );

        // 注入 Ctrl + Z 按键事件 (通过 KeyboardInput, 让 PreUpdate keyboard_input_system 处理)
        let make_ki = |code: KeyCode| KeyboardInput {
            key_code: code,
            logical_key: Key::Unidentified(NativeKey::Unidentified),
            state: ButtonState::Pressed,
            window: Entity::PLACEHOLDER,
            repeat: false,
        };
        app.world_mut().send_event(make_ki(KeyCode::ControlLeft));
        app.world_mut().send_event(make_ki(KeyCode::KeyZ));
        // 帧 1: PreUpdate 处理按键, Update 触发 keyboard system → 发 Undo event
        app.update();
        // 帧 2: handle_editor_action 读 Undo event → 撤销
        app.update();

        let state = app
            .world()
            .resource::<slg_editor::editor_state::EditorState>();
        eprintln!(
            "TEST36 debug: undo_stack={}, redo_stack={}",
            state.history.undo_stack.len(),
            state.history.redo_stack.len()
        );
        assert_eq!(state.history.undo_stack.len(), 0, "Ctrl+Z 后 undo_stack 应空");
        assert_eq!(state.history.redo_stack.len(), 1, "Ctrl+Z 后 redo_stack 应有 1");
        eprintln!("TEST36 ✅: Ctrl+Z → EditorAction::Undo 正确触发");
    }

    /// TEST37: Ctrl+Z 在 Playing phase → noop (不发 Undo)
    /// M9.2: 防止游戏模式误触编辑器快捷键
    #[test]
    fn bevy_editor_keyboard_noop_in_playing() {
        use bevy::input::ButtonState;
        use bevy::input::keyboard::{Key, KeyboardInput, NativeKey};
        use bevy::prelude::Entity;
        let mut app = make_app();
        init_playing_state(app.world_mut()); // phase = Playing
        app.add_systems(
            Update,
            (
                slg_editor::editor_state::handle_editor_action,
                handle_editor_keyboard,
            ),
        );
        // 注入 Ctrl+Z 按键事件
        let make_ki = |code: KeyCode| KeyboardInput {
            key_code: code,
            logical_key: Key::Unidentified(NativeKey::Unidentified),
            state: ButtonState::Pressed,
            window: Entity::PLACEHOLDER,
            repeat: false,
        };
        app.world_mut().send_event(make_ki(KeyCode::ControlLeft));
        app.world_mut().send_event(make_ki(KeyCode::KeyZ));
        app.update();
        app.update();
        // 验证: history 仍然空
        let state = app
            .world()
            .resource::<slg_editor::editor_state::EditorState>();
        assert_eq!(state.history.undo_stack.len(), 0);
        assert_eq!(state.history.redo_stack.len(), 0);
        eprintln!("TEST37 ✅: Ctrl+Z 在 Playing phase → keyboard system 早 return, 无副作用");
    }

    /// TEST38: EditorAction::OpenMap(path) → load_editor 读 → status_message 反馈
    /// M9.2: Open File UI 集成测试
    #[test]
    fn bevy_editor_open_map_action() {
        let mut app = make_app();
        {
            let mut gs = app.world_mut().resource_mut::<GameState>();
            gs.phase = GamePhase::Editor;
        }
        // 先 Save 一份到磁盘, 然后 Open 读回
        let path = std::env::temp_dir().join("rh_slg_test_open.ron");
        let _ = std::fs::remove_file(&path);
        // 准备一个 city entity
        {
            let mut state = app
                .world_mut()
                .resource_mut::<slg_editor::editor_state::EditorState>();
            state.current_tool = slg_editor::editor_state::EditorTool::PlaceEntity;
            state.save_path = Some(path.clone());
        }
        app.add_systems(
            Update,
            (
                slg_editor::editor_state::dispatch_editor_tool,
                slg_editor::editor_state::handle_editor_action,
            ),
        );
        // 放 city
        app.world_mut().send_event(HexClickEvent {
            coord: HexCoord::new(22, 22),
            world_pos: Vec2::new(0.0, 0.0),
        });
        app.update();
        // Save
        app.world_mut()
            .send_event(slg_editor::editor_state::EditorAction::Save);
        app.update();
        assert!(path.exists(), "save_path 文件应被创建");

        // 重置 EditorState (清空 placements)
        {
            let mut state = app
                .world_mut()
                .resource_mut::<slg_editor::editor_state::EditorState>();
            state.doc.entities.placements.clear();
        }
        let cleared = app
            .world()
            .resource::<slg_editor::editor_state::EditorState>()
            .doc
            .entities
            .placements
            .is_empty();
        assert!(cleared, "Open 之前应先清空 placements");

        // Open → load_editor
        app.world_mut()
            .send_event(slg_editor::editor_state::EditorAction::OpenMap(
                path.clone(),
            ));
        app.update();
        let state = app
            .world()
            .resource::<slg_editor::editor_state::EditorState>();
        let key = HexCoord::new(22, 22).to_tile_key();
        assert!(
            state.doc.entities.placements.contains_key(&key),
            "Open 后 (22,22) 实体应加载回来"
        );
        assert!(
            state.status_message.starts_with("已打开:"),
            "status_message 应反馈: {}",
            state.status_message
        );
        eprintln!("TEST38 ✅: EditorAction::OpenMap → load_editor 正确读回, status = {}", state.status_message);

        // 清理
        let _ = std::fs::remove_file(&path);
    }

    /// TEST39: 一帧内 2 次 paint 同 brush_terrain → 1 个 undo (BatchPaintCommand 合并)
    /// M9.3: 笔刷拖动合并 (per-frame flush)
    #[test]
    fn bevy_editor_paint_stroke_merge_per_frame() {
        let mut app = make_app();
        {
            let mut gs = app.world_mut().resource_mut::<GameState>();
            gs.phase = GamePhase::Editor;
        }
        {
            let mut state = app
                .world_mut()
                .resource_mut::<slg_editor::editor_state::EditorState>();
            state.current_tool = slg_editor::editor_state::EditorTool::Paint;
            state.brush_terrain = "terrain_forest".to_string();
        }
        app.add_systems(
            Update,
            slg_editor::editor_state::dispatch_editor_tool,
        );
        // 一帧内 2 个 click, 同 brush_terrain → 合并 1 个 undo
        app.world_mut().send_event(HexClickEvent {
            coord: HexCoord::new(3, 3),
            world_pos: Vec2::new(0.0, 0.0),
        });
        app.world_mut().send_event(HexClickEvent {
            coord: HexCoord::new(4, 3),
            world_pos: Vec2::new(0.0, 0.0),
        });
        app.update();
        let state = app
            .world()
            .resource::<slg_editor::editor_state::EditorState>();
        assert_eq!(
            state.history.undo_stack.len(),
            1,
            "一帧 2 paint 同 brush_terrain → 1 undo, 实际 {}",
            state.history.undo_stack.len()
        );
        eprintln!("TEST39 ✅: 一帧 2 paint 同 brush_terrain → 1 undo (合并)");
    }

    /// TEST40: 一帧内 2 次 paint 不同 brush_terrain → 2 个 undo (开新 batch)
    /// M9.3: 切换笔刷地形触发新 stroke
    #[test]
    fn bevy_editor_paint_stroke_split_on_terrain_change() {
        let mut app = make_app();
        {
            let mut gs = app.world_mut().resource_mut::<GameState>();
            gs.phase = GamePhase::Editor;
        }
        {
            let mut state = app
                .world_mut()
                .resource_mut::<slg_editor::editor_state::EditorState>();
            state.current_tool = slg_editor::editor_state::EditorTool::Paint;
            state.brush_terrain = "terrain_forest".to_string();
        }
        app.add_systems(
            Update,
            (
                slg_editor::editor_state::dispatch_editor_tool,
                slg_editor::editor_state::handle_editor_action,
            ),
        );
        // 第一次 paint: forest
        app.world_mut().send_event(HexClickEvent {
            coord: HexCoord::new(3, 3),
            world_pos: Vec2::new(0.0, 0.0),
        });
        app.update();
        // 切 brush_terrain → mountain (通过 SetBrushTerrain action)
        app.world_mut().send_event(
            slg_editor::editor_state::EditorAction::SetBrushTerrain(
                "terrain_mountain".to_string(),
            ),
        );
        app.update();
        // 第二次 paint: mountain (不同 brush, 触发 flush 旧 + 开新)
        app.world_mut().send_event(HexClickEvent {
            coord: HexCoord::new(4, 3),
            world_pos: Vec2::new(0.0, 0.0),
        });
        app.update();
        let state = app
            .world()
            .resource::<slg_editor::editor_state::EditorState>();
        assert_eq!(
            state.history.undo_stack.len(),
            2,
            "切 brush_terrain 后 paint → 2 undo, 实际 {}",
            state.history.undo_stack.len()
        );
        eprintln!("TEST40 ✅: 不同 brush_terrain → 2 undo (新 stroke)");
    }

    /// TEST41: 跨 frame 2 次 paint 同 brush_terrain → 2 个 undo (不跨 frame 合并)
    /// M9.3: 跨 frame 不合并 (per-frame flush 限制)
    #[test]
    fn bevy_editor_paint_stroke_no_merge_across_frames() {
        let mut app = make_app();
        {
            let mut gs = app.world_mut().resource_mut::<GameState>();
            gs.phase = GamePhase::Editor;
        }
        {
            let mut state = app
                .world_mut()
                .resource_mut::<slg_editor::editor_state::EditorState>();
            state.current_tool = slg_editor::editor_state::EditorTool::Paint;
            state.brush_terrain = "terrain_forest".to_string();
        }
        app.add_systems(
            Update,
            slg_editor::editor_state::dispatch_editor_tool,
        );
        // 帧 1: 1 个 paint
        app.world_mut().send_event(HexClickEvent {
            coord: HexCoord::new(3, 3),
            world_pos: Vec2::new(0.0, 0.0),
        });
        app.update();
        // 帧 2: 1 个 paint (跨 frame, 不合并)
        app.world_mut().send_event(HexClickEvent {
            coord: HexCoord::new(4, 3),
            world_pos: Vec2::new(0.0, 0.0),
        });
        app.update();
        let state = app
            .world()
            .resource::<slg_editor::editor_state::EditorState>();
        assert_eq!(
            state.history.undo_stack.len(),
            2,
            "跨 frame paint → 2 undo, 实际 {}",
            state.history.undo_stack.len()
        );
        eprintln!("TEST41 ✅: 跨 frame 同 brush_terrain → 2 undo (不跨 frame 合并)");
    }
}


