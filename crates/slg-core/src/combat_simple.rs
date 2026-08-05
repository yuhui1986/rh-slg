//! 简化版战斗系统 (M0 战斗)
//!
//! 率土核心：派兵到邻接敌方格 → 战报 → 胜/败 → 占据/撤退。
//!
//! M0 简化（与完整 combat.rs 区别）：
//! - **没有武将**（M7 才有）：attacker 单兵 = 100 (TROOPS_PER_MARCH)
//! - **没有兵种克制 / 战法**（M7 才有）
//! - **没有士气 / 撤退判定**（简化）
//! - **defender 静态防御值** per terrain（不是动态部队数）
//!
//! 公式（attacker vs defender）：
//! - atk_strength = attacker_troops × terrain_attack_mod
//! - def_strength = defender_troops × terrain_defense_mod
//! - atk > def * 1.5 → Victory
//! - atk * 1.5 < def → Defeat
//! - 其它 → Draw
//!
//! 后果：
//! - Victory: attacker 占据目标格 + 损失 50% 兵
//! - Defeat: attacker troops 归零（行军失败）
//! - Draw: 双方都扣 25% 兵，目标格不变
//!
//! **M7 武将系统上线后，应该改用 `crate::rule::combat::simulate()`**。

use serde::{Deserialize, Serialize};

use crate::map::tile::TerrainType;

// ---------------------------------------------------------------------------
// CombatResult
// ---------------------------------------------------------------------------

/// 战斗结果
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CombatResult {
    /// 攻方胜：占据目标格
    Victory,
    /// 攻方败：行军失败
    Defeat,
    /// 平局：双方都扣兵，目标格不变
    Draw,
}

// ---------------------------------------------------------------------------
// 地形修正
// ---------------------------------------------------------------------------

/// 地形对 attacker 的攻击修正
fn terrain_attack_mod(terrain: TerrainType) -> f64 {
    match terrain {
        TerrainType::Plains => 1.0,
        TerrainType::Mountain => 0.9,
        TerrainType::Forest => 0.95,
        TerrainType::Hills => 0.95,
        TerrainType::Desert => 0.9,
        TerrainType::Swamp => 0.85,
        TerrainType::Pass => 0.85,
        TerrainType::Water => 0.8,
    }
}

/// 地形对 defender 的防御修正
fn terrain_defense_mod(terrain: TerrainType) -> f64 {
    match terrain {
        TerrainType::Plains => 1.0,
        TerrainType::Mountain => 1.2,
        TerrainType::Forest => 1.1,
        TerrainType::Hills => 1.15,
        TerrainType::Desert => 1.0,
        TerrainType::Swamp => 1.05,
        TerrainType::Pass => 1.3,
        TerrainType::Water => 1.0,
    }
}

// ---------------------------------------------------------------------------
// 静态防御值（每种地形的 defender 基础 troops）
// ---------------------------------------------------------------------------

/// M0 静态防御值：每个地形格上有多少 defender troops
///
/// 没有动态部队数（M7 才有），用地形静态值代表"驻防力量"。
///
/// 玩家点击邻接敌方格时，defender = target 地形的静态值。
/// 攻方 = TROOPS_PER_MARCH (100)。
pub fn static_defender_troops(terrain: TerrainType) -> u32 {
    match terrain {
        TerrainType::Plains => 50,
        TerrainType::Hills => 80,
        TerrainType::Forest => 80,
        TerrainType::Mountain => 120,
        TerrainType::Desert => 50,
        TerrainType::Swamp => 60,
        TerrainType::Pass => 200,
        TerrainType::Water => 30, // 水域驻防弱
    }
}

// ---------------------------------------------------------------------------
// 战斗结算
// ---------------------------------------------------------------------------

/// 战斗结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimpleCombatReport {
    pub result: CombatResult,
    /// 攻方剩余 troops
    pub attacker_remaining: u32,
    /// 守方剩余 troops
    pub defender_remaining: u32,
}

