//! 事件触发条件

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use slg_data::ids::{FactionId, TileKey};

use crate::entity::faction::FactionState;
use crate::map::grid::HexCoord;

/// 触发条件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TriggerCondition {
    /// 游戏时间到达
    TimeReached { tick: u64 },
    /// 势力资源低于阈值
    FactionResourceLow {
        faction: FactionId,
        resource: String,
        threshold: u64,
    },
    /// 势力资源高于阈值
    FactionResourceHigh {
        faction: FactionId,
        resource: String,
        threshold: u64,
    },
    /// 特定格子被占领
    TileOccupied { coord: HexCoord, by: FactionId },
    /// 势力覆灭（无领地）
    FactionEliminated { faction: FactionId },
    /// 随机触发（带冷却）
    RandomChance {
        probability: f64,
        cooldown_ticks: u64,
    },
    /// 逻辑与
    And(Vec<TriggerCondition>),
    /// 逻辑或
    Or(Vec<TriggerCondition>),
    /// 逻辑非
    Not(Box<TriggerCondition>),
}

/// 条件评估上下文
#[derive(Debug, Clone)]
pub struct EvalContext {
    pub current_tick: u64,
    pub factions: BTreeMap<FactionId, FactionState>,
    pub tile_owners: BTreeMap<TileKey, FactionId>,
    /// 各事件链最近一次触发的 tick
    pub last_triggered: BTreeMap<String, u64>,
}

impl TriggerCondition {
    /// 评估条件是否满足
    pub fn evaluate(&self, ctx: &EvalContext, chain_id: &str, rng: &mut impl rand::Rng) -> bool {
        match self {
            TriggerCondition::TimeReached { tick } => ctx.current_tick >= *tick,

            TriggerCondition::FactionResourceLow {
                faction,
                resource,
                threshold,
            } => {
                if let Some(f) = ctx.factions.get(faction) {
                    Self::read_resource(&f.resources, resource) < *threshold
                } else {
                    false
                }
            }

            TriggerCondition::FactionResourceHigh {
                faction,
                resource,
                threshold,
            } => {
                if let Some(f) = ctx.factions.get(faction) {
                    Self::read_resource(&f.resources, resource) > *threshold
                } else {
                    false
                }
            }

            TriggerCondition::TileOccupied { coord, by } => {
                let key = coord.to_tile_key();
                ctx.tile_owners.get(&key) == Some(by)
            }

            TriggerCondition::FactionEliminated { faction } => {
                !ctx.tile_owners.values().any(|f| f == faction)
            }

            TriggerCondition::RandomChance {
                probability,
                cooldown_ticks,
            } => {
                let key = format!("{}_random", chain_id);
                let last = ctx.last_triggered.get(&key).copied().unwrap_or(0);
                if ctx.current_tick.saturating_sub(last) < *cooldown_ticks {
                    return false;
                }
                rng.gen::<f64>() < *probability
            }

            TriggerCondition::And(conditions) => {
                conditions.iter().all(|c| c.evaluate(ctx, chain_id, rng))
            }

            TriggerCondition::Or(conditions) => {
                conditions.iter().any(|c| c.evaluate(ctx, chain_id, rng))
            }

            TriggerCondition::Not(condition) => !condition.evaluate(ctx, chain_id, rng),
        }
    }

