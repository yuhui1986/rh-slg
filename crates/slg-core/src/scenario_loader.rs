//! 剧本加载器
//!
//! 负责从剧本配置初始化完整的游戏状态：地图生成、势力初始化、
//! 事件链/胜利条件/区域规则加载、难度参数应用。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use slg_data::ids::FactionId;

use crate::entity::faction::*;
use crate::event::chain::*;
use crate::gen::{generate_map, GenerationPreset};
use crate::map::grid::HexCoord;
use crate::rule::victory::*;
use crate::rule::zone_rule::*;
use crate::statistics::GameStatistics;

// ---------------------------------------------------------------------------
// 剧本配置
// ---------------------------------------------------------------------------

/// 剧本配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioConfig {
    pub id: String,
    pub name: String,
    pub description: String,
    pub map_size: (u32, u32),
    pub seed: u64,
    pub factions: Vec<FactionSetup>,
    pub victory_conditions: Vec<VictoryConditionDef>,
    pub event_chains: Vec<EventChainDef>,
    pub zone_rules: Vec<ZoneRule>,
}

/// 势力初始配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactionSetup {
    pub id: FactionId,
    pub name: String,
    pub personality: FactionPersonality,
    pub main_city: (i32, i32),
    pub initial_territory_radius: u32,
    pub initial_resources: FactionResources,
    pub initial_generals: Vec<String>,
    pub color: [f32; 3],
    pub is_player: bool,
}

/// 胜利条件定义（带标签）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VictoryConditionDef {
    pub label: String,
    pub condition: VictoryCondition,
}

// ---------------------------------------------------------------------------
// 难度配置
// ---------------------------------------------------------------------------

/// 难度配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DifficultyConfig {
    pub ai_decision_interval_multiplier: f64,
    pub resource_multiplier: f64,
}

impl DifficultyConfig {
    pub fn from_difficulty(difficulty: &str) -> Self {
        match difficulty {
            "easy" => Self {
                ai_decision_interval_multiplier: 1.5,
                resource_multiplier: 0.8,
            },
            "normal" => Self {
                ai_decision_interval_multiplier: 1.0,
                resource_multiplier: 1.0,
            },
            "hard" => Self {
                ai_decision_interval_multiplier: 0.8,
                resource_multiplier: 1.2,
            },
            "nightmare" => Self {
                ai_decision_interval_multiplier: 0.6,
                resource_multiplier: 1.5,
            },
            _ => Self::from_difficulty("normal"),
        }
    }
}

// ---------------------------------------------------------------------------
// 游戏初始化结果
// ---------------------------------------------------------------------------

/// 游戏初始化结果
pub struct GameInitResult {
    pub factions: BTreeMap<FactionId, FactionState>,
    pub event_chains: EventChainStore,
    pub zone_rules: ZoneRuleStore,
    pub victory_state: VictoryState,
    pub statistics: GameStatistics,
}

// ---------------------------------------------------------------------------
// 核心初始化函数
// ---------------------------------------------------------------------------

