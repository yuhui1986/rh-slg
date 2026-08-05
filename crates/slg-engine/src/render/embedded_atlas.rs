//! M10.3 美术资源接 mesh: 内嵌 atlas.png + atlas.json
//!
//! 编译时把 `crates/slg-engine/assets/atlas.png` 和 atlas.json 嵌入 binary.
//! 不依赖 slg-assets (避免循环依赖).
//!
//! 同步工作: assets/textures/atlas.png 改了之后, 复制到 crates/slg-engine/assets/atlas.png,
//! 重编译 slg-engine. 后续 M10.4+ 可以做 build.rs 自动复制.

use bevy::prelude::*;

/// atlas.png 字节 (1024x1024, 8 地形 + N icon)
pub const ATLAS_PNG: &[u8] = include_bytes!("../../assets/atlas.png");

/// atlas.json 字符串
pub const ATLAS_JSON: &str = include_str!("../../assets/atlas.json");

/// 8 地形 UV 数组 (索引 = TerrainType::to_u8)
///
/// 从 atlas.json.terrain_uv 解析. None 表示该地形没图 (Swamp/Hills/Pass 暂用 snow).
/// UV 格式: [u_min, v_min, u_max, v_max]
#[derive(Debug, Clone)]
pub struct TerrainAtlasMeta {
    pub terrain_uv: Vec<Option<[f32; 4]>>,
    pub atlas_size: u32,
    pub tile_size: u32,
}

impl TerrainAtlasMeta {
    /// 从内嵌的 ATLAS_JSON 解析
    pub fn parse_embedded() -> Self {
        #[derive(serde::Deserialize)]
        struct Json {
            tile_size: u32,
            atlas_size: u32,
            terrain_uv: Vec<Option<[f32; 4]>>,
        }
        let parsed: Json = serde_json::from_str(ATLAS_JSON)
            .expect("atlas.json 解析失败 (重新运行 `cargo run --bin build_atlas -p slg-assets`)");
        Self {
            terrain_uv: parsed.terrain_uv,
            atlas_size: parsed.atlas_size,
            tile_size: parsed.tile_size,
        }
    }
}

/// M10.3 引擎侧 atlas UV Resource
///
/// 启动时从内嵌 JSON 解析, init_resource! 自动填.
/// chunk mesh rebuild 时读这个, 不用每个 chunk 存.
#[derive(Resource, Debug, Clone)]
pub struct AtlasUvRes(pub [[f32; 4]; 8]);

impl Default for AtlasUvRes {
    fn default() -> Self {
        let meta = TerrainAtlasMeta::parse_embedded();
        // 把 Vec<Option<[f32;4]>> 转 [[f32;4]; 8], None 填 fallback 整图
        let mut arr = [[0.0, 0.0, 1.0, 1.0]; 8];
        for (i, slot) in meta.terrain_uv.iter().enumerate() {
            if let Some(uv) = slot {
                arr[i] = *uv;
            }
        }
        Self(arr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_embedded_atlas() {
        let meta = TerrainAtlasMeta::parse_embedded();
        assert_eq!(meta.atlas_size, 1024);
        assert_eq!(meta.tile_size, 32);
        // 8 地形槽位都填了
        assert_eq!(meta.terrain_uv.len(), 8);
        // Plains (0) UV 是 (0/1024, 0/1024, 32/1024, 32/1024)
        let plains = meta.terrain_uv[0].unwrap();
        assert_eq!(plains[0], 0.0);
        assert!((plains[2] - 32.0 / 1024.0).abs() < 0.001);
    }

    /// TEST45: AtlasUvRes::default() 从内嵌 JSON 正确解析
    /// 验证 8 地形 UV 都填了, 索引 = TerrainType::to_u8
    #[test]
    fn test_atlas_uv_res_default() {
        let res = AtlasUvRes::default();
        // 索引 0 (Plains) UV 是 (0, 0, 32/1024, 32/1024)
        assert_eq!(res.0[0][0], 0.0);
        assert!((res.0[0][2] - 32.0 / 1024.0).abs() < 0.001);
        // 索引 1 (Mountain) UV 是 (32/1024, 0, 64/1024, 32/1024)
        assert!((res.0[1][0] - 32.0 / 1024.0).abs() < 0.001);
        // 索引 7 (Pass/Snow) 也填了
        assert!((res.0[7][2] - res.0[7][0]) > 0.0, "tile 7 应该有非零宽度");
    }
}
