//! 沙盒动态事件系统

use crate::entity::faction::FactionState;
use serde::{Deserialize, Serialize};
use slg_data::ids::{FactionId, TileKey};
use std::collections::BTreeMap;

/// 事件类别
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum EventCategory {
    Disaster,      // 天灾（旱灾/洪水/蝗灾/瘟疫）
    Rebellion,     // 叛乱
    FamousGeneral, // 名将出世
    TradeRoute,    // 商路事件
    Weather,       // 天气事件
    Diplomatic,    // 外交事件
}

/// 事件频率
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum EventFrequency {
    Low,    // 每 100~200 tick 一次
    Medium, // 每 50~100 tick 一次
    High,   // 每 20~50 tick 一次
}

/// 沙盒事件配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxEventConfig {
    pub event_frequency: EventFrequency,
    pub enabled_categories: Vec<EventCategory>,
    pub difficulty_modifier: f64,
}

impl Default for SandboxEventConfig {
    fn default() -> Self {
        Self {
            event_frequency: EventFrequency::Medium,
            enabled_categories: vec![
                EventCategory::Disaster,
                EventCategory::Rebellion,
                EventCategory::FamousGeneral,
                EventCategory::TradeRoute,
            ],
            difficulty_modifier: 1.0,
        }
    }
}

/// 沙盒事件调度器
#[derive(Debug)]
pub struct SandboxEventScheduler {
    pub config: SandboxEventConfig,
    pub next_event_tick: u64,
    pub cooldowns: BTreeMap<EventCategory, u64>,
    pub event_count: u64,
}

impl SandboxEventScheduler {
    pub fn new(config: SandboxEventConfig) -> Self {
        Self {
            config,
            next_event_tick: 50, // 第一个事件在 50 tick 后
            cooldowns: BTreeMap::new(),
            event_count: 0,
        }
    }

    /// 每 tick 检查是否触发事件
    pub fn tick(
        &mut self,
        current_tick: u64,
        factions: &BTreeMap<FactionId, FactionState>,
        tile_owners: &BTreeMap<TileKey, FactionId>,
    ) -> Option<SandboxEvent> {
        if current_tick < self.next_event_tick {
            return None;
        }

        // 选择事件类别
        let category = self.select_category(current_tick);

        // 生成事件
        let event = self.generate_event(category, current_tick, factions, tile_owners);

        // 设置下一个事件时间
        self.next_event_tick = current_tick + self.get_interval();
        self.cooldowns.insert(category, current_tick);
        self.event_count += 1;

        event
    }

    /// 选择事件类别
    fn select_category(&self, current_tick: u64) -> EventCategory {
        // 简化实现：轮询启用的类别
        let enabled: Vec<_> = self
            .config
            .enabled_categories
            .iter()
            .filter(|c| {
                let last = self.cooldowns.get(c).copied().unwrap_or(0);
                current_tick - last >= 30 // 冷却 30 tick
            })
            .collect();

        if enabled.is_empty() {
            return EventCategory::Disaster;
        }

        let index = (self.event_count as usize) % enabled.len();
        *enabled[index]
    }

    /// 获取事件间隔
    fn get_interval(&self) -> u64 {
        match self.config.event_frequency {
            EventFrequency::Low => 150,
            EventFrequency::Medium => 75,
            EventFrequency::High => 35,
        }
    }

