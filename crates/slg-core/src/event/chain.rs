//! 事件链引擎

use std::collections::BTreeMap;

use rand::SeedableRng;
use rand_chacha::ChaCha12Rng;
use serde::{Deserialize, Serialize};
use slg_data::ids::{FactionId, TileKey};

use crate::entity::faction::FactionState;
use crate::event::effect::{EffectResult, EventEffect};
use crate::event::trigger::{EvalContext, TriggerCondition};

/// 事件链定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventChainDef {
    /// 链唯一 ID
    pub id: String,
    /// 链显示名
    pub name: String,
    /// 按顺序排列的事件节点
    pub nodes: Vec<EventNode>,
    /// 是否循环执行
    pub repeat: bool,
}

/// 事件节点
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventNode {
    /// 触发条件
    pub trigger: TriggerCondition,
    /// 触发后执行的效果列表
    pub effects: Vec<EventEffect>,
    /// 显式指定下一个节点索引（None = 下一个）
    pub next_index: Option<usize>,
}

/// 事件链运行实例
#[derive(Debug, Clone)]
pub struct EventChainInstance {
    /// 对应的 EventChainDef.id
    pub chain_id: String,
    /// 当前等待触发的节点索引
    pub current_index: usize,
    /// 已触发过的节点索引（用于调试/回放）
    pub triggered_nodes: Vec<usize>,
    /// 最后一次触发的 tick
    pub last_triggered_tick: u64,
}

/// 事件链存储（游戏 Resource）
#[derive(Debug, Default)]
pub struct EventChainStore {
    /// 已注册的事件链定义
    pub definitions: BTreeMap<String, EventChainDef>,
    /// 活跃的事件链实例
    pub instances: Vec<EventChainInstance>,
    /// 全局触发记录（用于冷却判断）
    pub last_triggered: BTreeMap<String, u64>,
}

impl EventChainStore {
    /// 注册事件链定义
    pub fn register(&mut self, def: EventChainDef) {
        self.definitions.insert(def.id.clone(), def);
    }

    /// 启动一个事件链实例
    pub fn start_chain(&mut self, chain_id: &str) -> bool {
        if !self.definitions.contains_key(chain_id) {
            return false;
        }
        // 避免重复启动
        if self.instances.iter().any(|i| i.chain_id == chain_id) {
            return false;
        }
        self.instances.push(EventChainInstance {
            chain_id: chain_id.to_string(),
            current_index: 0,
            triggered_nodes: Vec::new(),
            last_triggered_tick: 0,
        });
        true
    }

    /// 每 tick 评估所有活跃事件链，返回消息列表
    pub fn tick(
        &mut self,
        ctx: &EvalContext,
        factions: &mut BTreeMap<FactionId, FactionState>,
        tile_owners: &mut BTreeMap<TileKey, FactionId>,
    ) -> Vec<String> {
        let mut messages = Vec::new();
        let seed = ctx.current_tick;

        for instance in &mut self.instances {
            let def = match self.definitions.get(&instance.chain_id) {
                Some(d) => d,
                None => continue,
            };

            if instance.current_index >= def.nodes.len() {
                if def.repeat {
                    instance.current_index = 0;
                } else {
                    continue;
                }
            }

            // 节点评估循环：显式导航（分支跳转、next_index）后在同一 tick 内继续评估目标节点。
            // 自然推进（index += 1）退出循环，等待下一 tick。
            let max_steps = def.nodes.len() + 1; // 防止无限循环
            for _ in 0..max_steps {
                if instance.current_index >= def.nodes.len() {
                    if def.repeat {
                        instance.current_index = 0;
                    } else {
                        break;
                    }
                }

                let node = &def.nodes[instance.current_index];
                // 每个节点使用确定性 rng（种子 = tick ^ node_index ^ chain_id哈希）
                let chain_seed = seed
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(instance.current_index as u64)
                    ^ Self::hash_str(&instance.chain_id);
                let mut rng = ChaCha12Rng::seed_from_u64(chain_seed);

                if !node.trigger.evaluate(ctx, &instance.chain_id, &mut rng) {
                    break;
                }

                // 执行效果
                let mut branch_target = None;
                for effect in &node.effects {
                    if let Some(result) = effect.execute(factions, tile_owners) {
                        match result {
                            EffectResult::Message(msg) => messages.push(msg),
                            EffectResult::BranchTo(idx) => branch_target = Some(idx),
                        }
                    }
                }

                instance.triggered_nodes.push(instance.current_index);
                instance.last_triggered_tick = ctx.current_tick;

                // 更新全局触发记录
                self.last_triggered
                    .insert(instance.chain_id.clone(), ctx.current_tick);

                // 前进到下一个节点
                if let Some(target) = branch_target {
                    // 显式分支：同一 tick 内继续评估目标节点
                    instance.current_index = target;
                    continue;
                } else if let Some(next) = node.next_index {
                    // 显式导航：同一 tick 内继续评估
                    instance.current_index = next;
                    continue;
                } else {
                    // 自然推进：退出循环，等待下一 tick
                    instance.current_index += 1;
                    break;
                }
            }
        }

        messages
    }

