//! Atlas 加载 + UV 索引 (M10 美术资源)
//!
//! atlas.png + atlas.json 由 `cargo run --bin build_atlas -p slg-assets` 生成
//! 本模块负责反序列化 + 提供按 TerrainType/u8/icon name 查 UV 的 helper
//!
//! 重要: 改 UV 索引时一定要看 slg-core/src/map/tile.rs::to_u8 是否一致

use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

/// 8 地形 UV (索引 = TerrainType::to_u8())
/// None 表示该地形没图(目前 Swamp/Hills/Pass 用 Snow 顶)
pub type TerrainUv = [f32; 4];

/// icon UV: name (e.g. "brush", "save") -> [u_min, v_min, u_max, v_max]
pub type IconUv = [f32; 4];

/// 加载后的 atlas (PNG bytes + UV 索引)
#[derive(Debug, Clone)]
pub struct LoadedAtlas {
    /// atlas.png 原始 bytes
    pub png_bytes: Vec<u8>,
    /// atlas size (单边 px)
    pub atlas_size: u32,
    /// tile size (单边 px)
    pub tile_size: u32,
    /// 8 地形 UV
    pub terrain_uv: Vec<Option<TerrainUv>>,
    /// icon UV
    pub icon_uv: HashMap<String, IconUv>,
}

/// JSON 反序列化结构
#[derive(Debug, Deserialize)]
struct AtlasJson {
    tile_size: u32,
    atlas_size: u32,
    terrain_uv: Vec<Option<TerrainUv>>,
    icon_uv: HashMap<String, IconUv>,
    #[serde(default)]
    #[allow(dead_code)]
    entries: HashMap<String, serde_json::Value>,
}

impl LoadedAtlas {
    /// 从磁盘加载 atlas.png + atlas.json
    pub fn load(png_path: impl AsRef<Path>, json_path: impl AsRef<Path>) -> Result<Self, String> {
        let png_bytes = std::fs::read(png_path.as_ref())
            .map_err(|e| format!("读 atlas.png 失败: {}", e))?;
        let json_str = std::fs::read_to_string(json_path.as_ref())
            .map_err(|e| format!("读 atlas.json 失败: {}", e))?;
        let parsed: AtlasJson = serde_json::from_str(&json_str)
            .map_err(|e| format!("解析 atlas.json 失败: {}", e))?;
        Ok(Self {
            png_bytes,
            atlas_size: parsed.atlas_size,
            tile_size: parsed.tile_size,
            terrain_uv: parsed.terrain_uv,
            icon_uv: parsed.icon_uv,
        })
    }

    /// 按 TerrainType u8 索引 (0..7) 查 UV
    ///
    /// 返回 None 表示该地形没图(Swamp/Hills/Pass 暂用 Snow 顶, 但 uv 都填了)
    pub fn terrain_uv_by_u8(&self, terrain_u8: u8) -> Option<TerrainUv> {
        if (terrain_u8 as usize) < self.terrain_uv.len() {
            self.terrain_uv[terrain_u8 as usize]
        } else {
            None
        }
    }

    /// 按 icon name 查 UV
    pub fn icon_uv(&self, name: &str) -> Option<IconUv> {
        self.icon_uv.get(name).copied()
    }

    /// 8 地形是否都填了 UV
    pub fn all_terrains_mapped(&self) -> bool {
        self.terrain_uv.iter().all(|uv| uv.is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// M10 测试: 加载现有 atlas + 验证索引正确
    #[test]
    fn test_load_atlas() {
        // cargo test 跑在 crate 目录, 用 CARGO_MANIFEST_DIR 拼绝对路径
        let manifest = env!("CARGO_MANIFEST_DIR");
        let atlas = LoadedAtlas::load(
            format!("{}/../../assets/textures/atlas.png", manifest),
            format!("{}/../../assets/textures/atlas.json", manifest),
        )
        .expect("atlas 应能加载");

        // atlas.png 应该非空
        assert!(!atlas.png_bytes.is_empty());
        assert_eq!(atlas.atlas_size, 1024);
        assert_eq!(atlas.tile_size, 32);

        // 8 地形 UV 都填了 (Plains..Pass, 含 Swamp/Hills/Pass 暂用 Snow)
        assert_eq!(atlas.terrain_uv.len(), 8);
        assert!(
            atlas.all_terrains_mapped(),
            "8 地形 UV 全部映射 (暂用 Snow 顶缺的)"
        );

        // Plains UV 应该是 (0/1024, 0/1024, 32/1024, 32/1024) = (0, 0, 0.03125, 0.03125)
        let plains = atlas.terrain_uv_by_u8(0).unwrap();
        assert_eq!(plains[0], 0.0);
        assert_eq!(plains[1], 0.0);
        assert!((plains[2] - 32.0 / 1024.0).abs() < 0.001);
        assert!((plains[3] - 32.0 / 1024.0).abs() < 0.001);

        // Mountain UV 在第 2 格 (1*32 / 1024)
        let mountain = atlas.terrain_uv_by_u8(1).unwrap();
        assert!((mountain[0] - 32.0 / 1024.0).abs() < 0.001);

        // 超出 7 → None
        assert!(atlas.terrain_uv_by_u8(8).is_none());
    }

    #[test]
    fn test_icon_uv_lookup() {
        let manifest = env!("CARGO_MANIFEST_DIR");
        let atlas = LoadedAtlas::load(
            format!("{}/../../assets/textures/atlas.png", manifest),
            format!("{}/../../assets/textures/atlas.json", manifest),
        )
        .expect("atlas 应能加载");

        // icon 都在第 8 格之后, 8/9/10... 列
        let brush = atlas.icon_uv("brush").expect("brush icon 应存在");
        let save = atlas.icon_uv("save").expect("save icon 应存在");
        let undo = atlas.icon_uv("undo").expect("undo icon 应存在");

        // brush 在 col=8 (icon 从第 8 格开始)
        assert!((brush[0] - 8.0 * 32.0 / 1024.0).abs() < 0.001);
        assert_eq!(brush[1], 0.0);

        // save 在 col=15 (icon 按字母序: brush/flood_fill/load/new/place/redo/remove/save/undo)
        assert!((save[0] - 15.0 * 32.0 / 1024.0).abs() < 0.001);

        // undo 在 col=16
        assert!((undo[0] - 16.0 * 32.0 / 1024.0).abs() < 0.001);

        // 不存在的 icon → None
        assert!(atlas.icon_uv("nonexistent").is_none());
    }
}
