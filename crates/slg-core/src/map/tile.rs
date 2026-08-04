//! 地图格子数据定义

use serde::{Deserialize, Serialize};

/// 单格数据（运行时形态，存于 Chunk Component 中）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TileData {
    pub terrain: TerrainType,
    pub owner: u8, // 0 = 无主，1~255 = 势力编号
    pub level: u8, // 1~9，土地等级
    pub resource: Option<ResourceType>,
}

/// 地形类型枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TerrainType {
    Plains,   // 平原
    Mountain, // 山地
    Water,    // 水域
    Forest,   // 森林
    Desert,   // 沙漠
    Swamp,    // 沼泽
    Hills,    // 丘陵
    Pass,     // 关隘
}

impl TerrainType {
    /// 从 u8 转换（用于 Chunk 数组存储）
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Plains),
            1 => Some(Self::Mountain),
            2 => Some(Self::Water),
            3 => Some(Self::Forest),
            4 => Some(Self::Desert),
            5 => Some(Self::Swamp),
            6 => Some(Self::Hills),
            7 => Some(Self::Pass),
            _ => None,
        }
    }

    /// 转为 u8
    pub fn to_u8(self) -> u8 {
        match self {
            Self::Plains => 0,
            Self::Mountain => 1,
            Self::Water => 2,
            Self::Forest => 3,
            Self::Desert => 4,
            Self::Swamp => 5,
            Self::Hills => 6,
            Self::Pass => 7,
        }
    }
}

/// 资源类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResourceType {
    Gold,  // 金币
    Food,  // 粮食
    Wood,  // 木材
    Iron,  // 铁矿
    Stone, // 石料
}

impl Default for TileData {
    fn default() -> Self {
        Self {
            terrain: TerrainType::Plains,
            owner: 0,
            level: 1,
            resource: None,
        }
    }
}
