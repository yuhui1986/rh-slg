//! 地图文档定义（编辑器/磁盘形态）

use crate::ids::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// 地图元信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapMeta {
    pub name: String,
    pub seed: u64,
    pub width: u32,
    pub height: u32,
    pub preset_name: Option<String>,
}

/// 地形层（RLE 密集数组，100% 填充）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerrainLayer {
    /// RLE 编码：(terrain_type_id, 连续次数)
    pub rle_data: Vec<(TerrainTypeId, u32)>,
    /// 解压后的总格数（用于校验）
    pub total_tiles: u32,
}

/// 资源层（稀疏，<5% 填充）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLayer {
    pub entries: BTreeMap<TileKey, ResourceEntry>,
}

/// 资源条目
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResourceEntry {
    pub resource_type: String,
    pub level: u8,
}

/// 实体层（稀疏，城池/要塞等）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityLayer {
    pub placements: BTreeMap<TileKey, EntityPlacement>,
}

/// 实体放置
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntityPlacement {
    pub entity_type: String, // "city", "fortress", "pass" 等
    pub faction_id: Option<FactionId>,
    pub properties: BTreeMap<String, String>,
}

/// 规则层
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleLayer {
    pub zones: Vec<ZoneRule>,
    pub triggers: Vec<TriggerRule>,
}

/// 区域规则
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZoneRule {
    pub name: String,
    pub tiles: Vec<TileKey>,
    pub rule_type: String,
}

/// 触发规则
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerRule {
    pub event_id: EventId,
    pub condition: String,
    pub effect: String,
}

/// 地图文档（编辑器/磁盘形态）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapDocument {
    pub meta: MapMeta,
    pub terrain: TerrainLayer,
    pub resources: ResourceLayer,
    pub entities: EntityLayer,
    pub rules: RuleLayer,
    /// 河流层（稀疏，用于存储河流数据）
    #[serde(default)]
    pub rivers: RiverLayer,
}

/// 河流层（稀疏存储）
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RiverLayer {
    pub segments: BTreeMap<TileKey, RiverSegment>,
}

/// 河流段数据
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RiverSegment {
    /// 河流宽度：1/2/3 格
    pub width: u8,
    /// 是否是渡口
    pub is_ford: bool,
    /// 流向（可选）
    pub direction: Option<FlowDirection>,
}

/// 河流流向
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FlowDirection {
    East,
    Southeast,
    Southwest,
    West,
    Northwest,
    Northeast,
}