/// 简化版战斗结算
///
/// # 参数
/// - `attacker_troops`: 攻方出兵数（M0 固定 100）
/// - `defender_troops`: 守方静态防御值（用 `static_defender_troops(terrain)`）
/// - `terrain`: 战斗所在的地形
///
/// # 公式
/// - atk_strength = attacker_troops × terrain_attack_mod(terrain)
/// - def_strength = defender_troops × terrain_defense_mod(terrain)
/// - 胜负: atk > def * 1.5 → Victory; atk * 1.5 < def → Defeat; 其它 → Draw
///
/// # 损失
/// - Victory: 攻方 50% 兵 / 守方 0 (全灭)
/// - Defeat: 攻方 0 (全灭) / 守方 25% (反扑)
/// - Draw: 攻方 25% / 守方 25%
pub fn resolve_simple_combat(
    attacker_troops: u32,
    defender_troops: u32,
    terrain: TerrainType,
) -> SimpleCombatReport {
    let atk_strength = attacker_troops as f64 * terrain_attack_mod(terrain);
    let def_strength = defender_troops as f64 * terrain_defense_mod(terrain);

    let (result, atk_remaining, def_remaining) = if atk_strength > def_strength * 1.5 {
        (CombatResult::Victory, attacker_troops / 2, 0u32)
    } else if atk_strength * 1.5 < def_strength {
        (CombatResult::Defeat, 0u32, defender_troops - defender_troops / 4)
    } else {
        (CombatResult::Draw, attacker_troops - attacker_troops / 4, defender_troops - defender_troops / 4)
    };

    SimpleCombatReport {
        result,
        attacker_remaining: atk_remaining,
        defender_remaining: def_remaining,
    }
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_static_defender_troops_per_terrain() {
        assert_eq!(static_defender_troops(TerrainType::Plains), 50);
        assert_eq!(static_defender_troops(TerrainType::Mountain), 120);
        assert_eq!(static_defender_troops(TerrainType::Pass), 200);
    }

    #[test]
    fn test_attacker_100_vs_plains_50_victory() {
        // 攻 100 平原 vs 守 50 平原
        // atk = 100 * 1.0 = 100
        // def = 50 * 1.0 = 50
        // 100 > 50 * 1.5 = 75 → Victory
        let report = resolve_simple_combat(100, 50, TerrainType::Plains);
        assert_eq!(report.result, CombatResult::Victory);
        assert_eq!(report.attacker_remaining, 50);
        assert_eq!(report.defender_remaining, 0);
    }

    #[test]
    fn test_attacker_100_vs_mountain_120_defeat() {
        // 攻 100 山地 vs 守 120 山地
        // atk = 100 * 0.9 = 90
        // def = 120 * 1.2 = 144
        // 90 * 1.5 = 135 < 144 → Defeat
        let report = resolve_simple_combat(100, 120, TerrainType::Mountain);
        assert_eq!(report.result, CombatResult::Defeat);
        assert_eq!(report.attacker_remaining, 0);
        // 守方反扑: 120 - 30 = 90
        assert_eq!(report.defender_remaining, 90);
    }

    #[test]
    fn test_attacker_100_vs_plains_50_draw() {
        // 攻 100 平原 vs 守 50 平原 → 不该 draw
        // atk = 100, def = 50, atk > def * 1.5 = 75 → Victory
        // 改为 atk = 100, def = 80 → atk < def * 1.5 = 120, atk * 1.5 = 150 > 80 → Draw
        let report = resolve_simple_combat(100, 80, TerrainType::Plains);
        assert_eq!(report.result, CombatResult::Draw);
        // 双方都扣 25%
        assert_eq!(report.attacker_remaining, 75); // 100 - 25
        assert_eq!(report.defender_remaining, 60); // 80 - 20
    }

    #[test]
    fn test_attacker_100_vs_pass_200_defeat() {
        // 关隘驻防 200，攻方 100 必败
        let report = resolve_simple_combat(100, 200, TerrainType::Pass);
        assert_eq!(report.result, CombatResult::Defeat);
        assert_eq!(report.attacker_remaining, 0);
    }

    #[test]
    fn test_terrain_modifier_advantage() {
        // 攻方在平原 vs 山地守军
        // atk = 100 * 1.0 = 100 (plains atk)
        // def = 100 * 1.2 = 120 (mountain def)
        // 100 < 120 * 1.5 = 180 → 100 * 1.5 = 150 > 120 → Draw (小幅吃亏但不是完败)
        let report = resolve_simple_combat(100, 100, TerrainType::Mountain);
        // atk 100 * 1.0 = 100, def 100 * 1.2 = 120
        // 100 < 120, 100 * 1.5 = 150 > 120 → Draw
        assert_eq!(report.result, CombatResult::Draw);
    }
}
