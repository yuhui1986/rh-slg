//! 城池相关组件
//!
//! 每座城池对应一个 ECS Entity，挂载 CityLevel / CityGarrison / CityBuildQueue / Position / OwnerFaction。

use serde::{Deserialize, Serialize};
use slg_data::ids::{BuildingId, UnitTypeId};

use crate::map::grid::HexCoord;

/// 城池等级（1~10）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CityLevel {
    pub level: u8,
}

/// 城池驻军
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CityGarrison {
    pub troops: Vec<(UnitTypeId, u32)>,
}

/// 城池建造队列
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CityBuildQueue {
    pub queue: Vec<BuildEntry>,
}

/// 单条建造条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildEntry {
    pub building_id: BuildingId,
    pub start_tick: u64,
    pub end_tick: u64,
}

/// 城池坐标
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub coord: HexCoord,
}
