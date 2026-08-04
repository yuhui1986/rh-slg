//! 部队相关组件
//!
//! 每支部队对应一个 ECS Entity，挂载 ArmyTroops / ArmyMarch / ArmyPosition / OwnerFaction。

use serde::{Deserialize, Serialize};
use slg_data::ids::UnitTypeId;

use crate::map::grid::HexCoord;

/// 部队兵力信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArmyTroops {
    pub unit_type: UnitTypeId,
    pub count: u32,
    /// 士气 0.0~100.0
    pub morale: f64,
}

/// 行军状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArmyMarch {
    /// 完整路径（含起点与终点）
    pub path: Vec<HexCoord>,
    /// 当前已到达的路径节点索引
    pub path_index: usize,
    /// 预计到达的 tick
    pub arrive_tick: u64,
}

/// 部队当前所在格子
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArmyPosition {
    pub coord: HexCoord,
}