    /// 简易字符串哈希（djb2 变体）
    fn hash_str(s: &str) -> u64 {
        let mut h: u64 = 5381;
        for b in s.bytes() {
            h = h.wrapping_mul(33).wrapping_add(b as u64);
        }
        h
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::faction::{FactionPersonality, FactionResources};
    use crate::map::grid::HexCoord;

    fn make_faction(gold: u64) -> FactionState {
        FactionState {
            resources: FactionResources {
                gold,
                food: 500,
                wood: 300,
                iron: 200,
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
            ..Default::default()
        }
    }

    fn make_ctx(tick: u64, factions: BTreeMap<FactionId, FactionState>) -> EvalContext {
        EvalContext {
            current_tick: tick,
            factions,
            tile_owners: BTreeMap::new(),
            last_triggered: BTreeMap::new(),
        }
    }

    #[test]
    fn test_simple_time_trigger() {
        let mut store = EventChainStore::default();
        store.register(EventChainDef {
            id: "chain_1".into(),
            name: "test".into(),
            nodes: vec![EventNode {
                trigger: TriggerCondition::TimeReached { tick: 5 },
                effects: vec![EventEffect::ShowMessage {
                    key: "hello".into(),
                }],
                next_index: None,
            }],
            repeat: false,
        });
        store.start_chain("chain_1");

        let factions = BTreeMap::new();
        let mut f_mut = factions.clone();

        // tick 4 -> 不触发
        let ctx4 = make_ctx(4, factions.clone());
        let msgs = store.tick(&ctx4, &mut f_mut, &mut BTreeMap::new());
        assert!(msgs.is_empty());

        // tick 5 -> 触发
        let ctx5 = make_ctx(5, factions.clone());
        let msgs = store.tick(&ctx5, &mut f_mut, &mut BTreeMap::new());
        assert_eq!(msgs, vec!["hello".to_string()]);
    }

    #[test]
    fn test_multi_node_chain() {
        let mut store = EventChainStore::default();
        store.register(EventChainDef {
            id: "chain_multi".into(),
            name: "multi".into(),
            nodes: vec![
                EventNode {
                    trigger: TriggerCondition::TimeReached { tick: 3 },
                    effects: vec![EventEffect::ShowMessage {
                        key: "step1".into(),
                    }],
                    next_index: None,
                },
                EventNode {
                    trigger: TriggerCondition::TimeReached { tick: 3 },
                    effects: vec![EventEffect::ShowMessage {
                        key: "step2".into(),
                    }],
                    next_index: None,
                },
            ],
            repeat: false,
        });
        store.start_chain("chain_multi");

        let factions = BTreeMap::new();
        let mut f_mut = factions.clone();

        // tick 3 -> 节点0触发，自然推进到节点1（等待下一 tick）
        let ctx3 = make_ctx(3, factions.clone());
        let msgs = store.tick(&ctx3, &mut f_mut, &mut BTreeMap::new());
        assert_eq!(msgs, vec!["step1".to_string()]);

        // tick 4 -> 节点1触发
        let ctx4 = make_ctx(4, factions.clone());
        let msgs = store.tick(&ctx4, &mut f_mut, &mut BTreeMap::new());
        assert_eq!(msgs, vec!["step2".to_string()]);

        // tick 5 -> 已经结束，不触发
        let ctx5 = make_ctx(5, factions.clone());
        let msgs = store.tick(&ctx5, &mut f_mut, &mut BTreeMap::new());
        assert!(msgs.is_empty());
    }

    #[test]
    fn test_repeat_chain() {
        let mut store = EventChainStore::default();
        store.register(EventChainDef {
            id: "chain_loop".into(),
            name: "loop".into(),
            nodes: vec![EventNode {
                trigger: TriggerCondition::TimeReached { tick: 0 },
                effects: vec![EventEffect::ShowMessage { key: "tick".into() }],
                next_index: None,
            }],
            repeat: true,
        });
        store.start_chain("chain_loop");

        let factions = BTreeMap::new();
        let mut f_mut = factions.clone();

        // 每个 tick 都应该触发
        for t in 0..5 {
            let ctx = make_ctx(t, factions.clone());
            let msgs = store.tick(&ctx, &mut f_mut, &mut BTreeMap::new());
            assert_eq!(msgs, vec!["tick".to_string()], "failed at tick {}", t);
        }
    }

    #[test]
    fn test_branch_index() {
        let mut store = EventChainStore::default();
        store.register(EventChainDef {
            id: "chain_branch".into(),
            name: "branch".into(),
            nodes: vec![
                EventNode {
                    trigger: TriggerCondition::TimeReached { tick: 1 },
                    effects: vec![EventEffect::SetBranchIndex { index: 2 }],
                    next_index: None,
                },
                EventNode {
                    trigger: TriggerCondition::TimeReached { tick: 1 },
                    effects: vec![EventEffect::ShowMessage {
                        key: "skipped".into(),
                    }],
                    next_index: None,
                },
                EventNode {
                    trigger: TriggerCondition::TimeReached { tick: 1 },
                    effects: vec![EventEffect::ShowMessage {
                        key: "jumped".into(),
                    }],
                    next_index: None,
                },
            ],
            repeat: false,
        });
        store.start_chain("chain_branch");

        let factions = BTreeMap::new();
        let mut f_mut = factions.clone();

        // tick 1 -> node[0] 触发，SetBranchIndex(2) -> 跳到 node[2]
        let ctx = make_ctx(1, factions.clone());
        let msgs = store.tick(&ctx, &mut f_mut, &mut BTreeMap::new());
        assert_eq!(msgs, vec!["jumped".to_string()]);
    }

    #[test]
    fn test_grant_resources_in_chain() {
        let mut store = EventChainStore::default();
        store.register(EventChainDef {
            id: "chain_res".into(),
            name: "res".into(),
            nodes: vec![EventNode {
                trigger: TriggerCondition::TimeReached { tick: 2 },
                effects: vec![EventEffect::GrantResources {
                    faction: "f1".into(),
                    gold: 500,
                    food: 0,
                    wood: 0,
                    iron: 0,
                    stone: 0,
                }],
                next_index: None,
            }],
            repeat: false,
        });
        store.start_chain("chain_res");

        let mut factions = BTreeMap::new();
        factions.insert("f1".into(), make_faction(1000));

        let ctx = make_ctx(2, factions.clone());
        let mut f_mut = factions.clone();
        store.tick(&ctx, &mut f_mut, &mut BTreeMap::new());

        assert_eq!(f_mut.get("f1").unwrap().resources.gold, 1500);
    }

    #[test]
    fn test_and_condition_chain() {
        let mut store = EventChainStore::default();
        store.register(EventChainDef {
            id: "chain_and".into(),
            name: "and".into(),
            nodes: vec![EventNode {
                trigger: TriggerCondition::And(vec![
                    TriggerCondition::TimeReached { tick: 10 },
                    TriggerCondition::FactionResourceLow {
                        faction: "f1".into(),
                        resource: "gold".into(),
                        threshold: 200,
                    },
                ]),
                effects: vec![EventEffect::ShowMessage {
                    key: "low_gold".into(),
                }],
                next_index: None,
            }],
            repeat: false,
        });
        store.start_chain("chain_and");

        let mut factions = BTreeMap::new();
        factions.insert("f1".into(), make_faction(300));

        let mut f_mut = factions.clone();

        // 时间到但 gold=300 > 200 -> 不触发
        let ctx = make_ctx(10, factions.clone());
        let msgs = store.tick(&ctx, &mut f_mut, &mut BTreeMap::new());
        assert!(msgs.is_empty());

        // gold 降到 100 < 200 -> 触发
        f_mut.get_mut("f1").unwrap().resources.gold = 100;
        let f_snap = f_mut.clone();
        let ctx2 = make_ctx(11, f_snap);
        let msgs = store.tick(&ctx2, &mut f_mut, &mut BTreeMap::new());
        assert_eq!(msgs, vec!["low_gold".to_string()]);
    }

    #[test]
    fn test_determinism() {
        // 同一事件链、同一 tick 应产生完全相同的结果
        let build_store = || {
            let mut s = EventChainStore::default();
            s.register(EventChainDef {
                id: "det".into(),
                name: "det".into(),
                nodes: vec![EventNode {
                    trigger: TriggerCondition::RandomChance {
                        probability: 0.5,
                        cooldown_ticks: 0,
                    },
                    effects: vec![EventEffect::ShowMessage {
                        key: "random_hit".into(),
                    }],
                    next_index: None,
                }],
                repeat: true,
            });
            s.start_chain("det");
            s
        };

        let factions = BTreeMap::new();
        let mut f1 = factions.clone();
        let mut f2 = factions.clone();

        let mut store1 = build_store();
        let mut store2 = build_store();

        for t in 0..20 {
            let ctx = make_ctx(t, factions.clone());
            let msgs1 = store1.tick(&ctx, &mut f1, &mut BTreeMap::new());
            let msgs2 = store2.tick(&ctx, &mut f2, &mut BTreeMap::new());
            assert_eq!(msgs1, msgs2, "determinism broken at tick {}", t);
        }
    }

    #[test]
    fn test_start_chain_nonexistent() {
        let mut store = EventChainStore::default();
        assert!(!store.start_chain("nonexistent"));
    }

    #[test]
    fn test_start_chain_duplicate() {
        let mut store = EventChainStore::default();
        store.register(EventChainDef {
            id: "dup".into(),
            name: "dup".into(),
            nodes: vec![],
            repeat: false,
        });
        assert!(store.start_chain("dup"));
        assert!(!store.start_chain("dup")); // 第二次启动应该失败
    }

    #[test]
    fn test_tile_occupied_condition() {
        let mut store = EventChainStore::default();
        let coord = HexCoord::new(3, 5);
        store.register(EventChainDef {
            id: "chain_tile".into(),
            name: "tile".into(),
            nodes: vec![EventNode {
                trigger: TriggerCondition::TileOccupied {
                    coord,
                    by: "f1".into(),
                },
                effects: vec![EventEffect::ShowMessage {
                    key: "captured".into(),
                }],
                next_index: None,
            }],
            repeat: false,
        });
        store.start_chain("chain_tile");

        let factions = BTreeMap::new();
        let mut f_mut = factions.clone();

        // 未占领 -> 不触发
        let ctx = make_ctx(1, factions.clone());
        let msgs = store.tick(&ctx, &mut f_mut, &mut BTreeMap::new());
        assert!(msgs.is_empty());

        // 占领 -> 触发
        let mut ctx2 = make_ctx(2, factions.clone());
        ctx2.tile_owners.insert(coord.to_tile_key(), "f1".into());
        let msgs = store.tick(&ctx2, &mut f_mut, &mut BTreeMap::new());
        assert_eq!(msgs, vec!["captured".to_string()]);
    }

    #[test]
    fn test_change_diplomacy_in_chain() {
        let mut store = EventChainStore::default();
        store.register(EventChainDef {
            id: "chain_diplo".into(),
            name: "diplo".into(),
            nodes: vec![EventNode {
                trigger: TriggerCondition::TimeReached { tick: 1 },
                effects: vec![EventEffect::ChangeDiplomacy {
                    a: "f1".into(),
                    b: "f2".into(),
                    delta: 30,
                }],
                next_index: None,
            }],
            repeat: false,
        });
        store.start_chain("chain_diplo");

        let mut factions = BTreeMap::new();
        factions.insert("f1".into(), make_faction(500));
        factions.insert("f2".into(), make_faction(500));
        let mut f_mut = factions.clone();

        let ctx = make_ctx(1, factions.clone());
        store.tick(&ctx, &mut f_mut, &mut BTreeMap::new());

        assert_eq!(*f_mut.get("f1").unwrap().diplomacy.get("f2").unwrap(), 30);
        assert_eq!(*f_mut.get("f2").unwrap().diplomacy.get("f1").unwrap(), 30);
    }
}
