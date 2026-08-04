//! 事件链集成测试
//!
//! 验收标准：
//! - 开局事件在 tick 0 触发
//! - 资源告急事件在资源低时触发
//! - 事件不会在错误时间触发
//! - 多个事件链独立运行

use std::collections::BTreeMap;

use slg_core::entity::faction::{FactionPersonality, FactionResources, FactionState};
use slg_core::event::chain::{EventChainDef, EventChainStore, EventNode};
use slg_core::event::effect::EventEffect;
use slg_core::event::trigger::{EvalContext, TriggerCondition};
use slg_core::map::grid::HexCoord;

// ---------------------------------------------------------------------------
// 辅助函数
// ---------------------------------------------------------------------------

fn make_faction(gold: u64, food: u64) -> FactionState {
    FactionState {
        resources: FactionResources {
            gold,
            food,
            wood: 0,
            iron: 0,
            stone: 0,
            troops: 100,
        },
        personality: FactionPersonality {
            aggression: 0.5,
            expansion: 0.5,
            diplomacy: 0.5,
            caution: 0.5,
        },
        main_city: Some(HexCoord::new(5, 5)),
        diplomacy: BTreeMap::new(),
    }
}

fn make_ctx(tick: u64, factions: BTreeMap<String, FactionState>) -> EvalContext {
    EvalContext {
        current_tick: tick,
        factions,
        tile_owners: BTreeMap::new(),
        last_triggered: BTreeMap::new(),
    }
}

// ---------------------------------------------------------------------------
// 测试用例
// ---------------------------------------------------------------------------

/// 开局事件在 tick 0 正确触发
#[test]
fn test_opening_event_triggers_at_tick_0() {
    let mut store = EventChainStore::default();

    let chain = EventChainDef {
        id: "chain_opening".to_string(),
        name: "乱世开端".to_string(),
        nodes: vec![EventNode {
            trigger: TriggerCondition::TimeReached { tick: 0 },
            effects: vec![EventEffect::ShowMessage {
                key: "event_opening".to_string(),
            }],
            next_index: None,
        }],
        repeat: false,
    };

    store.register(chain);
    store.start_chain("chain_opening");

    let factions = BTreeMap::new();
    let mut f_mut = factions.clone();
    let ctx = make_ctx(0, factions);

    let messages = store.tick(&ctx, &mut f_mut, &mut BTreeMap::new());
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0], "event_opening");
}

/// 事件在目标 tick 之前不会触发
#[test]
fn test_event_does_not_trigger_before_time() {
    let mut store = EventChainStore::default();

    let chain = EventChainDef {
        id: "chain_test".to_string(),
        name: "测试".to_string(),
        nodes: vec![EventNode {
            trigger: TriggerCondition::TimeReached { tick: 100 },
            effects: vec![EventEffect::ShowMessage {
                key: "test".to_string(),
            }],
            next_index: None,
        }],
        repeat: false,
    };

    store.register(chain);
    store.start_chain("chain_test");

    let factions = BTreeMap::new();
    let mut f_mut = factions.clone();
    let ctx = make_ctx(50, factions);

    let messages = store.tick(&ctx, &mut f_mut, &mut BTreeMap::new());
    assert!(messages.is_empty());
}

/// 资源低于阈值时触发告急事件
#[test]
fn test_resource_low_triggers_event() {
    let mut store = EventChainStore::default();

    let chain = EventChainDef {
        id: "chain_resource_warning".to_string(),
        name: "资源告急".to_string(),
        nodes: vec![EventNode {
            trigger: TriggerCondition::FactionResourceLow {
                faction: "faction_player".to_string(),
                resource: "food".to_string(),
                threshold: 200,
            },
            effects: vec![EventEffect::ShowMessage {
                key: "food_warning".to_string(),
            }],
            next_index: None,
        }],
        repeat: true,
    };

    store.register(chain);
    store.start_chain("chain_resource_warning");

    // food = 100 < 200 -> 应触发
    let mut factions = BTreeMap::new();
    factions.insert("faction_player".to_string(), make_faction(1000, 100));

    let mut f_mut = factions.clone();
    let ctx = make_ctx(100, factions);

    let messages = store.tick(&ctx, &mut f_mut, &mut BTreeMap::new());
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0], "food_warning");
}

