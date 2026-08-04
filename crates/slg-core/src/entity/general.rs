//! 武将相关组件
//!
//! 每个武将对应一个 ECS Entity，挂载以下组件。

use serde::{Deserialize, Serialize};
use slg_data::ids::{FactionId, SkillId, UnitTypeId};

/// 武将运行时属性（含等级与经验，区别于配置表的 GeneralStats）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralStats {
    pub strength: u8,
    pub intelligence: u8,
    pub command: u8,
    pub politics: u8,
    pub charisma: u8,
    pub level: u16,
    pub exp: u32,
}

/// 武将已习得战法列表
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralSkills {
    pub skills: Vec<SkillId>,
}

/// 武将当前适配兵种
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralTroopType {
    pub unit_type: UnitTypeId,
}

/// 所属势力标记（武将、部队、城池共用）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OwnerFaction {
    pub faction: FactionId,
}
