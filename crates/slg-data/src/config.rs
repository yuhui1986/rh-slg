//! 配置表结构定义（RON 数据表对应）

use crate::ids::*;
use serde::{Deserialize, Serialize};

/// 武将稀有度
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Rarity {
    S,
    A,
    B,
    C,
}

/// 武将定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralDef {
    pub id: GeneralId,
    pub name: String,
    pub rarity: Rarity,
    pub base_stats: GeneralStats,
    pub growth_stats: GeneralStats,
    pub innate_skill: SkillId,
    pub learnable_skills: Vec<SkillId>,
    pub unit_types: Vec<UnitTypeId>,
}

/// 武将五维
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralStats {
    pub strength: u8,     // 武力
    pub intelligence: u8, // 智力
    pub command: u8,      // 统率
    pub politics: u8,     // 政治
    pub charisma: u8,     // 魅力
}

/// 战法类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SkillType {
    Active,  // 主动
    Passive, // 被动
    Command, // 指挥
    Rush,    // 突击
}

/// 伤害公式
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DamageFormula {
    /// 固定伤害
    Fixed { base: f64 },
    /// 武力加成
    Physical { base: f64, str_ratio: f64 },
    /// 智力加成
    Magical { base: f64, int_ratio: f64 },
    /// 百分比
    Percentage { ratio: f64 },
}

/// 战法定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillDef {
    pub id: SkillId,
    pub name: String,
    pub skill_type: SkillType,
    pub trigger_rate: f64,       // 发动概率 0.0~1.0
    pub target_strategy: String, // "random_enemy", "lowest_hp", "all_enemies" 等
    pub damage: DamageFormula,
    pub effects: Vec<SkillEffect>,
    pub source_general: Option<GeneralId>,
}

/// 战法效果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillEffect {
    pub effect_type: String, // "damage", "heal", "buff", "debuff"
    pub value: f64,
    pub duration: u32, // 回合数，0=即时
}

/// 兵种定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnitTypeDef {
    pub id: UnitTypeId,
    pub name: String,
    pub attack: u16,
    pub defense: u16,
    pub hp: u16,
    pub speed: u16,
    pub recruit_cost: u32,
    pub counter_target: UnitTypeId,                    // 克制目标
    pub terrain_adaptation: Vec<(TerrainTypeId, f64)>, // 地形适性
}

/// 地形类型定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerrainTypeDef {
    pub id: TerrainTypeId,
    pub name: String,
    pub movement_cost: f64,
    pub defense_bonus: f64,
    pub passable: bool,
    pub buildable: bool,
}

/// 建筑定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildingDef {
    pub id: BuildingId,
    pub name: String,
    pub category: String,
    pub levels: Vec<BuildingLevel>,
    pub terrain_req: Vec<TerrainTypeId>,
}

/// 建筑等级
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildingLevel {
    pub cost_resources: u32,
    pub build_time_ticks: u32,
    pub effect: String,
}

/// 事件定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventDef {
    pub id: EventId,
    pub name: String,
    pub trigger: String,
    pub effect: String,
    pub script_hook: Option<String>,
}

/// 全局参数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalParams {
    pub economy: EconomyParams,
    pub military: MilitaryParams,
    pub map: MapParams,
    pub diplomacy: DiplomacyParams,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EconomyParams {
    pub resource_multiplier: f64,
    pub build_cost_multiplier: f64,
    pub recruit_cost_multiplier: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MilitaryParams {
    pub combat_damage_multiplier: f64,
    pub march_speed_multiplier: f64,
    pub exp_gain_multiplier: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapParams {
    pub tile_level_range: (u8, u8),
    pub resource_density: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiplomacyParams {
    pub relation_decay_per_tick: f64,
    pub alliance_threshold: i32,
}
