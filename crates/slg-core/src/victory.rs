//! 胜利/失败条件检测
//!
//! 率土地图游戏目标：
//! - **胜利**: 玩家占地比例 ≥ 阈值（默认 50%）
//! - **失败**: 玩家主城被推（主城格 owner ≠ 玩家）
//!
//! MVP 简化：只检测这两个条件。后期可加：消灭所有 NPC、达成特定剧情。
//!
//! 调用方在每 tick 调一次 `check_victory_and_defeat`，返回 `GameOverReason`。
//! `slg-app` 收到后设 `game_state.phase = GamePhase::GameOver` + 显示 UI。

use slg_data::ids::{FactionId, TileKey};

use crate::map::grid::HexCoord;
use crate::map::tile::TerrainType;

// ---------------------------------------------------------------------------
// GameOverReason
// ---------------------------------------------------------------------------

/// 游戏结束原因
#[derive(Debug, Clone, PartialEq)]
pub enum GameOverReason {
    /// 胜利：占地比例达到阈值
    Victory { tiles: u32, total: u32, ratio: f64 },
    /// 失败：主城被推
    Defeat { city: HexCoord, attacker: FactionId },
}

impl GameOverReason {
    pub fn is_victory(&self) -> bool {
        matches!(self, Self::Victory { .. })
    }

