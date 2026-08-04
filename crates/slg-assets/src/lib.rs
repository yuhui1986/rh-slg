//! slg-assets: 《天下策》数据表加载与 Mod 合并
//!
//! 负责 RON 数据表加载、Mod 合并、热重载。

use slg_data::config::*;
use slg_data::ids::*;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// 数据加载错误
#[derive(Error, Debug)]
pub enum AssetError {
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),
    #[error("RON 解析错误: {0}")]
    RonParse(String),
    #[error("文件未找到: {0}")]
    FileNotFound(PathBuf),
    #[error("交叉引用错误: {0}")]
    InvalidReference(String),
}

/// 数据存储 Resource
///
/// 持有所有从 RON 文件加载的配置表。
#[derive(Debug, Default)]
pub struct DataStore {
    pub generals: BTreeMap<GeneralId, GeneralDef>,
    pub skills: BTreeMap<SkillId, SkillDef>,
    pub unit_types: BTreeMap<UnitTypeId, UnitTypeDef>,
    pub terrain_types: BTreeMap<TerrainTypeId, TerrainTypeDef>,
    pub buildings: BTreeMap<BuildingId, BuildingDef>,
    pub events: BTreeMap<EventId, EventDef>,
    pub global_params: Option<GlobalParams>,
    pub loaded: bool,
}

/// HasId trait：获取 ID
pub trait HasId {
    fn get_id(&self) -> &str;
}

// 为各类型实现 HasId
impl HasId for GeneralDef {
    fn get_id(&self) -> &str {
        &self.id
    }
}
impl HasId for SkillDef {
    fn get_id(&self) -> &str {
        &self.id
    }
}
impl HasId for UnitTypeDef {
    fn get_id(&self) -> &str {
        &self.id
    }
}
impl HasId for TerrainTypeDef {
    fn get_id(&self) -> &str {
        &self.id
    }
}
impl HasId for BuildingDef {
    fn get_id(&self) -> &str {
        &self.id
    }
}
impl HasId for EventDef {
    fn get_id(&self) -> &str {
        &self.id
    }
}

/// 从指定目录加载所有数据表
pub fn load_all(data_dir: &Path) -> Result<DataStore, AssetError> {
    if !data_dir.exists() {
        return Err(AssetError::FileNotFound(data_dir.to_path_buf()));
    }

    // 加载各 RON 文件
    let store = DataStore {
        generals: load_ron_file(&data_dir.join("generals.ron"))?,
        skills: load_ron_file(&data_dir.join("skills.ron"))?,
        unit_types: load_ron_file(&data_dir.join("unit_types.ron"))?,
        terrain_types: load_ron_file(&data_dir.join("terrain_types.ron"))?,
        buildings: load_ron_file(&data_dir.join("buildings.ron"))?,
        events: load_ron_file(&data_dir.join("events.ron"))?,
        global_params: Some(load_ron_file_single(&data_dir.join("global_params.ron"))?),
        loaded: true,
    };

    // 交叉引用校验
    validate_references(&store)?;

    Ok(store)
}

/// 加载 RON 文件为 BTreeMap（列表格式）
fn load_ron_file<T>(path: &Path) -> Result<BTreeMap<String, T>, AssetError>
where
    T: serde::de::DeserializeOwned + HasId,
{
    let content =
        std::fs::read_to_string(path).map_err(|_| AssetError::FileNotFound(path.to_path_buf()))?;

    let items: Vec<T> =
        ron::from_str(&content).map_err(|e| AssetError::RonParse(format!("{:?}: {}", path, e)))?;

    let mut map = BTreeMap::new();
    for item in items {
        map.insert(item.get_id().to_string(), item);
    }

    Ok(map)
}

/// 加载 RON 文件为单个结构体
fn load_ron_file_single<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, AssetError> {
    let content =
        std::fs::read_to_string(path).map_err(|_| AssetError::FileNotFound(path.to_path_buf()))?;

    ron::from_str(&content).map_err(|e| AssetError::RonParse(format!("{:?}: {}", path, e)))
}

