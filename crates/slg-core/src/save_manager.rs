//! 存档管理器
//!
//! 负责游戏内存档/读档的完整流程：创建存档、保存到文件、列出存档、加载存档。
//! 自动存档基于 tick 间隔触发。

use std::collections::BTreeMap;
use std::path::PathBuf;

use slg_data::ids::FactionId;
use slg_data::map_doc::MapDocument;
use slg_data::save::SaveFile;

use crate::entity::faction::FactionState;
use crate::resource::GameClock;

/// 存档元信息（用于 UI 展示存档列表）
#[derive(Debug, Clone)]
pub struct SaveMeta {
    pub filename: String,
    pub scenario_name: String,
    pub player_faction: String,
    pub game_day: u64,
    pub save_time: String,
    pub map_size: (u32, u32),
}

/// 存档管理器
///
/// 管理存档目录、自动存档间隔，提供创建/保存/列出/加载存档的完整 API。
/// 纯数据结构，不依赖任何引擎类型。
#[derive(Debug, Clone, Default)]
pub struct SaveManager {
    pub save_dir: PathBuf,
    pub auto_save_interval: u64,
    pub last_auto_save: u64,
}

impl SaveManager {
    /// 创建新的存档管理器
    pub fn new(save_dir: PathBuf) -> Self {
        Self {
            save_dir,
            auto_save_interval: 100, // 每 100 tick 自动存档
            last_auto_save: 0,
        }
    }

    /// 检查是否需要自动存档
    pub fn should_auto_save(&self, current_tick: u64) -> bool {
        current_tick - self.last_auto_save >= self.auto_save_interval
    }

    /// 从运行时状态创建存档
    ///
    /// 将 `crate::entity::faction::FactionState` 转换为 `slg_data::save::FactionState`，
    /// 并基于地图文档生成存档引用。
    pub fn create_save(
        &self,
        doc: &MapDocument,
        factions: &BTreeMap<FactionId, FactionState>,
        clock: &GameClock,
        _player_faction: &str,
    ) -> Result<SaveFile, String> {
        let faction_states = factions
            .iter()
            .map(|(id, state)| slg_data::save::FactionState {
                faction_id: id.clone(),
                resources: slg_data::save::FactionResources {
                    gold: state.resources.gold,
                    food: state.resources.food,
                    wood: state.resources.wood,
                    iron: state.resources.iron,
                    troops: state.resources.troops,
                },
                diplomacy: state
                    .diplomacy
                    .iter()
                    .map(|(k, v)| (k.clone(), *v))
                    .collect(),
            })
            .collect();

        let save = SaveFile {
            map_ref: slg_data::save::MapRef {
                path: doc.meta.name.clone(),
                content_hash: [0; 32],
            },
            tick: clock.current_tick,
            faction_states,
            entity_snapshots: Vec::new(),
            tile_delta: Vec::new(),
            event_log: Vec::new(),
        };

        Ok(save)
    }

    /// 将存档保存到文件
    ///
    /// 文件保存在 `save_dir` 下，扩展名为 `.slgsave`。
    pub fn save_to_file(&self, save: &SaveFile, filename: &str) -> Result<PathBuf, String> {
        // 确保目录存在
        std::fs::create_dir_all(&self.save_dir).map_err(|e| e.to_string())?;

        let path = self.save_dir.join(filename);
        slg_save::container::save_save_to_file(save, &path).map_err(|e| e.to_string())?;
        Ok(path)
    }

