//! AI 决策系统
//!
//! 三层 AI 决策架构：
//! - Layer 0 硬规则（每 tick）：主城被围 -> 全军回防 / 兵力 < 阈值 -> 停攻征兵 / 资源 < 7 天消耗 -> 停建
//! - Layer 1 战略层（每 50 tick）：Region 价值评估 -> 主攻方向；外交威胁评估 -> 结盟/宣战
//! - Layer 2 战术层（每 10 tick）：候选动作效用评分（占地/攻城/增援/侦察），取 Top-N
//! - Layer 3 执行层（每 tick）：战术指令 -> 具体行军命令入队
//!
//! 错峰调度：AISlotAssignments，`tick % 10 == slot` 时执行。

pub mod diplomacy;
pub mod persona;
pub mod utility;

use std::collections::BTreeMap;

use slg_data::ids::{FactionId, TileKey};

use crate::ai::utility::{
    generate_candidates, score_defend, score_recruit, CandidateAction, ScoredAction,
};
use crate::clock::should_ai_decide;
use crate::entity::faction::FactionState;
use crate::map::grid::HexCoord;
use crate::resource::{CommandQueue, PlayerCommand};

// ---------------------------------------------------------------------------
// AI 决策入口
// ---------------------------------------------------------------------------

/// AI 决策入口（错峰调度）
///
/// 每 tick 检查当前 tick 是否轮到该势力执行决策。
/// 错峰策略：`tick % 10 == slot` 时执行。
pub fn tick_ai(
    faction_id: &FactionId,
    faction: &mut FactionState,
    slot: u8,
    current_tick: u64,
    tile_owners: &BTreeMap<TileKey, FactionId>,
    tile_levels: &BTreeMap<TileKey, u8>,
    commands: &mut CommandQueue,
) {
    // 错峰检查：只在该势力的 slot 周期执行
    if !should_ai_decide(current_tick, slot) {
        return;
    }

    // Layer 0: 硬规则兜底（每 tick 执行）
    if apply_hard_rules(faction_id, faction, commands) {
        return; // 硬规则触发，跳过效用决策
    }

    // Layer 1: 防御评估 — 扫描己方领地威胁
    if current_tick.is_multiple_of(10) {
        let mut best_defend_score = 0.0;
        for (&key, owner) in tile_owners.iter() {
            if owner == faction_id {
                let coord = HexCoord::from_tile_key(key);
                let score = score_defend(coord, faction, tile_owners, faction_id);
                if score > best_defend_score {
                    best_defend_score = score;
                }
            }
        }

        // 受到威胁时优先征兵补员
        if best_defend_score > 0.3 {
            let recruit_score =
                score_recruit(faction, faction.resources.troops, &faction.resources);
            if recruit_score > 0.5 {
                // TODO: 发出征兵命令（当前仅做评分记录，后续接入 RecruitTroops）
                let _ = recruit_score;
            }
        }
    }

    // Layer 2: 战术层（每 10 tick）
    if current_tick.is_multiple_of(10) {
        let candidates = generate_candidates(faction_id, faction, tile_owners, tile_levels);

        // 取最高分动作执行
        if let Some(best) = candidates.first() {
            execute_action(best, faction_id, commands);
        }
    }
}

// ---------------------------------------------------------------------------
// Layer 0: 硬规则兜底
// ---------------------------------------------------------------------------

/// Layer 0: 硬规则兜底
///
/// 返回 `true` 表示硬规则触发（应跳过效用决策），`false` 表示正常。
fn apply_hard_rules(
    _faction_id: &FactionId,
    faction: &FactionState,
    _commands: &mut CommandQueue,
) -> bool {
    // 规则 1: 资源 < 7 天消耗 -> 停建
    if faction.resources.food < 100 {
        // TODO: 取消建造队列
        return true;
    }

    // 规则 2: 兵力 < 阈值 -> 停攻征兵
    if faction.resources.troops < 100 {
        // TODO: 停止进攻，开始征兵
        return true;
    }

    false
}

