//! 外交系统完整实现

use serde::{Deserialize, Serialize};
use slg_data::ids::FactionId;
use std::collections::BTreeMap;

use crate::entity::faction::FactionState;

/// 外交行为类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DiplomacyActionType {
    /// 结盟
    Alliance,
    /// 宣战
    DeclareWar,
    /// 停战
    CeaseFire,
    /// 送礼（资源）
    Gift { gold: u64, food: u64 },
    /// 威胁
    Threaten,
    /// 贸易协定
    TradeAgreement,
}

/// 外交行为
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiplomacyAction {
    pub action_type: DiplomacyActionType,
    pub source: FactionId,
    pub target: FactionId,
}

/// 外交结果
#[derive(Debug, Clone)]
pub enum DiplomacyResult {
    /// 成功
    Success { message: String },
    /// 失败（好感度不足等）
    Failed { reason: String },
    /// 已经是盟友
    AlreadyAllied,
    /// 已经在战争中
    AlreadyAtWar,
}

/// 外交系统
#[derive(Debug, Default)]
pub struct DiplomacySystem {
    /// 盟约关系：(faction_a, faction_b) -> 结盟 tick
    pub alliances: BTreeMap<(FactionId, FactionId), u64>,
    /// 战争关系：(faction_a, faction_b) -> 宣战 tick
    pub wars: BTreeMap<(FactionId, FactionId), u64>,
}

impl DiplomacySystem {
    /// 执行外交动作
    pub fn execute_action(
        &mut self,
        action: &DiplomacyAction,
        factions: &mut BTreeMap<FactionId, FactionState>,
        current_tick: u64,
    ) -> DiplomacyResult {
        match &action.action_type {
            DiplomacyActionType::Alliance => {
                self.form_alliance(&action.source, &action.target, factions, current_tick)
            }
            DiplomacyActionType::DeclareWar => {
                self.declare_war(&action.source, &action.target, factions, current_tick)
            }
            DiplomacyActionType::CeaseFire => {
                self.cease_fire(&action.source, &action.target, factions, current_tick)
            }
            DiplomacyActionType::Gift { gold, food } => {
                self.gift(&action.source, &action.target, *gold, *food, factions)
            }
            DiplomacyActionType::Threaten => {
                self.threaten(&action.source, &action.target, factions)
            }
            DiplomacyActionType::TradeAgreement => {
                self.trade_agreement(&action.source, &action.target, factions, current_tick)
            }
        }
    }

    /// 结盟
    fn form_alliance(
        &mut self,
        source: &FactionId,
        target: &FactionId,
        factions: &mut BTreeMap<FactionId, FactionState>,
        current_tick: u64,
    ) -> DiplomacyResult {
        let key = Self::normalize_pair(source, target);

        // 检查是否已经在战争中
        if self.wars.contains_key(&key) {
            return DiplomacyResult::Failed {
                reason: "双方正在战争中，无法结盟".to_string(),
            };
        }

        // 检查好感度
        let relation = Self::get_relation(source, target, factions);
        if relation < 50 {
            return DiplomacyResult::Failed {
                reason: format!("好感度不足（当前: {}，需要: 50）", relation),
            };
        }

        // 检查是否已经结盟
        if self.alliances.contains_key(&key) {
            return DiplomacyResult::AlreadyAllied;
        }

        // 建立盟约
        self.alliances.insert(key, current_tick);

        // 提升双方好感度
        Self::adjust_relation(source, target, 20, factions);

        DiplomacyResult::Success {
            message: "结盟成功".to_string(),
        }
    }

    /// 宣战
    fn declare_war(
        &mut self,
        source: &FactionId,
        target: &FactionId,
        factions: &mut BTreeMap<FactionId, FactionState>,
        current_tick: u64,
    ) -> DiplomacyResult {
        let key = Self::normalize_pair(source, target);

        // 检查是否已经在战争中
        if self.wars.contains_key(&key) {
            return DiplomacyResult::AlreadyAtWar;
        }

        // 如果是盟友，先解除盟约
        if self.alliances.contains_key(&key) {
            self.alliances.remove(&key);
            Self::adjust_relation(source, target, -30, factions);
        }

        // 宣战
        self.wars.insert(key, current_tick);

        // 大幅降低好感度
        Self::adjust_relation(source, target, -50, factions);

        DiplomacyResult::Success {
            message: "宣战成功".to_string(),
        }
    }

