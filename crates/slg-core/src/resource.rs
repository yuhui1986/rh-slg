//! 全局 Resource 数据结构
//!
//! 这些结构本身是纯数据类型，由 slg-engine 桥接到 Bevy Resource。
//! 包含游戏时钟、命令队列、迷雾、寻路缓存、战报、AI 决策槽、领地图。

use std::collections::{BTreeMap, VecDeque};

use lru::LruCache;
use serde::{Deserialize, Serialize};
use slg_data::ids::{BuildingId, FactionId, TileKey, UnitTypeId};

use crate::ai::diplomacy::DiplomacyAction;
use crate::entity::faction::FactionState;
use crate::map::grid::HexCoord;
use crate::rule::combat::CombatReport;
use crate::rule::movement::MarchRequest;

// ---------------------------------------------------------------------------
// 时钟
// ---------------------------------------------------------------------------

/// 游戏时钟
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameClock {
    pub current_tick: u64,
    pub speed: Speed,
    pub accumulator: f64,
}

/// 游戏速度
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Speed {
    Paused,
    #[default]
    X1,
    X2,
    X3,
}

impl Speed {
    /// 返回速度对应的倍率（Paused = 0.0）
    pub fn multiplier(&self) -> f64 {
        match self {
            Speed::Paused => 0.0,
            Speed::X1 => 1.0,
            Speed::X2 => 2.0,
            Speed::X3 => 3.0,
        }
    }
}

impl Default for GameClock {
    fn default() -> Self {
        Self {
            current_tick: 0,
            speed: Speed::default(),
            accumulator: 0.0,
        }
    }
}

// ---------------------------------------------------------------------------
// 势力存储
// ---------------------------------------------------------------------------

/// 所有势力状态集合
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactionStore {
    pub factions: BTreeMap<FactionId, FactionState>,
}

// ---------------------------------------------------------------------------
// 命令队列
// ---------------------------------------------------------------------------

/// 玩家命令队列
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CommandQueue {
    pub commands: VecDeque<PlayerCommand>,
}

/// 玩家可执行的命令
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PlayerCommand {
    MoveArmy(MarchRequest),
    OccupyTile(HexCoord, FactionId),
    BuildBuilding(HexCoord, BuildingId),
    RecruitTroops(HexCoord, UnitTypeId, u32),
    DiplomacyAction(DiplomacyAction),
}

// ---------------------------------------------------------------------------
// 寻路缓存
// ---------------------------------------------------------------------------

/// 寻路结果缓存（LRU 淘汰）
///
/// key: (起点 TileKey, 终点 TileKey, 移动代价预算)
/// value: 路径坐标序列
#[derive(Debug, Clone)]
pub struct PathCache {
    pub entries: LruCache<(TileKey, TileKey, u32), Vec<HexCoord>>,
}

// ---------------------------------------------------------------------------
// 战报存储
// ---------------------------------------------------------------------------

/// 战报存储
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CombatReportStore {
    pub reports: Vec<CombatReport>,
}

// ---------------------------------------------------------------------------
// 迷雾
// ---------------------------------------------------------------------------

/// 迷雾数据（按 chunk 存储，每 chunk 32x32 = 1024 格）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FogOfWar {
    pub chunks: Vec<FogChunk>,
}

/// 单个迷雾 chunk（32x32 = 1024 格）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FogChunk {
    pub data: Vec<u8>,
}

impl FogChunk {
    /// 创建全黑（未探索）的迷雾 chunk
    pub fn new_fogged() -> Self {
        Self {
            data: vec![0u8; 1024],
        }
    }

    /// 创建全亮（已探索）的迷雾 chunk
    pub fn new_visible() -> Self {
        Self {
            data: vec![0xFFu8; 1024],
        }
    }
}

// ---------------------------------------------------------------------------
// AI 决策槽
// ---------------------------------------------------------------------------

/// AI 决策槽位分配（最多 10 个势力）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AISlotAssignments {
    pub slots: [FactionId; 10],
}

// ---------------------------------------------------------------------------
// 领地
// ---------------------------------------------------------------------------

/// 领地归属图
///
/// owner_map 记录每个 TileKey 所属势力。
/// 完整的 Union-Find 实现在 T08 中完成。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerritoryGraph {
    pub owner_map: BTreeMap<TileKey, FactionId>,
}