// ---------------------------------------------------------------------------
// Layer 3: 执行层
// ---------------------------------------------------------------------------

/// 执行最高分动作
///
/// 将战术指令转化为具体命令入队。
fn execute_action(action: &ScoredAction, faction_id: &FactionId, commands: &mut CommandQueue) {
    match &action.action {
        CandidateAction::OccupyTile { target, .. } => {
            commands
                .commands
                .push_back(PlayerCommand::OccupyTile(*target, faction_id.clone()));
        }
        CandidateAction::MarchAttack { target, .. } => {
            // TODO: 派遣部队
            let _ = target;
        }
        CandidateAction::Recruit { city, count } => {
            // TODO: 征兵
            let _ = (city, count);
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    use crate::map::grid::HexCoord;
    use crate::resource::CommandQueue;

    /// 创建测试用势力状态
    fn create_faction() -> FactionState {
        FactionState {
            resources: crate::entity::faction::FactionResources {
                gold: 1000,
                food: 500,
                wood: 300,
                iron: 200,
                stone: 100,
                troops: 500,
            },
            personality: crate::entity::faction::FactionPersonality {
                aggression: 0.8,
                expansion: 0.9,
                diplomacy: 0.4,
                caution: 0.5,
            },
            main_city: Some(HexCoord::new(5, 5)),
            diplomacy: BTreeMap::new(),
            ..Default::default()
        }
    }

    #[test]
    fn test_hard_rules_low_food() {
        let mut faction = create_faction();
        faction.resources.food = 50;
        let faction_id = "faction_1".to_string();
        let mut commands = CommandQueue::default();

        let triggered = apply_hard_rules(&faction_id, &faction, &mut commands);
        assert!(triggered, "hard rules should trigger when food < 100");
    }

    #[test]
    fn test_hard_rules_low_troops() {
        let mut faction = create_faction();
        faction.resources.troops = 50;
        let faction_id = "faction_1".to_string();
        let mut commands = CommandQueue::default();

        let triggered = apply_hard_rules(&faction_id, &faction, &mut commands);
        assert!(triggered, "hard rules should trigger when troops < 100");
    }

    #[test]
    fn test_hard_rules_normal() {
        let faction = create_faction();
        let faction_id = "faction_1".to_string();
        let mut commands = CommandQueue::default();

        let triggered = apply_hard_rules(&faction_id, &faction, &mut commands);
        assert!(
            !triggered,
            "hard rules should not trigger under normal conditions"
        );
    }

    #[test]
    fn test_ai_slot_staggering() {
        // faction 0: tick 0, 10, 20 ...
        assert!(should_ai_decide(0, 0));
        assert!(!should_ai_decide(0, 1));
        assert!(should_ai_decide(10, 0));

        // faction 5: tick 5, 15, 25 ...
        assert!(should_ai_decide(5, 5));
        assert!(!should_ai_decide(6, 5));
        assert!(should_ai_decide(15, 5));

        // faction 9: tick 9, 19, 29 ...
        assert!(should_ai_decide(9, 9));
        assert!(should_ai_decide(19, 9));
        assert!(!should_ai_decide(0, 9));
    }

    #[test]
    fn test_execute_action_occupy() {
        use crate::ai::utility::ScoredAction;

        let faction_id = "faction_1".to_string();
        let mut commands = CommandQueue::default();

        let action = ScoredAction {
            action: CandidateAction::OccupyTile {
                target: HexCoord::new(6, 5),
                value: 0.5,
            },
            score: 0.5,
        };

        execute_action(&action, &faction_id, &mut commands);

        assert_eq!(commands.commands.len(), 1);
        match commands.commands.front() {
            Some(PlayerCommand::OccupyTile(coord, fid)) => {
                assert_eq!(*coord, HexCoord::new(6, 5));
                assert_eq!(*fid, faction_id);
            }
            _ => panic!("expected OccupyTile command"),
        }
    }

    // -----------------------------------------------------------------------
    // 势力差异化测试
    // -----------------------------------------------------------------------

    #[test]
    fn test_personality_differences_wei_expands_more() {
        use crate::ai::persona::{SHU, WEI};
        use crate::ai::utility::score_occupy_enhanced;

        let tile_levels = BTreeMap::new();

        let mut wei_faction = create_faction();
        wei_faction.personality = WEI;

        let mut shu_faction = create_faction();
        shu_faction.personality = SHU;

        let wei_id = "faction_wei".to_string();
        let shu_id = "faction_shu".to_string();

        let wei_score = score_occupy_enhanced(
            HexCoord::new(10, 10),
            &wei_faction,
            &BTreeMap::new(),
            &tile_levels,
            &wei_id,
            0,
        );

        let shu_score = score_occupy_enhanced(
            HexCoord::new(10, 10),
            &shu_faction,
            &BTreeMap::new(),
            &tile_levels,
            &shu_id,
            0,
        );

        assert!(
            wei_score > shu_score,
            "Wei should expand more than Shu: wei={wei_score}, shu={shu_score}"
        );
    }

    #[test]
    fn test_defend_score_wu_values_defense() {
        use crate::ai::persona::WU;
        use crate::ai::utility::score_defend;

        let fid = "faction_wu".to_string();
        let mut wu_faction = create_faction();
        wu_faction.personality = WU;

        let mut tile_owners = BTreeMap::new();
        tile_owners.insert(HexCoord::new(5, 5).to_tile_key(), fid.clone());
        tile_owners.insert(HexCoord::new(6, 5).to_tile_key(), "enemy".to_string());

        let score = score_defend(HexCoord::new(5, 5), &wu_faction, &tile_owners, &fid);

        assert!(
            score > 0.0,
            "Wu should have a positive defend score for threatened tile"
        );
    }

    #[test]
    fn test_wu_more_defensive_than_wei() {
        use crate::ai::persona::{WEI, WU};
        use crate::ai::utility::score_defend;

        let fid = "faction_wu".to_string();
        let mut tile_owners = BTreeMap::new();
        tile_owners.insert(HexCoord::new(5, 5).to_tile_key(), fid.clone());
        tile_owners.insert(HexCoord::new(6, 5).to_tile_key(), "enemy".to_string());

        let mut wu_faction = create_faction();
        wu_faction.personality = WU;

        let mut wei_faction = create_faction();
        wei_faction.personality = WEI;

        let wu_score = score_defend(HexCoord::new(5, 5), &wu_faction, &tile_owners, &fid);
        let wei_score = score_defend(HexCoord::new(5, 5), &wei_faction, &tile_owners, &fid);

        assert!(
            wu_score > wei_score,
            "Wu should value defense more than Wei: wu={wu_score}, wei={wei_score}"
        );
    }

    #[test]
    fn test_liaodong_more_aggressive_than_nanzhong() {
        use crate::ai::persona::{LIAODONG, NANZHONG};
        use crate::ai::utility::score_occupy_enhanced;

        let tile_levels = BTreeMap::new();

        let mut liaodong_faction = create_faction();
        liaodong_faction.personality = LIAODONG;

        let mut nanzhong_faction = create_faction();
        nanzhong_faction.personality = NANZHONG;

        let liaodong_id = "faction_liaodong".to_string();
        let nanzhong_id = "faction_nanzhong".to_string();

        let liaodong_score = score_occupy_enhanced(
            HexCoord::new(10, 10),
            &liaodong_faction,
            &BTreeMap::new(),
            &tile_levels,
            &liaodong_id,
            0,
        );

        let nanzhong_score = score_occupy_enhanced(
            HexCoord::new(10, 10),
            &nanzhong_faction,
            &BTreeMap::new(),
            &tile_levels,
            &nanzhong_id,
            0,
        );

        assert!(
            liaodong_score > nanzhong_score,
            "Liaodong should expand more aggressively than Nanzhong: liaodong={liaodong_score}, nanzhong={nanzhong_score}"
        );
    }
}