/// 资源高于阈值时不触发告急事件
#[test]
fn test_resource_high_does_not_trigger_warning() {
    let mut store = EventChainStore::default();

    let chain = EventChainDef {
        id: "chain_resource_warning".to_string(),
        name: "资源告急".to_string(),
        nodes: vec![EventNode {
            trigger: TriggerCondition::FactionResourceLow {
                faction: "faction_player".to_string(),
                resource: "food".to_string(),
                threshold: 200,
            },
            effects: vec![EventEffect::ShowMessage {
                key: "food_warning".to_string(),
            }],
            next_index: None,
        }],
        repeat: true,
    };

    store.register(chain);
    store.start_chain("chain_resource_warning");

    // food = 500 > 200 -> 不应触发
    let mut factions = BTreeMap::new();
    factions.insert("faction_player".to_string(), make_faction(1000, 500));

    let mut f_mut = factions.clone();
    let ctx = make_ctx(100, factions);

    let messages = store.tick(&ctx, &mut f_mut, &mut BTreeMap::new());
    assert!(messages.is_empty());
}

/// 多个事件链在相同 tick 独立触发
#[test]
fn test_multiple_chains_independent() {
    let mut store = EventChainStore::default();

    store.register(EventChainDef {
        id: "chain_a".to_string(),
        name: "A".to_string(),
        nodes: vec![EventNode {
            trigger: TriggerCondition::TimeReached { tick: 10 },
            effects: vec![EventEffect::ShowMessage {
                key: "a".to_string(),
            }],
            next_index: None,
        }],
        repeat: false,
    });

    store.register(EventChainDef {
        id: "chain_b".to_string(),
        name: "B".to_string(),
        nodes: vec![EventNode {
            trigger: TriggerCondition::TimeReached { tick: 10 },
            effects: vec![EventEffect::ShowMessage {
                key: "b".to_string(),
            }],
            next_index: None,
        }],
        repeat: false,
    });

    store.start_chain("chain_a");
    store.start_chain("chain_b");

    let factions = BTreeMap::new();
    let mut f_mut = factions.clone();
    let ctx = make_ctx(10, factions);

    let messages = store.tick(&ctx, &mut f_mut, &mut BTreeMap::new());
    assert_eq!(messages.len(), 2);
    // 两条链的消息都应出现
    assert!(messages.contains(&"a".to_string()));
    assert!(messages.contains(&"b".to_string()));
}

/// 非 repeat 事件链触发完毕后不再触发
#[test]
fn test_non_repeat_chain_fires_once() {
    let mut store = EventChainStore::default();

    store.register(EventChainDef {
        id: "chain_once".to_string(),
        name: "一次性".to_string(),
        nodes: vec![EventNode {
            trigger: TriggerCondition::TimeReached { tick: 5 },
            effects: vec![EventEffect::ShowMessage {
                key: "once_msg".to_string(),
            }],
            next_index: None,
        }],
        repeat: false,
    });
    store.start_chain("chain_once");

    let factions = BTreeMap::new();
    let mut f_mut = factions.clone();

    // 第一次触发
    let ctx5 = make_ctx(5, factions.clone());
    let msgs = store.tick(&ctx5, &mut f_mut, &mut BTreeMap::new());
    assert_eq!(msgs, vec!["once_msg".to_string()]);

    // 第二次同一 tick 不应再触发
    let ctx5b = make_ctx(5, factions.clone());
    let msgs = store.tick(&ctx5b, &mut f_mut, &mut BTreeMap::new());
    assert!(msgs.is_empty());

    // 后续 tick 也不应再触发
    let ctx10 = make_ctx(10, factions.clone());
    let msgs = store.tick(&ctx10, &mut f_mut, &mut BTreeMap::new());
    assert!(msgs.is_empty());
}

/// repeat 事件链可循环触发
#[test]
fn test_repeat_chain_fires_multiple_times() {
    let mut store = EventChainStore::default();

    store.register(EventChainDef {
        id: "chain_loop".to_string(),
        name: "循环".to_string(),
        nodes: vec![EventNode {
            trigger: TriggerCondition::TimeReached { tick: 0 },
            effects: vec![EventEffect::ShowMessage {
                key: "tick".to_string(),
            }],
            next_index: None,
        }],
        repeat: true,
    });
    store.start_chain("chain_loop");

    let factions = BTreeMap::new();
    let mut f_mut = factions.clone();

    // 每个 tick 都应触发
    for t in 0..5 {
        let ctx = make_ctx(t, factions.clone());
        let msgs = store.tick(&ctx, &mut f_mut, &mut BTreeMap::new());
        assert_eq!(msgs, vec!["tick".to_string()], "failed at tick {}", t);
    }
}

