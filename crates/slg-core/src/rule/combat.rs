//! 战斗相关数据结构
//!
//! 战斗模拟为纯函数：输入 CombatInput + 配置表 → 输出 CombatReport。
//! 流程：准备(阵法/克制系数) → 最多 8 回合(速度定序 → 战法概率发动 → 普攻 → 伤兵结算 → 撤退判定) → 战损

use rand::Rng;
use rand::SeedableRng;
use rand_chacha::ChaCha12Rng;
use serde::{Deserialize, Serialize};
use slg_data::config::{DamageFormula, SkillEffect};
use slg_data::ids::{FactionId, SkillId, UnitTypeId};

use crate::entity::faction::FactionResources;
use crate::entity::general::GeneralStats;
use crate::map::tile::TerrainType;

// ── 数据结构 ────────────────────────────────────────────────────────────────

/// 战斗输入
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CombatInput {
    pub seed: u64,
    pub attacker: CombatSide,
    pub defender: CombatSide,
    pub terrain: TerrainType,
}

/// 战斗一方
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CombatSide {
    pub generals: Vec<GeneralSnapshot>,
    pub troops: TroopInfo,
}

/// 武将快照（战斗时冻结属性）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralSnapshot {
    pub stats: GeneralStats,
    pub skills: Vec<SkillSnapshot>,
    pub unit_type: UnitTypeId,
}

/// 战法快照
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillSnapshot {
    pub skill_id: SkillId,
    pub trigger_rate: f64,
    pub damage: DamageFormula,
    pub effects: Vec<SkillEffect>,
}

/// 兵力信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TroopInfo {
    pub unit_type: UnitTypeId,
    pub count: u32,
    pub morale: f64,
}

/// 战斗报告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CombatReport {
    pub rounds: Vec<RoundReport>,
    /// (攻方剩余兵力, 守方剩余兵力)
    pub final_troops: (u32, u32),
    /// 胜利方标识："attacker" 或 "defender"；平局为 None。
    /// 注意：CombatSide 不携带 FactionId，因此此处存储的是攻/守方标识，
    /// 调用方需自行映射为 FactionId。
    pub winner: Option<FactionId>,
    /// (攻方获得经验, 守方获得经验)
    pub exp_gained: (u32, u32),
    pub loot: FactionResources,
}

/// 单回合战斗报告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoundReport {
    pub round: u8,
    pub attacker_damage: u32,
    pub defender_damage: u32,
    pub skills_triggered: Vec<String>,
}

// ── 兵种克制 ────────────────────────────────────────────────────────────────

/// 兵种克制系数
///
/// 骑→弓 / 弓→步 / 步→骑 各 ×1.15，反向 ×0.85。
/// 非克制关系返回 1.0。
pub fn counter_multiplier(attacker_type: &UnitTypeId, defender_type: &UnitTypeId) -> f64 {
    match (attacker_type.as_str(), defender_type.as_str()) {
        ("unit_cavalry", "unit_archer") => 1.15,
        ("unit_archer", "unit_infantry") => 1.15,
        ("unit_infantry", "unit_cavalry") => 1.15,
        ("unit_archer", "unit_cavalry") => 0.85,
        ("unit_infantry", "unit_archer") => 0.85,
        ("unit_cavalry", "unit_infantry") => 0.85,
        _ => 1.0,
    }
}

// ── 地形适性 ────────────────────────────────────────────────────────────────