    /// 列出所有存档
    ///
    /// 扫描 `save_dir` 下的 `.slgsave` 文件，按 game_day 降序排列。
    pub fn list_saves(&self) -> Vec<SaveMeta> {
        let mut saves = Vec::new();

        if let Ok(entries) = std::fs::read_dir(&self.save_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "slgsave") {
                    if let Ok(save) = slg_save::container::load_save_from_file(&path) {
                        saves.push(SaveMeta {
                            filename: path.file_name().unwrap().to_string_lossy().to_string(),
                            scenario_name: save.map_ref.path.clone(),
                            player_faction: "player".to_string(),
                            game_day: save.tick / 10,
                            save_time: "未知".to_string(),
                            map_size: (256, 256),
                        });
                    }
                }
            }
        }

        saves.sort_by_key(|b| std::cmp::Reverse(b.game_day));
        saves
    }

    /// 从文件加载存档
    pub fn load_save(&self, filename: &str) -> Result<SaveFile, String> {
        let path = self.save_dir.join(filename);
        slg_save::container::load_save_from_file(&path).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::faction::{FactionPersonality, FactionResources, FactionState};
    use std::path::PathBuf;

    fn create_test_manager() -> SaveManager {
        SaveManager::new(PathBuf::from("user/saves"))
    }

    fn create_test_factions() -> BTreeMap<FactionId, FactionState> {
        let mut factions = BTreeMap::new();
        factions.insert(
            "faction_wei".to_string(),
            FactionState {
                resources: FactionResources {
                    gold: 5000,
                    food: 3000,
                    wood: 2000,
                    iron: 1000,
                    stone: 500,
                    troops: 500,
                },
                personality: FactionPersonality {
                    aggression: 0.7,
                    expansion: 0.6,
                    diplomacy: 0.4,
                    caution: 0.3,
                },
                main_city: None,
                diplomacy: BTreeMap::new(),
                ..Default::default()
            },
        );
        factions
    }

    fn create_test_doc() -> MapDocument {
        MapDocument {
            meta: slg_data::map_doc::MapMeta {
                name: "测试地图".to_string(),
                seed: 42,
                width: 256,
                height: 256,
                preset_name: None,
            },
            terrain: slg_data::map_doc::TerrainLayer {
                rle_data: vec![("terrain_plains".to_string(), 1024)],
                total_tiles: 1024,
            },
            resources: slg_data::map_doc::ResourceLayer {
                entries: BTreeMap::new(),
            },
            entities: slg_data::map_doc::EntityLayer {
                placements: BTreeMap::new(),
            },
            rules: slg_data::map_doc::RuleLayer {
                zones: vec![],
                triggers: vec![],
            },
            rivers: Default::default(),
        }
    }

    #[test]
    fn test_save_manager_creation() {
        let manager = create_test_manager();
        assert_eq!(manager.auto_save_interval, 100);
        assert_eq!(manager.last_auto_save, 0);
    }

    #[test]
    fn test_should_auto_save_at_interval() {
        let manager = create_test_manager();
        assert!(manager.should_auto_save(100));
        assert!(manager.should_auto_save(200));
        assert!(manager.should_auto_save(150));
    }

    #[test]
    fn test_should_not_auto_save_before_interval() {
        let manager = create_test_manager();
        assert!(!manager.should_auto_save(50));
        assert!(!manager.should_auto_save(0));
        assert!(!manager.should_auto_save(99));
    }

    #[test]
    fn test_create_save_basic() {
        let manager = create_test_manager();
        let doc = create_test_doc();
        let factions = create_test_factions();
        let clock = GameClock {
            current_tick: 1000,
            speed: crate::resource::Speed::X1,
            accumulator: 0.0,
        };

        let save = manager
            .create_save(&doc, &factions, &clock, "player")
            .expect("create_save failed");

        assert_eq!(save.tick, 1000);
        assert_eq!(save.map_ref.path, "测试地图");
        assert_eq!(save.faction_states.len(), 1);
        assert_eq!(save.faction_states[0].faction_id, "faction_wei");
        assert_eq!(save.faction_states[0].resources.gold, 5000);
        assert_eq!(save.faction_states[0].resources.troops, 500);
    }

    #[test]
    fn test_create_save_converts_faction_state() {
        let manager = create_test_manager();
        let doc = create_test_doc();
        let factions = create_test_factions();
        let clock = GameClock::default();

        let save = manager
            .create_save(&doc, &factions, &clock, "player")
            .expect("create_save failed");

        let fs = &save.faction_states[0];
        assert_eq!(fs.resources.food, 3000);
        assert_eq!(fs.resources.wood, 2000);
        assert_eq!(fs.resources.iron, 1000);
    }

    #[test]
    fn test_save_and_load_file() {
        let dir = std::env::temp_dir().join("slg_save_manager_test_roundtrip");
        let _ = std::fs::create_dir_all(&dir);

        let manager = SaveManager::new(dir.clone());
        let doc = create_test_doc();
        let factions = create_test_factions();
        let clock = GameClock {
            current_tick: 500,
            speed: crate::resource::Speed::X2,
            accumulator: 0.5,
        };

        let save = manager
            .create_save(&doc, &factions, &clock, "player")
            .expect("create_save failed");

        let path = manager
            .save_to_file(&save, "test_save.slgsave")
            .expect("save_to_file failed");
        assert!(path.exists());

        let loaded = manager
            .load_save("test_save.slgsave")
            .expect("load_save failed");

        assert_eq!(loaded.tick, 500);
        assert_eq!(loaded.map_ref.path, "测试地图");
        assert_eq!(loaded.faction_states.len(), 1);
        assert_eq!(loaded.faction_states[0].faction_id, "faction_wei");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_list_saves() {
        let dir = std::env::temp_dir().join("slg_save_manager_test_list");
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);

        let manager = SaveManager::new(dir.clone());
        let doc = create_test_doc();
        let factions = create_test_factions();
        let clock = GameClock {
            current_tick: 100,
            speed: crate::resource::Speed::X1,
            accumulator: 0.0,
        };

        // 保存两个存档
        let save1 = manager
            .create_save(&doc, &factions, &clock, "player")
            .unwrap();
        manager.save_to_file(&save1, "save1.slgsave").unwrap();

        let clock2 = GameClock {
            current_tick: 300,
            ..clock
        };
        let save2 = manager
            .create_save(&doc, &factions, &clock2, "player")
            .unwrap();
        manager.save_to_file(&save2, "save2.slgsave").unwrap();

        let list = manager.list_saves();
        assert_eq!(list.len(), 2);

        // 按 game_day 降序排列
        assert_eq!(list[0].filename, "save2.slgsave");
        assert_eq!(list[0].game_day, 30); // 300 / 10
        assert_eq!(list[1].filename, "save1.slgsave");
        assert_eq!(list[1].game_day, 10); // 100 / 10

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_list_saves_empty_dir() {
        let dir = std::env::temp_dir().join("slg_save_manager_test_empty");
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);

        let manager = SaveManager::new(dir.clone());
        let list = manager.list_saves();
        assert!(list.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_list_saves_nonexistent_dir() {
        let manager = SaveManager::new(PathBuf::from("nonexistent/path/to/saves"));
        let list = manager.list_saves();
        assert!(list.is_empty());
    }

    #[test]
    fn test_load_save_nonexistent() {
        let manager = SaveManager::new(PathBuf::from("nonexistent/path"));
        let result = manager.load_save("no_such_file.slgsave");
        assert!(result.is_err());
    }

    #[test]
    fn test_auto_save_with_offset() {
        let mut manager = create_test_manager();
        manager.last_auto_save = 50;

        assert!(!manager.should_auto_save(100)); // 100 - 50 = 50 < 100
        assert!(manager.should_auto_save(150)); // 150 - 50 = 100 >= 100
        assert!(manager.should_auto_save(200)); // 200 - 50 = 150 >= 100
    }
}