/// And 条件组合：时间和资源同时满足才触发
#[test]
fn test_and_condition_triggers() {
    let mut store = EventChainStore::default();

    store.register(EventChainDef {
        id: "chain_and".to_string(),
        name: "复合条件".to_string(),
        nodes: vec![EventNode {
            trigger: TriggerCondition::And(vec![
                TriggerCondition::TimeReached { tick: 10 },
                TriggerCondition::FactionResourceLow {
                    faction: "f1".to_string(),
                    resource: "gold".to_string(),
                    threshold: 200,
                },
            ]),
            effects: vec![EventEffect::ShowMessage {
                key: "low_gold".to_string(),
            }],
            next_index: None,
        }],
        repeat: false,
    });
    store.start_chain("chain_and");

    let mut factions = BTreeMap::new();
    factions.insert("f1".to_string(), make_faction(300, 500));
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

/// 多节点事件链按顺序依次触发
#[test]
fn test_multi_node_chain_sequential() {
    let mut store = EventChainStore::default();

    store.register(EventChainDef {
        id: "chain_seq".to_string(),
        name: "顺序链".to_string(),
        nodes: vec![
            EventNode {
                trigger: TriggerCondition::TimeReached { tick: 1 },
                effects: vec![EventEffect::ShowMessage {
                    key: "step1".to_string(),
                }],
                next_index: None,
            },
            EventNode {
                trigger: TriggerCondition::TimeReached { tick: 1 },
                effects: vec![EventEffect::ShowMessage {
                    key: "step2".to_string(),
                }],
                next_index: None,
            },
        ],
        repeat: false,
    });
    store.start_chain("chain_seq");

    let factions = BTreeMap::new();
    let mut f_mut = factions.clone();

    // tick 1 -> 节点0触发，自然推进到节点1（等待下一 tick）
    let ctx1 = make_ctx(1, factions.clone());
    let msgs = store.tick(&ctx1, &mut f_mut, &mut BTreeMap::new());
    assert_eq!(msgs, vec!["step1".to_string()]);

    // tick 2 -> 节点1触发
    let ctx2 = make_ctx(2, factions.clone());
    let msgs = store.tick(&ctx2, &mut f_mut, &mut BTreeMap::new());
    assert_eq!(msgs, vec!["step2".to_string()]);

    // tick 3 -> 已经结束，不触发
    let ctx3 = make_ctx(3, factions.clone());
    let msgs = store.tick(&ctx3, &mut f_mut, &mut BTreeMap::new());
    assert!(msgs.is_empty());
}

/// 事件效果可授予资源
#[test]
fn test_grant_resources_effect() {
    let mut store = EventChainStore::default();

    store.register(EventChainDef {
        id: "chain_grant".to_string(),
        name: "赏赐".to_string(),
        nodes: vec![EventNode {
            trigger: TriggerCondition::TimeReached { tick: 2 },
            effects: vec![EventEffect::GrantResources {
                faction: "f1".to_string(),
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
    store.start_chain("chain_grant");

    let mut factions = BTreeMap::new();
    factions.insert("f1".to_string(), make_faction(1000, 500));
    let mut f_mut = factions.clone();

    let ctx = make_ctx(2, factions.clone());
    store.tick(&ctx, &mut f_mut, &mut BTreeMap::new());

    assert_eq!(f_mut.get("f1").unwrap().resources.gold, 1500);
}

/// 事件效果可改变外交关系
#[test]
fn test_change_diplomacy_effect() {
    let mut store = EventChainStore::default();

    store.register(EventChainDef {
        id: "chain_diplo".to_string(),
        name: "外交".to_string(),
        nodes: vec![EventNode {
            trigger: TriggerCondition::TimeReached { tick: 1 },
            effects: vec![EventEffect::ChangeDiplomacy {
                a: "f1".to_string(),
                b: "f2".to_string(),
                delta: 30,
            }],
            next_index: None,
        }],
        repeat: false,
    });
    store.start_chain("chain_diplo");

    let mut factions = BTreeMap::new();
    factions.insert("f1".to_string(), make_faction(500, 500));
    factions.insert("f2".to_string(), make_faction(500, 500));
    let mut f_mut = factions.clone();

    let ctx = make_ctx(1, factions.clone());
    store.tick(&ctx, &mut f_mut, &mut BTreeMap::new());

    assert_eq!(*f_mut.get("f1").unwrap().diplomacy.get("f2").unwrap(), 30);
    assert_eq!(*f_mut.get("f2").unwrap().diplomacy.get("f1").unwrap(), 30);
}

/// 不存在的事件链启动失败
#[test]
fn test_start_chain_nonexistent() {
    let mut store = EventChainStore::default();
    assert!(!store.start_chain("nonexistent"));
}

/// 重复启动同一事件链应失败
#[test]
fn test_start_chain_duplicate() {
    let mut store = EventChainStore::default();
    store.register(EventChainDef {
        id: "dup".to_string(),
        name: "dup".to_string(),
        nodes: vec![EventNode {
            trigger: TriggerCondition::TimeReached { tick: 0 },
            effects: vec![EventEffect::ShowMessage {
                key: "x".to_string(),
            }],
            next_index: None,
        }],
        repeat: false,
    });
    assert!(store.start_chain("dup"));
    assert!(!store.start_chain("dup"));
}