    /// 从 FactionResources 读取指定资源值
    fn read_resource(res: &crate::entity::faction::FactionResources, name: &str) -> u64 {
        match name {
            "gold" => res.gold,
            "food" => res.food,
            "wood" => res.wood,
            "iron" => res.iron,
            "stone" => res.stone,
            "troops" => res.troops as u64,
            _ => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::faction::{FactionPersonality, FactionResources};
    use rand::SeedableRng;
    use rand_chacha::ChaCha12Rng;

    fn make_ctx(tick: u64) -> EvalContext {
        EvalContext {
            current_tick: tick,
            factions: BTreeMap::new(),
            tile_owners: BTreeMap::new(),
            last_triggered: BTreeMap::new(),
        }
    }

    fn make_faction(gold: u64, food: u64) -> FactionState {
        FactionState {
            resources: FactionResources {
                gold,
                food,
                wood: 100,
                iron: 100,
                stone: 100,
                troops: 50,
            },
            personality: FactionPersonality {
                aggression: 0.5,
                expansion: 0.5,
                diplomacy: 0.5,
                caution: 0.5,
            },
            main_city: None,
            diplomacy: BTreeMap::new(),
        }
    }

    #[test]
    fn test_time_reached() {
        let cond = TriggerCondition::TimeReached { tick: 10 };
        let mut rng = ChaCha12Rng::seed_from_u64(42);
        assert!(cond.evaluate(&make_ctx(10), "c", &mut rng));
        assert!(cond.evaluate(&make_ctx(15), "c", &mut rng));
        assert!(!cond.evaluate(&make_ctx(9), "c", &mut rng));
    }

    #[test]
    fn test_faction_resource_low() {
        let cond = TriggerCondition::FactionResourceLow {
            faction: "f1".into(),
            resource: "gold".into(),
            threshold: 100,
        };
        let mut rng = ChaCha12Rng::seed_from_u64(42);

        // 无势力 -> false
        assert!(!cond.evaluate(&make_ctx(0), "c", &mut rng));

        // 资源低于阈值 -> true
        let mut ctx = make_ctx(0);
        ctx.factions.insert("f1".into(), make_faction(50, 200));
        assert!(cond.evaluate(&ctx, "c", &mut rng));

        // 资源高于阈值 -> false
        let mut ctx2 = make_ctx(0);
        ctx2.factions.insert("f1".into(), make_faction(150, 200));
        assert!(!cond.evaluate(&ctx2, "c", &mut rng));
    }

    #[test]
    fn test_faction_resource_high() {
        let cond = TriggerCondition::FactionResourceHigh {
            faction: "f1".into(),
            resource: "food".into(),
            threshold: 500,
        };
        let mut rng = ChaCha12Rng::seed_from_u64(42);

        let mut ctx = make_ctx(0);
        ctx.factions.insert("f1".into(), make_faction(100, 600));
        assert!(cond.evaluate(&ctx, "c", &mut rng));

        let mut ctx2 = make_ctx(0);
        ctx2.factions.insert("f1".into(), make_faction(100, 400));
        assert!(!cond.evaluate(&ctx2, "c", &mut rng));
    }

    #[test]
    fn test_tile_occupied() {
        let coord = HexCoord::new(3, 5);
        let cond = TriggerCondition::TileOccupied {
            coord,
            by: "f1".into(),
        };
        let mut rng = ChaCha12Rng::seed_from_u64(42);

        let mut ctx = make_ctx(0);
        ctx.tile_owners.insert(coord.to_tile_key(), "f1".into());
        assert!(cond.evaluate(&ctx, "c", &mut rng));

        let mut ctx2 = make_ctx(0);
        ctx2.tile_owners.insert(coord.to_tile_key(), "f2".into());
        assert!(!cond.evaluate(&ctx2, "c", &mut rng));
    }

    #[test]
    fn test_faction_eliminated() {
        let cond = TriggerCondition::FactionEliminated {
            faction: "f1".into(),
        };
        let mut rng = ChaCha12Rng::seed_from_u64(42);

        // 无领地 -> 被消灭
        let ctx = make_ctx(0);
        assert!(cond.evaluate(&ctx, "c", &mut rng));

        // 有领地 -> 未被消灭
        let mut ctx2 = make_ctx(0);
        ctx2.tile_owners
            .insert(HexCoord::new(0, 0).to_tile_key(), "f1".into());
        assert!(!cond.evaluate(&ctx2, "c", &mut rng));
    }

    #[test]
    fn test_random_chance_cooldown() {
        let cond = TriggerCondition::RandomChance {
            probability: 1.0, // 100% 概率
            cooldown_ticks: 10,
        };

        // 从未触发 -> 可以触发
        let ctx = make_ctx(20);
        let mut rng = ChaCha12Rng::seed_from_u64(42);
        assert!(cond.evaluate(&ctx, "c", &mut rng));

        // 冷却中 -> 不触发
        let mut ctx2 = make_ctx(20);
        ctx2.last_triggered.insert("c_random".into(), 15);
        let mut rng2 = ChaCha12Rng::seed_from_u64(42);
        assert!(!cond.evaluate(&ctx2, "c", &mut rng2));

        // 冷却结束 -> 触发
        let mut ctx3 = make_ctx(30);
        ctx3.last_triggered.insert("c_random".into(), 15);
        let mut rng3 = ChaCha12Rng::seed_from_u64(42);
        assert!(cond.evaluate(&ctx3, "c", &mut rng3));
    }

    #[test]
    fn test_random_chance_deterministic() {
        let cond = TriggerCondition::RandomChance {
            probability: 0.5,
            cooldown_ticks: 0,
        };
        let ctx = make_ctx(0);
        // 同种子 -> 同结果
        let mut r1 = ChaCha12Rng::seed_from_u64(99);
        let mut r2 = ChaCha12Rng::seed_from_u64(99);
        let a = cond.evaluate(&ctx, "c", &mut r1);
        let b = cond.evaluate(&ctx, "c", &mut r2);
        assert_eq!(a, b);
    }

    #[test]
    fn test_and_trigger() {
        let cond = TriggerCondition::And(vec![
            TriggerCondition::TimeReached { tick: 10 },
            TriggerCondition::FactionResourceLow {
                faction: "f1".into(),
                resource: "gold".into(),
                threshold: 100,
            },
        ]);
        let mut rng = ChaCha12Rng::seed_from_u64(42);

        // 时间到但资源不低 -> false
        let mut ctx = make_ctx(10);
        ctx.factions.insert("f1".into(), make_faction(200, 0));
        assert!(!cond.evaluate(&ctx, "c", &mut rng));

        // 时间到且资源低 -> true
        let mut ctx2 = make_ctx(10);
        ctx2.factions.insert("f1".into(), make_faction(50, 0));
        assert!(cond.evaluate(&ctx2, "c", &mut rng));
    }

    #[test]
    fn test_or_trigger() {
        let cond = TriggerCondition::Or(vec![
            TriggerCondition::TimeReached { tick: 10 },
            TriggerCondition::FactionEliminated {
                faction: "f1".into(),
            },
        ]);
        let mut rng = ChaCha12Rng::seed_from_u64(42);

        // 时间未到且势力未灭 -> false
        let mut ctx = make_ctx(5);
        ctx.tile_owners
            .insert(HexCoord::new(0, 0).to_tile_key(), "f1".into());
        assert!(!cond.evaluate(&ctx, "c", &mut rng));

        // 时间到 -> true
        assert!(cond.evaluate(&make_ctx(10), "c", &mut rng));

        // 势力灭 -> true
        let ctx2 = make_ctx(5);
        assert!(cond.evaluate(&ctx2, "c", &mut rng));
    }

    #[test]
    fn test_not_trigger() {
        let cond = TriggerCondition::Not(Box::new(TriggerCondition::TimeReached { tick: 10 }));
        let mut rng = ChaCha12Rng::seed_from_u64(42);
        assert!(!cond.evaluate(&make_ctx(10), "c", &mut rng));
        assert!(cond.evaluate(&make_ctx(9), "c", &mut rng));
    }
}