    pub fn reason_text(&self) -> String {
        match self {
            Self::Victory { tiles, total, ratio } => {
                format!("统一天下！占领 {} / {} 格 ({:.1}%)", tiles, total, ratio * 100.0)
            }
            Self::Defeat { city, attacker } => {
                format!("主城 ({},{}) 被 {} 攻陷", city.q, city.r, attacker)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Victory 阈值
// ---------------------------------------------------------------------------

/// 默认胜利阈值：占地 50%
pub const DEFAULT_VICTORY_RATIO: f64 = 0.5;

// ---------------------------------------------------------------------------
// 检测函数
// ---------------------------------------------------------------------------

/// 检查胜利条件：玩家占地 / 总可占地 ≥ threshold
///
/// `terrain_map` 必须包含所有 hex（决定 total = 非水域的格数）。
/// `owner_map` 是当前圈地。
pub fn check_victory(
    owner_map: &std::collections::BTreeMap<TileKey, FactionId>,
    terrain_map: &std::collections::BTreeMap<TileKey, TerrainType>,
    player_faction: &FactionId,
    threshold: f64,
) -> Option<GameOverReason> {
    // 总可占地 = 非水域格数
    let total: u32 = terrain_map
        .values()
        .filter(|t| !matches!(t, TerrainType::Water))
        .count() as u32;
    if total == 0 {
        return None; // 地图为空，不触发胜利
    }

    // 玩家占地
    let player_tiles: u32 = owner_map
        .values()
        .filter(|f| f == &player_faction)
        .count() as u32;

    let ratio = player_tiles as f64 / total as f64;
    if ratio >= threshold {
        Some(GameOverReason::Victory {
            tiles: player_tiles,
            total,
            ratio,
        })
    } else {
        None
    }
}

/// 检查失败条件：玩家主城被 NPC 推
///
/// `territory` 包含 main_cities + owner_map
pub fn check_defeat(
    main_city: HexCoord,
    owner_map: &std::collections::BTreeMap<TileKey, FactionId>,
    player_faction: &FactionId,
) -> Option<GameOverReason> {
    let key = main_city.to_tile_key();
    match owner_map.get(&key) {
        Some(owner) if owner != player_faction => Some(GameOverReason::Defeat {
            city: main_city,
            attacker: owner.clone(),
        }),
        _ => None,
    }
}

/// 一次性检查胜利 + 失败，返回最早触发的那一个
pub fn check_victory_and_defeat(
    main_city: HexCoord,
    owner_map: &std::collections::BTreeMap<TileKey, FactionId>,
    terrain_map: &std::collections::BTreeMap<TileKey, TerrainType>,
    player_faction: &FactionId,
    victory_threshold: f64,
) -> Option<GameOverReason> {
    // 失败优先（主城被推 > 胜利）
    if let Some(defeat) = check_defeat(main_city, owner_map, player_faction) {
        return Some(defeat);
    }
    if let Some(victory) = check_victory(owner_map, terrain_map, player_faction, victory_threshold) {
        return Some(victory);
    }
    None
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn key(q: i32, r: i32) -> TileKey {
        HexCoord::new(q, r).to_tile_key()
    }

    fn make_terrain_map(spec: &[(i32, i32, TerrainType)]) -> BTreeMap<TileKey, TerrainType> {
        spec.iter()
            .map(|(q, r, t)| (key(*q, *r), *t))
            .collect()
    }

    fn make_owner_map(spec: &[(i32, i32, &str)]) -> BTreeMap<TileKey, FactionId> {
        spec.iter()
            .map(|(q, r, f)| (key(*q, *r), f.to_string()))
            .collect()
    }

    #[test]
    fn test_victory_no_triggers() {
        // 4 格，玩家占 1 格，比例 25% < 50%
        let terrain = make_terrain_map(&[
            (0, 0, TerrainType::Plains),
            (1, 0, TerrainType::Plains),
            (2, 0, TerrainType::Plains),
            (3, 0, TerrainType::Plains),
        ]);
        let owners = make_owner_map(&[(0, 0, "faction_1")]);
        let result = check_victory(&owners, &terrain, &"faction_1".to_string(), 0.5);
        assert!(result.is_none(), "25% 不到 50% 阈值");
    }

    #[test]
    fn test_victory_meets_threshold() {
        // 4 格，玩家占 2 格 = 50%
        let terrain = make_terrain_map(&[
            (0, 0, TerrainType::Plains),
            (1, 0, TerrainType::Plains),
            (2, 0, TerrainType::Plains),
            (3, 0, TerrainType::Plains),
        ]);
        let owners = make_owner_map(&[
            (0, 0, "faction_1"),
            (1, 0, "faction_1"),
            (2, 0, "faction_2"),
            (3, 0, "faction_2"),
        ]);
        let result = check_victory(&owners, &terrain, &"faction_1".to_string(), 0.5);
        assert!(result.is_some(), "50% 触发胜利");
        match result.unwrap() {
            GameOverReason::Victory { tiles, total, ratio } => {
                assert_eq!(tiles, 2);
                assert_eq!(total, 4);
                assert!((ratio - 0.5).abs() < 0.01);
            }
            _ => panic!("应该是 Victory"),
        }
    }

    #[test]
    fn test_victory_excludes_water() {
        // 5 格，1 格水域，玩家占 2/4 = 50% (水域不计入)
        let terrain = make_terrain_map(&[
            (0, 0, TerrainType::Plains),
            (1, 0, TerrainType::Plains),
            (2, 0, TerrainType::Plains),
            (3, 0, TerrainType::Plains),
            (4, 0, TerrainType::Water),
        ]);
        let owners = make_owner_map(&[
            (0, 0, "faction_1"),
            (1, 0, "faction_1"),
            (2, 0, "faction_2"),
            (3, 0, "faction_2"),
        ]);
        let result = check_victory(&owners, &terrain, &"faction_1".to_string(), 0.5);
        assert!(result.is_some(), "玩家 2/4 = 50% 触发胜利（水域不计）");
    }

    #[test]
    fn test_defeat_city_unchanged() {
        // 玩家主城是 faction_1 的 -> 没失败
        let owners = make_owner_map(&[(0, 0, "faction_1")]);
        let result = check_defeat(HexCoord::new(0, 0), &owners, &"faction_1".to_string());
        assert!(result.is_none());
    }

    #[test]
    fn test_defeat_city_captured() {
        // 玩家主城被 faction_2 占
        let owners = make_owner_map(&[(0, 0, "faction_2")]);
        let result = check_defeat(HexCoord::new(0, 0), &owners, &"faction_1".to_string());
        assert!(result.is_some());
        match result.unwrap() {
            GameOverReason::Defeat { city, attacker } => {
                assert_eq!(city, HexCoord::new(0, 0));
                assert_eq!(attacker, "faction_2");
            }
            _ => panic!("应该是 Defeat"),
        }
    }

    #[test]
    fn test_check_both_defeat_priority() {
        // 玩家占 50% 触发胜利 AND 主城被推 -> 失败优先
        let terrain = make_terrain_map(&[
            (0, 0, TerrainType::Plains),
            (1, 0, TerrainType::Plains),
            (2, 0, TerrainType::Plains),
            (3, 0, TerrainType::Plains),
        ]);
        let owners = make_owner_map(&[
            (0, 0, "faction_2"), // 主城被 faction_2 推
            (1, 0, "faction_1"),
            (2, 0, "faction_1"),
            (3, 0, "faction_2"),
        ]);
        let result = check_victory_and_defeat(
            HexCoord::new(0, 0),
            &owners,
            &terrain,
            &"faction_1".to_string(),
            0.5,
        );
        assert!(result.is_some());
        assert!(!result.unwrap().is_victory(), "失败优先于胜利");
    }

    #[test]
    fn test_victory_empty_terrain() {
        let terrain: BTreeMap<TileKey, TerrainType> = BTreeMap::new();
        let owners: BTreeMap<TileKey, FactionId> = BTreeMap::new();
        let result = check_victory(&owners, &terrain, &"faction_1".to_string(), 0.5);
        assert!(result.is_none(), "空地图不触发");
    }

    #[test]
    fn test_game_over_reason_text() {
        let v = GameOverReason::Victory {
            tiles: 100,
            total: 200,
            ratio: 0.5,
        };
        let text = v.reason_text();
        assert!(text.contains("统一天下"));
        assert!(text.contains("100"));
        assert!(text.contains("200"));

        let d = GameOverReason::Defeat {
            city: HexCoord::new(10, 20),
            attacker: "faction_2".to_string(),
        };
        let text = d.reason_text();
        assert!(text.contains("主城"));
        assert!(text.contains("(10,20)"));
        assert!(text.contains("faction_2"));
    }
}