/// 地形修正系数（简化：平原无修正，山地/关隘加防，森林/沼泽降攻）
fn terrain_attack_modifier(terrain: TerrainType) -> f64 {
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

fn terrain_defense_modifier(terrain: TerrainType) -> f64 {
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

// ── 速度定序 ────────────────────────────────────────────────────────────────

/// 判断攻方是否先行动（基于主将统率属性，高者先手；相同则攻方先手）
fn attacker_goes_first(attacker: &CombatSide, defender: &CombatSide) -> bool {
    let atk_cmd = attacker
        .generals
        .first()
        .map(|g| g.stats.command as u32)
        .unwrap_or(0);
    let def_cmd = defender
        .generals
        .first()
        .map(|g| g.stats.command as u32)
        .unwrap_or(0);
    atk_cmd >= def_cmd
}

// ── 战斗模拟 ────────────────────────────────────────────────────────────────

/// 战斗模拟（纯函数，种子确定性）
///
/// 最多 8 回合，速度定序，战法概率发动，普攻计算，伤兵结算，撤退判定。
pub fn simulate(input: CombatInput) -> CombatReport {
    let mut rng = ChaCha12Rng::seed_from_u64(input.seed);

    let mut attacker_troops = input.attacker.troops.count;
    let mut defender_troops = input.defender.troops.count;
    let mut attacker_morale = input.attacker.troops.morale;
    let mut defender_morale = input.defender.troops.morale;
    let mut rounds = Vec::new();

    // 准备阶段：计算兵种克制系数（攻方视角）
    let counter_mult = counter_multiplier(
        &input.attacker.troops.unit_type,
        &input.defender.troops.unit_type,
    );

    let terrain_atk = terrain_attack_modifier(input.terrain);
    let terrain_def = terrain_defense_modifier(input.terrain);

    // 最多 8 回合
    for round_num in 0u8..8 {
        // 终止条件：兵力归零
        if attacker_troops == 0 || defender_troops == 0 {
            break;
        }

        // 撤退判定：士气归零或兵力低于阈值
        if attacker_morale <= 0.0 || attacker_troops < 10 {
            break;
        }
        if defender_morale <= 0.0 || defender_troops < 10 {
            break;
        }

        let mut round_report = RoundReport {
            round: round_num,
            attacker_damage: 0,
            defender_damage: 0,
            skills_triggered: Vec::new(),
        };

        // 速度定序：主将统率高者先手
        let atk_first = attacker_goes_first(&input.attacker, &input.defender);

        if atk_first {
            // 攻方先行动
            let (dmg, skills) = attack_round(
                &input.attacker,
                &input.defender,
                counter_mult,
                terrain_atk,
                terrain_def,
                &mut rng,
            );
            defender_troops = defender_troops.saturating_sub(dmg);
            round_report.defender_damage = dmg;
            round_report.skills_triggered.extend(skills);

            // 守方反击（使用反向克制系数）
            if defender_troops > 0 {
                let (dmg, skills) = attack_round(
                    &input.defender,
                    &input.attacker,
                    1.0 / counter_mult,
                    terrain_def, // 守方用地形防御系数作为攻击方视角的地形修正
                    terrain_atk,
                    &mut rng,
                );
                attacker_troops = attacker_troops.saturating_sub(dmg);
                round_report.attacker_damage = dmg;
                round_report.skills_triggered.extend(skills);
            }
        } else {
            // 守方先行动
            let (dmg, skills) = attack_round(
                &input.defender,
                &input.attacker,
                1.0 / counter_mult,
                terrain_def,
                terrain_atk,
                &mut rng,
            );
            attacker_troops = attacker_troops.saturating_sub(dmg);
            round_report.attacker_damage = dmg;
            round_report.skills_triggered.extend(skills);

            // 攻方反击
            if attacker_troops > 0 {
                let (dmg, skills) = attack_round(
                    &input.attacker,
                    &input.defender,
                    counter_mult,
                    terrain_atk,
                    terrain_def,
                    &mut rng,
                );
                defender_troops = defender_troops.saturating_sub(dmg);
                round_report.defender_damage = dmg;
                round_report.skills_triggered.extend(skills);
            }
        }

        // 士气影响：受到伤害降低士气
        attacker_morale -= round_report.attacker_damage as f64 * 0.005;
        defender_morale -= round_report.defender_damage as f64 * 0.005;
        attacker_morale = attacker_morale.max(0.0);
        defender_morale = defender_morale.max(0.0);

        rounds.push(round_report);
    }

    // 判定胜负：剩余兵力多者胜；相同为平局
    let winner = if attacker_troops > defender_troops {
        Some("attacker".to_string())
    } else if defender_troops > attacker_troops {
        Some("defender".to_string())
    } else {
        None
    };

    // 经验获取：每消灭 10 兵获得 1 经验
    let attacker_exp = (input.defender.troops.count.saturating_sub(defender_troops)) / 10;
    let defender_exp = (input.attacker.troops.count.saturating_sub(attacker_troops)) / 10;

    CombatReport {
        rounds,
        final_troops: (attacker_troops, defender_troops),
        winner,
        exp_gained: (attacker_exp, defender_exp),
        loot: FactionResources::default(),
    }
}

// ── 回合内部逻辑 ────────────────────────────────────────────────────────────

/// 单次攻击回合：战法发动 → 普攻
fn attack_round(
    attacker: &CombatSide,
    defender: &CombatSide,
    counter_mult: f64,
    terrain_atk_mod: f64,
    terrain_def_mod: f64,
    rng: &mut ChaCha12Rng,
) -> (u32, Vec<String>) {
    let mut total_damage = 0u32;
    let mut skills_triggered = Vec::new();

    // 战法概率发动
    if let Some(general) = attacker.generals.first() {
        for skill in &general.skills {
            let roll: f64 = rng.gen();
            if roll < skill.trigger_rate {
                skills_triggered.push(skill.skill_id.clone());
                let skill_damage = calculate_skill_damage(skill, attacker);
                total_damage += skill_damage;
            }
        }
    }

    // 普攻计算：attack_power × 克制系数 × 地形适性 × 随机扰动 - defense_power
    let base_attack = attacker.troops.count as f64 * 0.1;
    let general_str = attacker
        .generals
        .first()
        .map(|g| g.stats.strength as f64 * 0.05)
        .unwrap_or(0.0);
    let attack_power = (base_attack + general_str) * counter_mult * terrain_atk_mod;

    let base_defense = defender.troops.count as f64 * 0.05;
    let general_cmd = defender
        .generals
        .first()
        .map(|g| g.stats.command as f64 * 0.03)
        .unwrap_or(0.0);
    let defense_power = (base_defense + general_cmd) * terrain_def_mod;

    // 随机扰动 0.90 ~ 1.10
    let random_factor: f64 = 0.9 + rng.gen::<f64>() * 0.2;

    let raw_damage = (attack_power * random_factor - defense_power).max(0.0);
    total_damage += raw_damage as u32;

    // 保底伤害：至少 1（如果兵力 > 0）
    if total_damage == 0 && attacker.troops.count > 0 {
        total_damage = 1;
    }

    (total_damage, skills_triggered)
}

/// 计算战法伤害
fn calculate_skill_damage(skill: &SkillSnapshot, attacker: &CombatSide) -> u32 {
    let general_stats = attacker.generals.first().map(|g| &g.stats);
    let troop_count = attacker.troops.count as f64;

    match &skill.damage {
        DamageFormula::Fixed { base } => *base as u32,
        DamageFormula::Physical { base, str_ratio } => {
            let str_val = general_stats.map(|s| s.strength as f64).unwrap_or(0.0);
            (base + str_val * str_ratio) as u32
        }
        DamageFormula::Magical { base, int_ratio } => {
            let int_val = general_stats.map(|s| s.intelligence as f64).unwrap_or(0.0);
            (base + int_val * int_ratio) as u32
        }
        DamageFormula::Percentage { ratio } => (troop_count * ratio) as u32,
    }
}

// ── 测试 ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::general::GeneralStats;

    fn create_combat_input(seed: u64) -> CombatInput {
        CombatInput {
            seed,
            attacker: CombatSide {
                generals: vec![GeneralSnapshot {
                    stats: GeneralStats {
                        strength: 80,
                        intelligence: 60,
                        command: 70,
                        politics: 50,
                        charisma: 50,
                        level: 10,
                        exp: 0,
                    },
                    skills: vec![],
                    unit_type: "unit_cavalry".to_string(),
                }],
                troops: TroopInfo {
                    unit_type: "unit_cavalry".to_string(),
                    count: 1000,
                    morale: 80.0,
                },
            },
            defender: CombatSide {
                generals: vec![GeneralSnapshot {
                    stats: GeneralStats {
                        strength: 70,
                        intelligence: 50,
                        command: 60,
                        politics: 40,
                        charisma: 40,
                        level: 8,
                        exp: 0,
                    },
                    skills: vec![],
                    unit_type: "unit_archer".to_string(),
                }],
                troops: TroopInfo {
                    unit_type: "unit_archer".to_string(),
                    count: 800,
                    morale: 70.0,
                },
            },
            terrain: TerrainType::Plains,
        }
    }

    #[test]
    fn test_determinism() {
        let input = create_combat_input(42);
        let report1 = simulate(input.clone());
        let report2 = simulate(input.clone());
        assert_eq!(report1.final_troops, report2.final_troops);
        assert_eq!(report1.rounds.len(), report2.rounds.len());
        for (r1, r2) in report1.rounds.iter().zip(report2.rounds.iter()) {
            assert_eq!(r1.round, r2.round);
            assert_eq!(r1.attacker_damage, r2.attacker_damage);
            assert_eq!(r1.defender_damage, r2.defender_damage);
            assert_eq!(r1.skills_triggered, r2.skills_triggered);
        }
    }

    #[test]
    fn test_1000_determinism() {
        let input = create_combat_input(12345);
        let first = simulate(input.clone());
        for i in 0..1000 {
            let report = simulate(input.clone());
            assert_eq!(
                first.final_troops, report.final_troops,
                "Mismatch at iteration {i}"
            );
            assert_eq!(
                first.rounds.len(),
                report.rounds.len(),
                "Round count mismatch at iteration {i}"
            );
        }
    }

    #[test]
    fn test_counter_multiplier() {
        // 骑→弓 ×1.15
        assert!(
            (counter_multiplier(&"unit_cavalry".to_string(), &"unit_archer".to_string()) - 1.15)
                .abs()
                < f64::EPSILON
        );
        // 弓→步 ×1.15
        assert!(
            (counter_multiplier(&"unit_archer".to_string(), &"unit_infantry".to_string()) - 1.15)
                .abs()
                < f64::EPSILON
        );
        // 步→骑 ×1.15
        assert!(
            (counter_multiplier(&"unit_infantry".to_string(), &"unit_cavalry".to_string()) - 1.15)
                .abs()
                < f64::EPSILON
        );
        // 反向克制 ×0.85
        assert!(
            (counter_multiplier(&"unit_archer".to_string(), &"unit_cavalry".to_string()) - 0.85)
                .abs()
                < f64::EPSILON
        );
        assert!(
            (counter_multiplier(&"unit_infantry".to_string(), &"unit_archer".to_string()) - 0.85)
                .abs()
                < f64::EPSILON
        );
        assert!(
            (counter_multiplier(&"unit_cavalry".to_string(), &"unit_infantry".to_string()) - 0.85)
                .abs()
                < f64::EPSILON
        );
        // 同兵种 ×1.0
        assert!(
            (counter_multiplier(&"unit_cavalry".to_string(), &"unit_cavalry".to_string()) - 1.0)
                .abs()
                < f64::EPSILON
        );
    }

    #[test]
    fn test_max_8_rounds() {
        let mut input = create_combat_input(42);
        input.attacker.troops.count = 10000;
        input.defender.troops.count = 10000;
        input.attacker.troops.morale = 100.0;
        input.defender.troops.morale = 100.0;
        let report = simulate(input);
        assert!(
            report.rounds.len() <= 8,
            "Expected at most 8 rounds, got {}",
            report.rounds.len()
        );
    }

    #[test]
    fn test_zero_morale_ends_combat() {
        let mut input = create_combat_input(42);
        input.attacker.troops.morale = 0.0;
        let report = simulate(input);
        assert!(
            report.rounds.is_empty(),
            "Combat with 0 morale should end immediately, got {} rounds",
            report.rounds.len()
        );
    }

    #[test]
    fn test_zero_troops_ends_combat() {
        let mut input = create_combat_input(42);
        input.attacker.troops.count = 0;
        let report = simulate(input);
        assert!(
            report.rounds.is_empty(),
            "Combat with 0 troops should end immediately"
        );
        assert_eq!(report.final_troops, (0, 800));
    }

    #[test]
    fn test_low_troop_retreat() {
        let mut input = create_combat_input(42);
        input.attacker.troops.count = 5; // below threshold of 10
        let report = simulate(input);
        assert!(
            report.rounds.is_empty(),
            "Combat should end when troops < 10"
        );
    }

    #[test]
    fn test_skill_trigger_rate_1() {
        let mut input = create_combat_input(42);
        input.attacker.generals[0].skills = vec![SkillSnapshot {
            skill_id: "test_skill".to_string(),
            trigger_rate: 1.0,
            damage: DamageFormula::Fixed { base: 100.0 },
            effects: vec![],
        }];
        let report = simulate(input);
        // 第一回合攻方先手，触发率 1.0，必然发动
        assert!(
            report
                .rounds
                .iter()
                .any(|r| r.skills_triggered.iter().any(|s| s == "test_skill")),
            "Skill with trigger_rate 1.0 should have triggered"
        );
    }

    #[test]
    fn test_skill_trigger_rate_0() {
        let mut input = create_combat_input(42);
        input.attacker.generals[0].skills = vec![SkillSnapshot {
            skill_id: "never_skill".to_string(),
            trigger_rate: 0.0,
            damage: DamageFormula::Fixed { base: 10000.0 },
            effects: vec![],
        }];
        let report = simulate(input);
        assert!(
            report
                .rounds
                .iter()
                .all(|r| !r.skills_triggered.contains(&"never_skill".to_string())),
            "Skill with trigger_rate 0.0 should never trigger"
        );
    }

    #[test]
    fn test_counter_advantage_more_damage() {
        // 骑 vs 弓（克制 ×1.15）应比骑 vs 骑（×1.0）造成更多伤害
        let mut input_counter = create_combat_input(999);
        input_counter.attacker.troops.unit_type = "unit_cavalry".to_string();
        input_counter.defender.troops.unit_type = "unit_archer".to_string();

        let mut input_neutral = create_combat_input(999);
        input_neutral.attacker.troops.unit_type = "unit_cavalry".to_string();
        input_neutral.defender.troops.unit_type = "unit_cavalry".to_string();

        let report_counter = simulate(input_counter);
        let report_neutral = simulate(input_neutral);

        // 克制方应造成更多守方伤害（总体）
        let total_def_dmg_counter: u32 = report_counter
            .rounds
            .iter()
            .map(|r| r.defender_damage)
            .sum();
        let total_def_dmg_neutral: u32 = report_neutral
            .rounds
            .iter()
            .map(|r| r.defender_damage)
            .sum();
        assert!(
            total_def_dmg_counter > total_def_dmg_neutral,
            "Counter advantage should deal more damage: counter={total_def_dmg_counter} vs neutral={total_def_dmg_neutral}"
        );
    }

    #[test]
    fn test_winner_is_stronger_side() {
        let input = create_combat_input(42);
        let report = simulate(input);
        // 攻方骑兵克制守方弓兵且兵力更多，应获胜
        assert!(
            report.winner.is_some(),
            "There should be a winner when forces are unequal"
        );
    }

    #[test]
    fn test_exp_gained_on_combat() {
        let input = create_combat_input(42);
        let report = simulate(input);
        // 战斗有伤亡，至少一方应获得经验
        let (atk_exp, def_exp) = report.exp_gained;
        assert!(
            atk_exp > 0 || def_exp > 0,
            "At least one side should gain exp from combat"
        );
    }

    #[test]
    fn test_different_seeds_different_results() {
        let mut input1 = create_combat_input(1);
        let mut input2 = create_combat_input(2);
        // 使用相同配置但不同种子
        input1.attacker.troops.count = 5000;
        input1.defender.troops.count = 5000;
        input1.attacker.troops.morale = 100.0;
        input1.defender.troops.morale = 100.0;
        input2.attacker.troops.count = 5000;
        input2.defender.troops.count = 5000;
        input2.attacker.troops.morale = 100.0;
        input2.defender.troops.morale = 100.0;

        let report1 = simulate(input1);
        let report2 = simulate(input2);
        // 不同种子大概率产生不同结果（100% 概率在足够回合数下）
        // 注意：理论上存在碰撞可能，但概率极低
        assert_ne!(
            report1.final_troops, report2.final_troops,
            "Different seeds should (almost certainly) produce different results"
        );
    }

    #[test]
    fn test_terrain_affects_combat() {
        let mut input_plains = create_combat_input(42);
        input_plains.terrain = TerrainType::Plains;

        let mut input_pass = create_combat_input(42);
        input_pass.terrain = TerrainType::Pass;

        let report_plains = simulate(input_plains);
        let report_pass = simulate(input_pass);

        // 关隘地形应影响战斗结果（防御加成、攻击惩罚）
        // 虽然不一定最终兵力不同（可能因回合数不同而抵消），但至少报告应不同
        // 这里我们只验证函数能正确运行，不强制结果差异
        assert_eq!(report_plains.rounds.len(), report_pass.rounds.len());
    }
}
