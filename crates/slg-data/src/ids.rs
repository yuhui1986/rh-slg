//! 语义化 ID 类型定义

/// 武将 ID（如 "general_wei_caocao"）
pub type GeneralId = String;

/// 战法 ID（如 "skill_active_luolei"）
pub type SkillId = String;

/// 兵种 ID（如 "unit_cavalry"）
pub type UnitTypeId = String;

/// 势力 ID（如 "faction_wei"）
pub type FactionId = String;

/// 地形类型 ID（如 "terrain_plains"）
pub type TerrainTypeId = String;

/// 建筑 ID（如 "building_farm"）
pub type BuildingId = String;

/// 事件 ID（如 "event_yellow_turban"）
pub type EventId = String;

/// 地图格子 key（axial 坐标编码为 u64）
pub type TileKey = u64;

/// 将 axial 坐标 (q, r) 编码为 TileKey
pub fn tile_key(q: i32, r: i32) -> TileKey {
    ((r as u64) << 32) | (q as u64 & 0xFFFF_FFFF)
}

/// 从 TileKey 解码 axial 坐标 (q, r)
pub fn from_tile_key(key: TileKey) -> (i32, i32) {
    let q = (key & 0xFFFF_FFFF) as i32;
    let r = (key >> 32) as i32;
    (q, r)
}
