//! 势力相关组件与状态
//!
//! 势力不对应单个 ECS Entity，而是以 FactionStore Resource 存储所有势力状态。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use slg_data::ids::{FactionId, UnitTypeId};

use crate::entity::general::GeneralStats;
use crate::map::grid::HexCoord;

/// 单个势力的完整状态
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FactionState {
    pub resources: FactionResources,
    pub personality: FactionPersonality,
    pub main_city: Option<HexCoord>,
    /// 与其他势力的关系值（正=友好，负=敌对）
    pub diplomacy: BTreeMap<FactionId, i32>,
    /// 势力麾下武将列表 (M7)
    ///
    /// M7 简化版：直接存 `GeneralStats`（没独立 GeneralId 映射 / 没战法），M8 再升级。
    /// 列表中第一个是"主将"（出征 / 驻防都用他）。
    pub generals: Vec<GeneralStats>,
}

impl FactionState {
    /// 获取主将（第一个武将）
    ///
    /// M7 简化：没有指定主将时返回 `None`（调用方应回退到无武将战斗）。
    pub fn primary_general(&self) -> Option<&GeneralStats> {
        self.generals.first()
    }

    /// 默认兵种 (M7 简化版)
    ///
    /// M7 没有"武将带兵种适配"概念，所有兵都是步兵 (`unit_infantry`)。
    /// M8 引入 `GeneralTroopType` 后改成查主将的 `unit_type`。
    pub fn default_unit_type() -> UnitTypeId {
        "unit_infantry".to_string()
    }
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
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FactionPersonality {
    pub aggression: f64,
    pub expansion: f64,
    pub diplomacy: f64,
    pub caution: f64,
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_general(strength: u8) -> GeneralStats {
        GeneralStats {
            strength,
            intelligence: 50,
            command: 50,
            politics: 50,
            charisma: 50,
            level: 1,
            exp: 0,
        }
    }

    #[test]
    fn test_primary_general_returns_first() {
        let state = FactionState {
            resources: FactionResources::default(),
            personality: FactionPersonality {
                aggression: 0.5,
                expansion: 0.5,
                diplomacy: 0.5,
                caution: 0.5,
            },
            main_city: None,
            diplomacy: BTreeMap::new(),
            generals: vec![make_general(80), make_general(60), make_general(95)],
        };
        let primary = state.primary_general().unwrap();
        assert_eq!(primary.strength, 80, "primary 应是第一个");
    }

    #[test]
    fn test_primary_general_empty_returns_none() {
        let state = FactionState {
            resources: FactionResources::default(),
            personality: FactionPersonality {
                aggression: 0.5,
                expansion: 0.5,
                diplomacy: 0.5,
                caution: 0.5,
            },
            main_city: None,
            diplomacy: BTreeMap::new(),
            generals: vec![],
        };
        assert!(state.primary_general().is_none(), "没武将返回 None");
    }

    #[test]
    fn test_default_unit_type_infantry() {
        assert_eq!(FactionState::default_unit_type(), "unit_infantry");
    }
}
