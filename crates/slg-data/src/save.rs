//! 存档文件定义

use crate::ids::*;
use serde::{Deserialize, Serialize};

/// 存档引用的地图信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapRef {
    pub path: String,
    pub content_hash: [u8; 32], // SHA-256
}

/// 势力状态快照
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactionState {
    pub faction_id: FactionId,
    pub resources: FactionResources,
    pub diplomacy: Vec<(FactionId, i32)>, // (对方势力, 好感度)
}

/// 势力资源
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactionResources {
    pub gold: u64,
    pub food: u64,
    pub wood: u64,
    pub iron: u64,
    pub troops: u32,
}

/// 实体快照
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntitySnapshot {
    pub entity_id: u64,
    pub entity_type: String,
    pub data: Vec<u8>, // bincode 序列化的组件数据
}

/// 格子变更增量
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TileDelta {
    pub tile_key: TileKey,
    pub old_terrain: TerrainTypeId,
    pub new_terrain: TerrainTypeId,
    pub old_owner: Option<FactionId>,
    pub new_owner: Option<FactionId>,
}

/// 事件日志条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventLogEntry {
    pub tick: u64,
    pub event_id: EventId,
    pub description: String,
}

/// 存档文件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveFile {
    pub map_ref: MapRef,
    pub tick: u64,
    pub faction_states: Vec<FactionState>,
    pub entity_snapshots: Vec<EntitySnapshot>,
    pub tile_delta: Vec<TileDelta>,
    pub event_log: Vec<EventLogEntry>,
}