    /// 生成事件
    fn generate_event(
        &self,
        category: EventCategory,
        current_tick: u64,
        _factions: &BTreeMap<FactionId, FactionState>,
        _tile_owners: &BTreeMap<TileKey, FactionId>,
    ) -> Option<SandboxEvent> {
        match category {
            EventCategory::Disaster => Some(SandboxEvent {
                id: format!("disaster_{}", self.event_count),
                category,
                name: "天灾降临".to_string(),
                description: "旱灾来袭，粮食产出降低 50%，持续 20 tick".to_string(),
                effects: vec![SandboxEffect::ResourceModifier {
                    resource: "food".to_string(),
                    factor: 0.5,
                    duration_ticks: 20,
                }],
                trigger_tick: current_tick,
            }),

            EventCategory::Rebellion => Some(SandboxEvent {
                id: format!("rebellion_{}", self.event_count),
                category,
                name: "叛乱爆发".to_string(),
                description: "领地爆发叛乱，出现敌对部队".to_string(),
                effects: vec![SandboxEffect::SpawnRebels {
                    count: 100,
                    duration_ticks: 10,
                }],
                trigger_tick: current_tick,
            }),

            EventCategory::FamousGeneral => Some(SandboxEvent {
                id: format!("general_{}", self.event_count),
                category,
                name: "名将出世".to_string(),
                description: "一位名将出现在城池中，可招募".to_string(),
                effects: vec![SandboxEffect::SpawnGeneral {
                    general_id: format!("general_random_{}", self.event_count),
                }],
                trigger_tick: current_tick,
            }),

            EventCategory::TradeRoute => Some(SandboxEvent {
                id: format!("trade_{}", self.event_count),
                category,
                name: "商路畅通".to_string(),
                description: "商路畅通，金币产出增加 30%，持续 30 tick".to_string(),
                effects: vec![SandboxEffect::ResourceModifier {
                    resource: "gold".to_string(),
                    factor: 1.3,
                    duration_ticks: 30,
                }],
                trigger_tick: current_tick,
            }),

            _ => None,
        }
    }
}

/// 沙盒事件
#[derive(Debug, Clone)]
pub struct SandboxEvent {
    pub id: String,
    pub category: EventCategory,
    pub name: String,
    pub description: String,
    pub effects: Vec<SandboxEffect>,
    pub trigger_tick: u64,
}

/// 沙盒效果
#[derive(Debug, Clone)]
pub enum SandboxEffect {
    /// 资源产出修正
    ResourceModifier {
        resource: String,
        factor: f64,
        duration_ticks: u64,
    },
    /// 生成叛军
    SpawnRebels { count: u32, duration_ticks: u64 },
    /// 生成武将
    SpawnGeneral { general_id: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scheduler_triggers_event() {
        let config = SandboxEventConfig::default();
        let mut scheduler = SandboxEventScheduler::new(config);
        let factions = BTreeMap::new();
        let tile_owners = BTreeMap::new();

        let event = scheduler.tick(50, &factions, &tile_owners);
        assert!(event.is_some());
    }

    #[test]
    fn test_scheduler_respects_cooldown() {
        let config = SandboxEventConfig::default();
        let mut scheduler = SandboxEventScheduler::new(config);
        let factions = BTreeMap::new();
        let tile_owners = BTreeMap::new();

        // 第一个事件
        scheduler.tick(50, &factions, &tile_owners);

        // 立即再次触发（应该返回 None，因为 next_event_tick 已更新）
        let event = scheduler.tick(51, &factions, &tile_owners);
        assert!(event.is_none());
    }

    #[test]
    fn test_event_has_effects() {
        let config = SandboxEventConfig::default();
        let mut scheduler = SandboxEventScheduler::new(config);
        let factions = BTreeMap::new();
        let tile_owners = BTreeMap::new();

        let event = scheduler.tick(50, &factions, &tile_owners).unwrap();
        assert!(!event.effects.is_empty());
    }

    #[test]
    fn test_different_categories() {
        let config = SandboxEventConfig::default();
        let mut scheduler = SandboxEventScheduler::new(config);
        let factions = BTreeMap::new();
        let tile_owners = BTreeMap::new();

        let mut categories = Vec::new();
        for tick in [50, 125, 200, 275] {
            if let Some(event) = scheduler.tick(tick, &factions, &tile_owners) {
                categories.push(event.category);
            }
        }

        // 应该有不同的事件类别
        assert!(categories.len() >= 2);
    }
}