    /// 停战
    fn cease_fire(
        &mut self,
        source: &FactionId,
        target: &FactionId,
        factions: &mut BTreeMap<FactionId, FactionState>,
        _current_tick: u64,
    ) -> DiplomacyResult {
        let key = Self::normalize_pair(source, target);

        // 检查是否在战争中
        if !self.wars.contains_key(&key) {
            return DiplomacyResult::Failed {
                reason: "双方未在战争中".to_string(),
            };
        }

        // 检查好感度（需要较高好感度才能停战）
        let relation = Self::get_relation(source, target, factions);
        if relation < -20 {
            return DiplomacyResult::Failed {
                reason: format!("好感度太低（当前: {}，需要: -20）", relation),
            };
        }

        // 停战
        self.wars.remove(&key);

        // 提升好感度
        Self::adjust_relation(source, target, 10, factions);

        DiplomacyResult::Success {
            message: "停战成功".to_string(),
        }
    }

    /// 送礼
    fn gift(
        &self,
        source: &FactionId,
        target: &FactionId,
        gold: u64,
        food: u64,
        factions: &mut BTreeMap<FactionId, FactionState>,
    ) -> DiplomacyResult {
        // 检查资源是否足够
        let source_has_enough = factions
            .get(source)
            .is_some_and(|f| f.resources.gold >= gold && f.resources.food >= food);

        if !source_has_enough {
            return DiplomacyResult::Failed {
                reason: "资源不足".to_string(),
            };
        }

        // 转移资源
        if let Some(source_faction) = factions.get_mut(source) {
            source_faction.resources.gold -= gold;
            source_faction.resources.food -= food;
        }
        if let Some(target_faction) = factions.get_mut(target) {
            target_faction.resources.gold += gold;
            target_faction.resources.food += food;
        }

        // 提升好感度（根据礼物价值）
        let relation_boost = ((gold + food) / 100).min(30) as i32;
        Self::adjust_relation(source, target, relation_boost, factions);

        DiplomacyResult::Success {
            message: format!("送礼成功，好感度 +{}", relation_boost),
        }
    }

    /// 威胁
    fn threaten(
        &self,
        source: &FactionId,
        target: &FactionId,
        factions: &mut BTreeMap<FactionId, FactionState>,
    ) -> DiplomacyResult {
        // 检查军事实力对比
        let source_troops = factions.get(source).map_or(0, |f| f.resources.troops);
        let target_troops = factions.get(target).map_or(0, |f| f.resources.troops);

        if source_troops <= target_troops {
            return DiplomacyResult::Failed {
                reason: "军事实力不足，威胁无效".to_string(),
            };
        }

        // 威胁成功，降低对方好感度
        Self::adjust_relation(source, target, -20, factions);

        DiplomacyResult::Success {
            message: "威胁成功，对方好感度 -20".to_string(),
        }
    }

    /// 贸易协定
    fn trade_agreement(
        &mut self,
        source: &FactionId,
        target: &FactionId,
        factions: &mut BTreeMap<FactionId, FactionState>,
        _current_tick: u64,
    ) -> DiplomacyResult {
        let key = Self::normalize_pair(source, target);

        // 检查是否在战争中
        if self.wars.contains_key(&key) {
            return DiplomacyResult::Failed {
                reason: "双方正在战争中，无法贸易".to_string(),
            };
        }

        // 检查好感度
        let relation = Self::get_relation(source, target, factions);
        if relation < 20 {
            return DiplomacyResult::Failed {
                reason: format!("好感度不足（当前: {}，需要: 20）", relation),
            };
        }

        // 提升好感度
        Self::adjust_relation(source, target, 10, factions);

        DiplomacyResult::Success {
            message: "贸易协定签订成功".to_string(),
        }
    }

    /// 获取好感度
    fn get_relation(
        source: &FactionId,
        target: &FactionId,
        factions: &BTreeMap<FactionId, FactionState>,
    ) -> i32 {
        factions
            .get(source)
            .and_then(|f| f.diplomacy.get(target))
            .copied()
            .unwrap_or(0)
    }

