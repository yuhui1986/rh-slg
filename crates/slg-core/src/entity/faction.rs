//! 势力相关组件与状态
//!
//! 势力不对应单个 ECS Entity，而是以 FactionStore Resource 存储所有势力状态。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use slg_data::ids::FactionId;

use crate::map::grid::HexCoord;

/// 单个势力的完整状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactionState {
    pub resources: FactionResources,
    pub personality: FactionPersonality,
    pub main_city: Option<HexCoord>,
    /// 与其他势力的关系值（正=友好，负=敌对）
    pub diplomacy: BTreeMap<FactionId, i32>,
}

/// 势力资源
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FactionResources {
    pub gold: u64,
    pub food: u64,
    pub wood: u64,
    pub iron: u64,
    pub stone: u64,
    pub troops: u32,
}

/// AI 性格参数（各维度 0.0~1.0）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactionPersonality {
    pub aggression: f64,
    pub expansion: f64,
    pub diplomacy: f64,
    pub caution: f64,
}