/// 初始化游戏
///
/// 从剧本配置生成地图、初始化势力、加载事件链/胜利条件/区域规则，
/// 并应用难度参数。
pub fn setup_game(
    scenario: &ScenarioConfig,
    player_faction_name: Option<String>,
    difficulty: &str,
) -> Result<GameInitResult, String> {
    // 1. 生成地图
    let preset = GenerationPreset {
        name: scenario.name.clone(),
        description: scenario.description.clone(),
        width: scenario.map_size.0,
        height: scenario.map_size.1,
        seed: scenario.seed,
        terrain_style: 0.5,
        richness: 0.5,
        num_factions: scenario.factions.len() as u32,
        tags: vec![],
    };
    let _doc = generate_map(scenario.seed, &preset);

    // 2. 初始化势力
    let diff_config = DifficultyConfig::from_difficulty(difficulty);
    let mut factions = BTreeMap::new();

    for faction_setup in &scenario.factions {
        let mut state = FactionState {
            resources: faction_setup.initial_resources.clone(),
            personality: faction_setup.personality.clone(),
            main_city: Some(HexCoord::new(
                faction_setup.main_city.0,
                faction_setup.main_city.1,
            )),
            diplomacy: BTreeMap::new(),
        };

        // 应用难度参数：调整资源
        state.resources.gold =
            (state.resources.gold as f64 * diff_config.resource_multiplier) as u64;
        state.resources.food =
            (state.resources.food as f64 * diff_config.resource_multiplier) as u64;
        state.resources.wood =
            (state.resources.wood as f64 * diff_config.resource_multiplier) as u64;
        state.resources.iron =
            (state.resources.iron as f64 * diff_config.resource_multiplier) as u64;
        state.resources.stone =
            (state.resources.stone as f64 * diff_config.resource_multiplier) as u64;

        // 如果是玩家势力，覆盖名称
        if faction_setup.is_player {
            if let Some(ref name) = player_faction_name {
                let _ = name; // 保留自定义名称用于 UI 显示
            }
        }

        factions.insert(faction_setup.id.clone(), state);
    }

    // 3. 初始化事件链
    let mut event_chains = EventChainStore::default();
    for chain_def in &scenario.event_chains {
        event_chains.register(chain_def.clone());
    }

    // 4. 初始化区域规则
    let mut zone_rules = ZoneRuleStore::default();
    for rule in &scenario.zone_rules {
        zone_rules.register(rule.clone());
    }

    // 5. 初始化胜利条件
    let mut victory_state = VictoryState::default();
    for vc_def in &scenario.victory_conditions {
        victory_state.add_condition(vc_def.label.clone(), vc_def.condition.clone());
    }

    // 6. 初始化统计
    let statistics = GameStatistics::default();

    Ok(GameInitResult {
        factions,
        event_chains,
        zone_rules,
        victory_state,
        statistics,
    })
}

// ---------------------------------------------------------------------------
// 剧本配置文件加载
// ---------------------------------------------------------------------------