    /// 调整好感度（双向）
    fn adjust_relation(
        source: &FactionId,
        target: &FactionId,
        delta: i32,
        factions: &mut BTreeMap<FactionId, FactionState>,
    ) {
        if let Some(source_faction) = factions.get_mut(source) {
            let relation = source_faction.diplomacy.entry(target.clone()).or_insert(0);
            *relation = (*relation + delta).clamp(-100, 100);
        }
        if let Some(target_faction) = factions.get_mut(target) {
            let relation = target_faction.diplomacy.entry(source.clone()).or_insert(0);
            *relation = (*relation + delta).clamp(-100, 100);
        }
    }

    /// 标准化势力对（确保顺序一致）
    fn normalize_pair(a: &FactionId, b: &FactionId) -> (FactionId, FactionId) {
        if a < b {
            (a.clone(), b.clone())
        } else {
            (b.clone(), a.clone())
        }
    }

    /// 检查是否是盟友
    pub fn is_allied(&self, a: &FactionId, b: &FactionId) -> bool {
        let key = Self::normalize_pair(a, b);
        self.alliances.contains_key(&key)
    }

    /// 检查是否在战争中
    pub fn is_at_war(&self, a: &FactionId, b: &FactionId) -> bool {
        let key = Self::normalize_pair(a, b);
        self.wars.contains_key(&key)
    }