/// 交叉引用校验
fn validate_references(store: &DataStore) -> Result<(), AssetError> {
    // 校验武将引用的战法存在
    for (id, general) in &store.generals {
        if !store.skills.contains_key(&general.innate_skill) {
            return Err(AssetError::InvalidReference(format!(
                "武将 {} 引用的战法 {} 不存在",
                id, general.innate_skill
            )));
        }
        for skill_id in &general.learnable_skills {
            if !store.skills.contains_key(skill_id) {
                return Err(AssetError::InvalidReference(format!(
                    "武将 {} 引用的可学习战法 {} 不存在",
                    id, skill_id
                )));
            }
        }
        for unit_id in &general.unit_types {
            if !store.unit_types.contains_key(unit_id) {
                return Err(AssetError::InvalidReference(format!(
                    "武将 {} 引用的兵种 {} 不存在",
                    id, unit_id
                )));
            }
        }
    }

    // 校验战法引用的武将存在
    for (id, skill) in &store.skills {
        if let Some(ref general_id) = skill.source_general {
            if !store.generals.contains_key(general_id) {
                return Err(AssetError::InvalidReference(format!(
                    "战法 {} 引用的来源武将 {} 不存在",
                    id, general_id
                )));
            }
        }
    }

    // 校验兵种克制目标存在
    for (id, unit) in &store.unit_types {
        if !store.unit_types.contains_key(&unit.counter_target) {
            return Err(AssetError::InvalidReference(format!(
                "兵种 {} 引用的克制目标 {} 不存在",
                id, unit.counter_target
            )));
        }
    }

    // 校验兵种地形适性引用的地形存在
    for (id, unit) in &store.unit_types {
        for (terrain_id, _) in &unit.terrain_adaptation {
            if !store.terrain_types.contains_key(terrain_id) {
                return Err(AssetError::InvalidReference(format!(
                    "兵种 {} 引用的地形 {} 不存在",
                    id, terrain_id
                )));
            }
        }
    }

    // 校验建筑地形需求引用的地形存在
    for (id, building) in &store.buildings {
        for terrain_id in &building.terrain_req {
            if !store.terrain_types.contains_key(terrain_id) {
                return Err(AssetError::InvalidReference(format!(
                    "建筑 {} 引用的地形 {} 不存在",
                    id, terrain_id
                )));
            }
        }
    }

    Ok(())
}

/// 加载剧本
pub fn load_scenario(path: &Path) -> Result<ScenarioDef, AssetError> {
    let content =
        std::fs::read_to_string(path).map_err(|_| AssetError::FileNotFound(path.to_path_buf()))?;

    ron::from_str(&content).map_err(|e| AssetError::RonParse(format!("{:?}: {}", path, e)))
}

/// 剧本定义（简化）
#[derive(Debug, serde::Deserialize)]
pub struct ScenarioDef {
    pub id: String,
    pub name: String,
    pub description: String,
    pub map_size: (u32, u32),
    pub seed: u64,
}

/// 合并 Mod 数据
///
/// 按 mod.toml priority 排序，同 ID 覆盖整条记录。
pub fn merge_mods(_base: &mut DataStore, _mod_dir: &Path) -> Result<(), AssetError> {
    // TODO: 遍历 mods/*/data/，按优先级合并
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_load_all_missing_dir() {
        let result = load_all(Path::new("/nonexistent/path"));
        assert!(result.is_err());
    }

    #[test]
    fn test_load_all_real_data() {
        let data_dir = Path::new("assets/data");
        if data_dir.exists() {
            let result = load_all(data_dir);
            assert!(
                result.is_ok(),
                "load_all should succeed: {:?}",
                result.err()
            );
            let store = result.unwrap();
            assert!(store.loaded);
            assert!(!store.generals.is_empty(), "generals should not be empty");
            assert!(!store.skills.is_empty(), "skills should not be empty");
            assert!(
                !store.unit_types.is_empty(),
                "unit_types should not be empty"
            );
            assert!(
                !store.terrain_types.is_empty(),
                "terrain_types should not be empty"
            );
            assert!(!store.buildings.is_empty(), "buildings should not be empty");
            assert!(!store.events.is_empty(), "events should not be empty");
            assert!(
                store.global_params.is_some(),
                "global_params should be loaded"
            );
        }
    }

    #[test]
    fn test_cross_references_valid() {
        let data_dir = Path::new("assets/data");
        if data_dir.exists() {
            let store = load_all(data_dir).expect("load_all should succeed");
            // 如果加载成功，交叉引用校验已通过
            assert!(store.loaded);
        }
    }

    #[test]
    fn test_load_scenario() {
        let scenario_path = Path::new("assets/data/scenarios/sanguo_dl/scenario.ron");
        if scenario_path.exists() {
            let result = load_scenario(scenario_path);
            assert!(
                result.is_ok(),
                "load_scenario should succeed: {:?}",
                result.err()
            );
            let scenario = result.unwrap();
            assert_eq!(scenario.id, "scenario_sanguo_dl");
            assert_eq!(scenario.name, "三国鼎立");
            assert_eq!(scenario.map_size, (256, 256));
            assert_eq!(scenario.seed, 42);
        }
    }
}