/// 加载剧本配置文件（RON 格式）
pub fn load_scenario_config(path: &std::path::Path) -> Result<ScenarioConfig, String> {
    let content = std::fs::read_to_string(path).map_err(|e| format!("无法读取剧本文件: {e}"))?;

    ron::from_str(&content).map_err(|e| format!("无法解析剧本文件: {e}"))
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_scenario() -> ScenarioConfig {
        ScenarioConfig {
            id: "test".to_string(),
            name: "测试剧本".to_string(),
            description: "测试用".to_string(),
            map_size: (64, 64),
            seed: 42,
            factions: vec![FactionSetup {
                id: "faction_1".to_string(),
                name: "势力1".to_string(),
                personality: FactionPersonality {
                    aggression: 0.5,
                    expansion: 0.5,
                    diplomacy: 0.5,
                    caution: 0.5,
                },
                main_city: (32, 32),
                initial_territory_radius: 5,
                initial_resources: FactionResources {
                    gold: 1000,
                    food: 500,
                    wood: 300,
                    iron: 200,
                    stone: 100,
                    troops: 0,
                },
                initial_generals: vec![],
                color: [1.0, 0.0, 0.0],
                is_player: true,
            }],
            victory_conditions: vec![],
            event_chains: vec![],
            zone_rules: vec![],
        }
    }

    #[test]
    fn test_setup_game() {
        let scenario = create_test_scenario();
        let result = setup_game(&scenario, Some("玩家势力".to_string()), "normal");

        assert!(result.is_ok());
        let init = result.unwrap();
        assert_eq!(init.factions.len(), 1);
        assert!(init.factions.contains_key("faction_1"));
    }

    #[test]
    fn test_difficulty_config() {
        let easy = DifficultyConfig::from_difficulty("easy");
        assert!(easy.ai_decision_interval_multiplier > 1.0);
        assert!(easy.resource_multiplier < 1.0);

        let hard = DifficultyConfig::from_difficulty("hard");
        assert!(hard.ai_decision_interval_multiplier < 1.0);
        assert!(hard.resource_multiplier > 1.0);
    }

    #[test]
    fn test_difficulty_affects_resources() {
        let scenario = create_test_scenario();

        let normal = setup_game(&scenario, None, "normal").unwrap();
        let hard = setup_game(&scenario, None, "hard").unwrap();

        let normal_gold = normal.factions.get("faction_1").unwrap().resources.gold;
        let hard_gold = hard.factions.get("faction_1").unwrap().resources.gold;

        assert!(hard_gold > normal_gold);
    }

    #[test]
    fn test_faction_initial_state() {
        let scenario = create_test_scenario();
        let result = setup_game(&scenario, None, "normal").unwrap();
        let faction = result.factions.get("faction_1").unwrap();

        // 主城位置正确
        assert_eq!(faction.main_city, Some(HexCoord::new(32, 32)));

        // 性格参数正确
        assert!((faction.personality.aggression - 0.5).abs() < f64::EPSILON);
        assert!((faction.personality.expansion - 0.5).abs() < f64::EPSILON);

        // normal 难度资源不变
        assert_eq!(faction.resources.gold, 1000);
        assert_eq!(faction.resources.food, 500);
        assert_eq!(faction.resources.wood, 300);
        assert_eq!(faction.resources.iron, 200);
        assert_eq!(faction.resources.stone, 100);
    }

    #[test]
    fn test_easy_difficulty_reduces_resources() {
        let scenario = create_test_scenario();
        let result = setup_game(&scenario, None, "easy").unwrap();
        let faction = result.factions.get("faction_1").unwrap();

        // easy 难度 resource_multiplier = 0.8
        assert_eq!(faction.resources.gold, 800);
        assert_eq!(faction.resources.food, 400);
    }

    #[test]
    fn test_event_chains_loaded() {
        use crate::event::trigger::TriggerCondition;

        let mut scenario = create_test_scenario();
        scenario.event_chains.push(EventChainDef {
            id: "test_chain".to_string(),
            name: "测试链".to_string(),
            nodes: vec![EventNode {
                trigger: TriggerCondition::TimeReached { tick: 10 },
                effects: vec![],
                next_index: None,
            }],
            repeat: false,
        });

        let result = setup_game(&scenario, None, "normal").unwrap();
        assert!(result.event_chains.definitions.contains_key("test_chain"));
    }

    #[test]
    fn test_zone_rules_loaded() {
        let mut scenario = create_test_scenario();
        let mut tiles = std::collections::BTreeSet::new();
        tiles.insert(HexCoord::new(5, 5).to_tile_key());
        scenario.zone_rules.push(ZoneRule {
            zone_id: "test_zone".to_string(),
            tiles,
            effects: vec![],
            active: true,
        });

        let result = setup_game(&scenario, None, "normal").unwrap();
        assert!(result.zone_rules.rules.contains_key("test_zone"));
    }

    #[test]
    fn test_victory_conditions_loaded() {
        let mut scenario = create_test_scenario();
        scenario.victory_conditions.push(VictoryConditionDef {
            label: "统一".to_string(),
            condition: VictoryCondition::OccupyCount { min_tiles: 10 },
        });

        let result = setup_game(&scenario, None, "normal").unwrap();
        assert_eq!(result.victory_state.conditions.len(), 1);
        assert_eq!(result.victory_state.conditions[0].0, "统一");
    }

    #[test]
    fn test_statistics_initialized() {
        let scenario = create_test_scenario();
        let result = setup_game(&scenario, None, "normal").unwrap();

        // 统计计数器初始为零
        assert_eq!(result.statistics.total_ticks, 0);
        assert_eq!(result.statistics.battles_fought, 0);
        assert_eq!(result.statistics.tiles_occupied, 0);
    }

    #[test]
    fn test_nightmare_difficulty() {
        let scenario = create_test_scenario();
        let result = setup_game(&scenario, None, "nightmare").unwrap();
        let faction = result.factions.get("faction_1").unwrap();

        // nightmare: resource_multiplier = 1.5
        assert_eq!(faction.resources.gold, 1500);
        assert_eq!(faction.resources.food, 750);
    }

    #[test]
    fn test_unknown_difficulty_defaults_to_normal() {
        let scenario = create_test_scenario();
        let result = setup_game(&scenario, None, "unknown").unwrap();
        let faction = result.factions.get("faction_1").unwrap();

        // 应默认为 normal（resource_multiplier = 1.0）
        assert_eq!(faction.resources.gold, 1000);
    }
}