    /// 每 tick 衰减好感度
    pub fn tick_decay(factions: &mut BTreeMap<FactionId, FactionState>, decay_rate: f64) {
        for faction in factions.values_mut() {
            for relation in faction.diplomacy.values_mut() {
                // 好感度向 0 衰减
                if *relation > 0 {
                    *relation = (*relation as f64 * (1.0 - decay_rate)).max(0.0) as i32;
                } else if *relation < 0 {
                    *relation = (*relation as f64 * (1.0 - decay_rate)).min(0.0) as i32;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::faction::{FactionPersonality, FactionResources};
    use crate::map::grid::HexCoord;

    fn create_faction(id: &str, troops: u32) -> (FactionId, FactionState) {
        (
            id.to_string(),
            FactionState {
                resources: FactionResources {
                    gold: 1000,
                    food: 500,
                    wood: 0,
                    iron: 0,
                    stone: 0,
                    troops,
                },
                personality: FactionPersonality {
                    aggression: 0.5,
                    expansion: 0.5,
                    diplomacy: 0.5,
                    caution: 0.5,
                },
                main_city: Some(HexCoord::new(0, 0)),
                diplomacy: BTreeMap::new(),
            },
        )
    }

    #[test]
    fn test_alliance_success() {
        let mut system = DiplomacySystem::default();
        let mut factions = BTreeMap::new();

        let (id_a, faction_a) = create_faction("a", 100);
        let (id_b, faction_b) = create_faction("b", 100);
        factions.insert(id_a.clone(), faction_a);
        factions.insert(id_b.clone(), faction_b);

        // 设置好感度足够
        factions
            .get_mut("a")
            .unwrap()
            .diplomacy
            .insert("b".to_string(), 60);
        factions
            .get_mut("b")
            .unwrap()
            .diplomacy
            .insert("a".to_string(), 60);

        let action = DiplomacyAction {
            action_type: DiplomacyActionType::Alliance,
            source: "a".to_string(),
            target: "b".to_string(),
        };

        let result = system.execute_action(&action, &mut factions, 100);
        assert!(matches!(result, DiplomacyResult::Success { .. }));
        assert!(system.is_allied(&"a".to_string(), &"b".to_string()));
    }

    #[test]
    fn test_alliance_fails_low_relation() {
        let mut system = DiplomacySystem::default();
        let mut factions = BTreeMap::new();

        let (id_a, faction_a) = create_faction("a", 100);
        let (id_b, faction_b) = create_faction("b", 100);
        factions.insert(id_a.clone(), faction_a);
        factions.insert(id_b.clone(), faction_b);

        let action = DiplomacyAction {
            action_type: DiplomacyActionType::Alliance,
            source: "a".to_string(),
            target: "b".to_string(),
        };

        let result = system.execute_action(&action, &mut factions, 100);
        assert!(matches!(result, DiplomacyResult::Failed { .. }));
    }

    #[test]
    fn test_declare_war() {
        let mut system = DiplomacySystem::default();
        let mut factions = BTreeMap::new();

        let (id_a, faction_a) = create_faction("a", 100);
        let (id_b, faction_b) = create_faction("b", 100);
        factions.insert(id_a.clone(), faction_a);
        factions.insert(id_b.clone(), faction_b);

        let action = DiplomacyAction {
            action_type: DiplomacyActionType::DeclareWar,
            source: "a".to_string(),
            target: "b".to_string(),
        };

        let result = system.execute_action(&action, &mut factions, 100);
        assert!(matches!(result, DiplomacyResult::Success { .. }));
        assert!(system.is_at_war(&"a".to_string(), &"b".to_string()));
    }

    #[test]
    fn test_declare_war_breaks_alliance() {
        let mut system = DiplomacySystem::default();
        let mut factions = BTreeMap::new();

        let (id_a, faction_a) = create_faction("a", 100);
        let (id_b, faction_b) = create_faction("b", 100);
        factions.insert(id_a.clone(), faction_a);
        factions.insert(id_b.clone(), faction_b);

        // 先设置盟约
        factions
            .get_mut("a")
            .unwrap()
            .diplomacy
            .insert("b".to_string(), 60);
        factions
            .get_mut("b")
            .unwrap()
            .diplomacy
            .insert("a".to_string(), 60);

        let action = DiplomacyAction {
            action_type: DiplomacyActionType::Alliance,
            source: "a".to_string(),
            target: "b".to_string(),
        };
        system.execute_action(&action, &mut factions, 50);
        assert!(system.is_allied(&"a".to_string(), &"b".to_string()));

        // 宣战应解除盟约
        let action = DiplomacyAction {
            action_type: DiplomacyActionType::DeclareWar,
            source: "a".to_string(),
            target: "b".to_string(),
        };
        let result = system.execute_action(&action, &mut factions, 100);
        assert!(matches!(result, DiplomacyResult::Success { .. }));
        assert!(!system.is_allied(&"a".to_string(), &"b".to_string()));
        assert!(system.is_at_war(&"a".to_string(), &"b".to_string()));
    }

    #[test]
    fn test_cease_fire_success() {
        let mut system = DiplomacySystem::default();
        let mut factions = BTreeMap::new();

        let (id_a, faction_a) = create_faction("a", 100);
        let (id_b, faction_b) = create_faction("b", 100);
        factions.insert(id_a.clone(), faction_a);
        factions.insert(id_b.clone(), faction_b);

        // 先宣战
        let action = DiplomacyAction {
            action_type: DiplomacyActionType::DeclareWar,
            source: "a".to_string(),
            target: "b".to_string(),
        };
        system.execute_action(&action, &mut factions, 50);

        // 停战需要好感度 >= -20，宣战后好感度降到 -50，手动调高
        factions
            .get_mut("a")
            .unwrap()
            .diplomacy
            .insert("b".to_string(), -10);
        factions
            .get_mut("b")
            .unwrap()
            .diplomacy
            .insert("a".to_string(), -10);

        let action = DiplomacyAction {
            action_type: DiplomacyActionType::CeaseFire,
            source: "a".to_string(),
            target: "b".to_string(),
        };
        let result = system.execute_action(&action, &mut factions, 100);
        assert!(matches!(result, DiplomacyResult::Success { .. }));
        assert!(!system.is_at_war(&"a".to_string(), &"b".to_string()));
    }

    #[test]
    fn test_cease_fire_fails_not_at_war() {
        let mut system = DiplomacySystem::default();
        let mut factions = BTreeMap::new();

        let (id_a, faction_a) = create_faction("a", 100);
        let (id_b, faction_b) = create_faction("b", 100);
        factions.insert(id_a.clone(), faction_a);
        factions.insert(id_b.clone(), faction_b);

        let action = DiplomacyAction {
            action_type: DiplomacyActionType::CeaseFire,
            source: "a".to_string(),
            target: "b".to_string(),
        };
        let result = system.execute_action(&action, &mut factions, 100);
        assert!(matches!(result, DiplomacyResult::Failed { .. }));
    }

    #[test]
    fn test_cease_fire_fails_low_relation() {
        let mut system = DiplomacySystem::default();
        let mut factions = BTreeMap::new();

        let (id_a, faction_a) = create_faction("a", 100);
        let (id_b, faction_b) = create_faction("b", 100);
        factions.insert(id_a.clone(), faction_a);
        factions.insert(id_b.clone(), faction_b);

        // 先宣战（好感度降到 -50）
        let action = DiplomacyAction {
            action_type: DiplomacyActionType::DeclareWar,
            source: "a".to_string(),
            target: "b".to_string(),
        };
        system.execute_action(&action, &mut factions, 50);

        // 好感度太低，停战应失败
        let action = DiplomacyAction {
            action_type: DiplomacyActionType::CeaseFire,
            source: "a".to_string(),
            target: "b".to_string(),
        };
        let result = system.execute_action(&action, &mut factions, 100);
        assert!(matches!(result, DiplomacyResult::Failed { .. }));
        assert!(system.is_at_war(&"a".to_string(), &"b".to_string()));
    }

    #[test]
    fn test_gift_increases_relation() {
        let mut system = DiplomacySystem::default();
        let mut factions = BTreeMap::new();

        let (id_a, faction_a) = create_faction("a", 100);
        let (id_b, faction_b) = create_faction("b", 100);
        factions.insert(id_a.clone(), faction_a);
        factions.insert(id_b.clone(), faction_b);

        let action = DiplomacyAction {
            action_type: DiplomacyActionType::Gift {
                gold: 500,
                food: 200,
            },
            source: "a".to_string(),
            target: "b".to_string(),
        };

        let result = system.execute_action(&action, &mut factions, 100);
        assert!(matches!(result, DiplomacyResult::Success { .. }));

        let relation = factions
            .get("b")
            .unwrap()
            .diplomacy
            .get("a")
            .copied()
            .unwrap_or(0);
        assert!(relation > 0);

        // 验证资源转移
        assert_eq!(factions.get("a").unwrap().resources.gold, 500);
        assert_eq!(factions.get("a").unwrap().resources.food, 300);
        assert_eq!(factions.get("b").unwrap().resources.gold, 1500);
        assert_eq!(factions.get("b").unwrap().resources.food, 700);
    }

    #[test]
    fn test_gift_fails_insufficient_resources() {
        let mut system = DiplomacySystem::default();
        let mut factions = BTreeMap::new();

        let (id_a, faction_a) = create_faction("a", 100);
        let (id_b, faction_b) = create_faction("b", 100);
        factions.insert(id_a.clone(), faction_a);
        factions.insert(id_b.clone(), faction_b);

        let action = DiplomacyAction {
            action_type: DiplomacyActionType::Gift {
                gold: 2000,
                food: 0,
            },
            source: "a".to_string(),
            target: "b".to_string(),
        };

        let result = system.execute_action(&action, &mut factions, 100);
        assert!(matches!(result, DiplomacyResult::Failed { .. }));
    }

    #[test]
    fn test_threaten_stronger_army() {
        let mut system = DiplomacySystem::default();
        let mut factions = BTreeMap::new();

        let (id_a, faction_a) = create_faction("a", 200);
        let (id_b, faction_b) = create_faction("b", 100);
        factions.insert(id_a.clone(), faction_a);
        factions.insert(id_b.clone(), faction_b);

        let action = DiplomacyAction {
            action_type: DiplomacyActionType::Threaten,
            source: "a".to_string(),
            target: "b".to_string(),
        };

        let result = system.execute_action(&action, &mut factions, 100);
        assert!(matches!(result, DiplomacyResult::Success { .. }));

        // 威胁降低对方好感度
        let relation = factions
            .get("b")
            .unwrap()
            .diplomacy
            .get("a")
            .copied()
            .unwrap_or(0);
        assert_eq!(relation, -20);
    }

    #[test]
    fn test_threaten_weaker_army() {
        let mut system = DiplomacySystem::default();
        let mut factions = BTreeMap::new();

        let (id_a, faction_a) = create_faction("a", 100);
        let (id_b, faction_b) = create_faction("b", 200);
        factions.insert(id_a.clone(), faction_a);
        factions.insert(id_b.clone(), faction_b);

        let action = DiplomacyAction {
            action_type: DiplomacyActionType::Threaten,
            source: "a".to_string(),
            target: "b".to_string(),
        };

        let result = system.execute_action(&action, &mut factions, 100);
        assert!(matches!(result, DiplomacyResult::Failed { .. }));
    }

    #[test]
    fn test_threaten_equal_army() {
        let mut system = DiplomacySystem::default();
        let mut factions = BTreeMap::new();

        let (id_a, faction_a) = create_faction("a", 100);
        let (id_b, faction_b) = create_faction("b", 100);
        factions.insert(id_a.clone(), faction_a);
        factions.insert(id_b.clone(), faction_b);

        let action = DiplomacyAction {
            action_type: DiplomacyActionType::Threaten,
            source: "a".to_string(),
            target: "b".to_string(),
        };

        let result = system.execute_action(&action, &mut factions, 100);
        assert!(matches!(result, DiplomacyResult::Failed { .. }));
    }

    #[test]
    fn test_trade_agreement_success() {
        let mut system = DiplomacySystem::default();
        let mut factions = BTreeMap::new();

        let (id_a, faction_a) = create_faction("a", 100);
        let (id_b, faction_b) = create_faction("b", 100);
        factions.insert(id_a.clone(), faction_a);
        factions.insert(id_b.clone(), faction_b);

        // 设置好感度足够
        factions
            .get_mut("a")
            .unwrap()
            .diplomacy
            .insert("b".to_string(), 30);
        factions
            .get_mut("b")
            .unwrap()
            .diplomacy
            .insert("a".to_string(), 30);

        let action = DiplomacyAction {
            action_type: DiplomacyActionType::TradeAgreement,
            source: "a".to_string(),
            target: "b".to_string(),
        };

        let result = system.execute_action(&action, &mut factions, 100);
        assert!(matches!(result, DiplomacyResult::Success { .. }));
    }

    #[test]
    fn test_trade_agreement_fails_during_war() {
        let mut system = DiplomacySystem::default();
        let mut factions = BTreeMap::new();

        let (id_a, faction_a) = create_faction("a", 100);
        let (id_b, faction_b) = create_faction("b", 100);
        factions.insert(id_a.clone(), faction_a);
        factions.insert(id_b.clone(), faction_b);

        // 先宣战
        let action = DiplomacyAction {
            action_type: DiplomacyActionType::DeclareWar,
            source: "a".to_string(),
            target: "b".to_string(),
        };
        system.execute_action(&action, &mut factions, 50);

        // 贸易应失败
        let action = DiplomacyAction {
            action_type: DiplomacyActionType::TradeAgreement,
            source: "a".to_string(),
            target: "b".to_string(),
        };
        let result = system.execute_action(&action, &mut factions, 100);
        assert!(matches!(result, DiplomacyResult::Failed { .. }));
    }

    #[test]
    fn test_trade_agreement_fails_low_relation() {
        let mut system = DiplomacySystem::default();
        let mut factions = BTreeMap::new();

        let (id_a, faction_a) = create_faction("a", 100);
        let (id_b, faction_b) = create_faction("b", 100);
        factions.insert(id_a.clone(), faction_a);
        factions.insert(id_b.clone(), faction_b);

        let action = DiplomacyAction {
            action_type: DiplomacyActionType::TradeAgreement,
            source: "a".to_string(),
            target: "b".to_string(),
        };

        let result = system.execute_action(&action, &mut factions, 100);
        assert!(matches!(result, DiplomacyResult::Failed { .. }));
    }

    #[test]
    fn test_relation_decay() {
        let mut factions = BTreeMap::new();

        let (id_a, faction_a) = create_faction("a", 100);
        factions.insert(id_a.clone(), faction_a);
        factions
            .get_mut("a")
            .unwrap()
            .diplomacy
            .insert("b".to_string(), 50);

        DiplomacySystem::tick_decay(&mut factions, 0.1);

        let relation = factions
            .get("a")
            .unwrap()
            .diplomacy
            .get("b")
            .copied()
            .unwrap_or(0);
        assert!(relation < 50);
        assert!(relation > 0);
    }

    #[test]
    fn test_relation_decay_negative() {
        let mut factions = BTreeMap::new();

        let (id_a, faction_a) = create_faction("a", 100);
        factions.insert(id_a.clone(), faction_a);
        factions
            .get_mut("a")
            .unwrap()
            .diplomacy
            .insert("b".to_string(), -50);

        DiplomacySystem::tick_decay(&mut factions, 0.1);

        let relation = factions
            .get("a")
            .unwrap()
            .diplomacy
            .get("b")
            .copied()
            .unwrap_or(0);
        assert!(relation > -50);
        assert!(relation < 0);
    }

    #[test]
    fn test_relation_decay_zero_stays_zero() {
        let mut factions = BTreeMap::new();

        let (id_a, faction_a) = create_faction("a", 100);
        factions.insert(id_a.clone(), faction_a);
        factions
            .get_mut("a")
            .unwrap()
            .diplomacy
            .insert("b".to_string(), 0);

        DiplomacySystem::tick_decay(&mut factions, 0.1);

        let relation = factions
            .get("a")
            .unwrap()
            .diplomacy
            .get("b")
            .copied()
            .unwrap_or(0);
        assert_eq!(relation, 0);
    }

    #[test]
    fn test_already_at_war() {
        let mut system = DiplomacySystem::default();
        let mut factions = BTreeMap::new();

        let (id_a, faction_a) = create_faction("a", 100);
        let (id_b, faction_b) = create_faction("b", 100);
        factions.insert(id_a.clone(), faction_a);
        factions.insert(id_b.clone(), faction_b);

        let action = DiplomacyAction {
            action_type: DiplomacyActionType::DeclareWar,
            source: "a".to_string(),
            target: "b".to_string(),
        };

        let result = system.execute_action(&action, &mut factions, 100);
        assert!(matches!(result, DiplomacyResult::Success { .. }));

        // 再次宣战应返回 AlreadyAtWar
        let action = DiplomacyAction {
            action_type: DiplomacyActionType::DeclareWar,
            source: "a".to_string(),
            target: "b".to_string(),
        };
        let result = system.execute_action(&action, &mut factions, 100);
        assert!(matches!(result, DiplomacyResult::AlreadyAtWar));
    }

    #[test]
    fn test_already_allied() {
        let mut system = DiplomacySystem::default();
        let mut factions = BTreeMap::new();

        let (id_a, faction_a) = create_faction("a", 100);
        let (id_b, faction_b) = create_faction("b", 100);
        factions.insert(id_a.clone(), faction_a);
        factions.insert(id_b.clone(), faction_b);

        // 设置好感度足够
        factions
            .get_mut("a")
            .unwrap()
            .diplomacy
            .insert("b".to_string(), 60);
        factions
            .get_mut("b")
            .unwrap()
            .diplomacy
            .insert("a".to_string(), 60);

        let action = DiplomacyAction {
            action_type: DiplomacyActionType::Alliance,
            source: "a".to_string(),
            target: "b".to_string(),
        };
        system.execute_action(&action, &mut factions, 100);

        // 再次结盟应返回 AlreadyAllied
        let action = DiplomacyAction {
            action_type: DiplomacyActionType::Alliance,
            source: "a".to_string(),
            target: "b".to_string(),
        };
        let result = system.execute_action(&action, &mut factions, 100);
        assert!(matches!(result, DiplomacyResult::AlreadyAllied));
    }

    #[test]
    fn test_alliance_blocked_by_war() {
        let mut system = DiplomacySystem::default();
        let mut factions = BTreeMap::new();

        let (id_a, faction_a) = create_faction("a", 100);
        let (id_b, faction_b) = create_faction("b", 100);
        factions.insert(id_a.clone(), faction_a);
        factions.insert(id_b.clone(), faction_b);

        // 先宣战
        let action = DiplomacyAction {
            action_type: DiplomacyActionType::DeclareWar,
            source: "a".to_string(),
            target: "b".to_string(),
        };
        system.execute_action(&action, &mut factions, 50);

        // 战争中结盟应失败
        factions
            .get_mut("a")
            .unwrap()
            .diplomacy
            .insert("b".to_string(), 60);
        factions
            .get_mut("b")
            .unwrap()
            .diplomacy
            .insert("a".to_string(), 60);

        let action = DiplomacyAction {
            action_type: DiplomacyActionType::Alliance,
            source: "a".to_string(),
            target: "b".to_string(),
        };
        let result = system.execute_action(&action, &mut factions, 100);
        assert!(matches!(result, DiplomacyResult::Failed { .. }));
    }
}
