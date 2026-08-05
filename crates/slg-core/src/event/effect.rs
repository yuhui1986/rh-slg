//! 事件效果

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use slg_data::ids::{FactionId, TileKey};

use crate::entity::faction::FactionState;
use crate::map::grid::HexCoord;

/// 事件效果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EventEffect {
    /// 授予资源（正数增加，负数减少）
    GrantResources {
        faction: FactionId,
        gold: i64,
        food: i64,
        wood: i64,
        iron: i64,
        stone: i64,
    },
    /// 生成部队
    SpawnArmy {
        faction: FactionId,
        coord: HexCoord,
        count: u32,
    },
    /// 改变外交好感
    ChangeDiplomacy {
        a: FactionId,
        b: FactionId,
        delta: i32,
    },
    /// 显示消息（键名）
    ShowMessage { key: String },
    /// 跳转到事件链指定节点
    SetBranchIndex { index: usize },
    /// 修改地形
    ModifyTerrain { coord: HexCoord, terrain: String },
}

/// 效果执行结果
#[derive(Debug, Clone)]
pub enum EffectResult {
    Message(String),
    BranchTo(usize),
}

impl EventEffect {
    /// 执行效果，返回可能的结果
    pub fn execute(
        &self,
        factions: &mut BTreeMap<FactionId, FactionState>,
        _tile_owners: &mut BTreeMap<TileKey, FactionId>,
    ) -> Option<EffectResult> {
        match self {
            EventEffect::GrantResources {
                faction,
                gold,
                food,
                wood,
                iron,
                stone,
            } => {
                if let Some(f) = factions.get_mut(faction) {
                    Self::apply_delta(&mut f.resources.gold, *gold);
                    Self::apply_delta(&mut f.resources.food, *food);
                    Self::apply_delta(&mut f.resources.wood, *wood);
                    Self::apply_delta(&mut f.resources.iron, *iron);
                    Self::apply_delta(&mut f.resources.stone, *stone);
                }
                None
            }

            EventEffect::ShowMessage { key } => Some(EffectResult::Message(key.clone())),

            EventEffect::SetBranchIndex { index } => Some(EffectResult::BranchTo(*index)),

            EventEffect::SpawnArmy {
                faction,
                coord: _,
                count,
            } => {
                // 增加势力兵力
                if let Some(f) = factions.get_mut(faction) {
                    f.resources.troops = f.resources.troops.saturating_add(*count);
                }
                None
            }

            EventEffect::ChangeDiplomacy { a, b, delta } => {
                if let Some(fa) = factions.get_mut(a) {
                    let entry = fa.diplomacy.entry(b.clone()).or_insert(0);
                    *entry += delta;
                }
                if let Some(fb) = factions.get_mut(b) {
                    let entry = fb.diplomacy.entry(a.clone()).or_insert(0);
                    *entry += delta;
                }
                None
            }

            EventEffect::ModifyTerrain { .. } => {
                // 地形修改留待后续实现
                None
            }
        }
    }

    /// 对 u64 资源值应用 i64 偏移，结果 clamp 到 >= 0
    fn apply_delta(current: &mut u64, delta: i64) {
        *current = (*current as i64 + delta).max(0) as u64;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::faction::{FactionPersonality, FactionResources};

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
            ..Default::default()
        }
    }

    #[test]
    fn test_grant_resources_positive() {
        let eff = EventEffect::GrantResources {
            faction: "f1".into(),
            gold: 200,
            food: -50,
            wood: 0,
            iron: 0,
            stone: 0,
        };
        let mut factions = BTreeMap::new();
        factions.insert("f1".into(), make_faction(500, 100));
        let mut tiles = BTreeMap::new();

        eff.execute(&mut factions, &mut tiles);

        let f = factions.get("f1").unwrap();
        assert_eq!(f.resources.gold, 700);
        assert_eq!(f.resources.food, 50);
    }

    #[test]
    fn test_grant_resources_clamp_zero() {
        let eff = EventEffect::GrantResources {
            faction: "f1".into(),
            gold: -1000,
            food: 0,
            wood: 0,
            iron: 0,
            stone: 0,
        };
        let mut factions = BTreeMap::new();
        factions.insert("f1".into(), make_faction(500, 100));
        let mut tiles = BTreeMap::new();

        eff.execute(&mut factions, &mut tiles);
        assert_eq!(factions.get("f1").unwrap().resources.gold, 0);
    }

    #[test]
    fn test_show_message() {
        let eff = EventEffect::ShowMessage {
            key: "msg_hello".into(),
        };
        let mut factions = BTreeMap::new();
        let mut tiles = BTreeMap::new();

        let result = eff.execute(&mut factions, &mut tiles);
        match result {
            Some(EffectResult::Message(k)) => assert_eq!(k, "msg_hello"),
            _ => panic!("expected Message result"),
        }
    }

    #[test]
    fn test_set_branch_index() {
        let eff = EventEffect::SetBranchIndex { index: 3 };
        let mut factions = BTreeMap::new();
        let mut tiles = BTreeMap::new();

        let result = eff.execute(&mut factions, &mut tiles);
        match result {
            Some(EffectResult::BranchTo(idx)) => assert_eq!(idx, 3),
            _ => panic!("expected BranchTo result"),
        }
    }

    #[test]
    fn test_spawn_army() {
        let eff = EventEffect::SpawnArmy {
            faction: "f1".into(),
            coord: HexCoord::new(0, 0),
            count: 100,
        };
        let mut factions = BTreeMap::new();
        factions.insert("f1".into(), make_faction(500, 100));
        let mut tiles = BTreeMap::new();

        eff.execute(&mut factions, &mut tiles);
        assert_eq!(factions.get("f1").unwrap().resources.troops, 150);
    }

    #[test]
    fn test_change_diplomacy() {
        let eff = EventEffect::ChangeDiplomacy {
            a: "f1".into(),
            b: "f2".into(),
            delta: 20,
        };
        let mut factions = BTreeMap::new();
        factions.insert("f1".into(), make_faction(500, 100));
        factions.insert("f2".into(), make_faction(400, 200));
        let mut tiles = BTreeMap::new();

        eff.execute(&mut factions, &mut tiles);
        assert_eq!(
            *factions.get("f1").unwrap().diplomacy.get("f2").unwrap(),
            20
        );
        assert_eq!(
            *factions.get("f2").unwrap().diplomacy.get("f1").unwrap(),
            20
        );
    }
}
