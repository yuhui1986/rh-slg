//! 率土风复古国风像素 atlas 打包工具 (M10 美术资源)
//!
//! 输入: `assets/textures/sample/*.png` (1024x1024 AI 出图)
//! 流程:
//!  1. 每个 PNG → resize 到 TILE_PX (32) nearest neighbor
//!  2. 地形 tile: hex mask 掉四角 (四角 alpha=0)
//!  3. icon tile: 保留方格 (将来 UI 用)
//!  4. 按命名顺序打包到 `atlas.png` (1024x1024, 32x32 grid)
//!  5. 生成 `atlas.json` 索引 (name → rect)
//!
//! 用法: `cargo run --bin build_atlas -p slg-assets`

use image::ImageReader;
use image::{imageops::resize, ImageBuffer, Rgba, RgbaImage};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::PathBuf;

/// 最终每 tile 像素尺寸 (32x32)
const TILE_PX: u32 = 32;
/// Atlas 大小 (32x32 grid = 1024x1024, 容纳 1024 个 tile)
const ATLAS_PX: u32 = TILE_PX * 32;
/// 6 地形 + 8 icon = 14 tile
const TILE_COLS: u32 = ATLAS_PX / TILE_PX; // 32

#[derive(Serialize, Debug, Clone)]
struct AtlasEntry {
    /// 在 atlas.png 中的像素矩形
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    /// 0..1 归一化 UV (左上角 + 右下角)
    uv_min: [f32; 2],
    uv_max: [f32; 2],
    /// 标签: "terrain" / "icon"
    kind: String,
}

#[derive(Serialize, Debug)]
struct AtlasIndex {
    tile_size: u32,
    atlas_size: u32,
    /// 8 地形 (Plains..Pass) UV 数组, 索引 = TerrainType::to_u8()
    /// 没图的用 snow 顶 (暂用 snow 顶 swamp/hills/pass)
    terrain_uv: Vec<Option<[f32; 4]>>,
    /// Icon UV: name -> [u_min, v_min, u_max, v_max]
    icon_uv: BTreeMap<String, [f32; 4]>,
    /// 详细 entries (调试用)
    entries: BTreeMap<String, AtlasEntry>,
}

/// tile 命名约定
/// - `tile_<name>.png` → kind = "terrain", hex mask
/// - `icon_<name>.png` → kind = "icon", 保持方格
fn classify(name: &str) -> Option<(&str, String)> {
    if let Some(rest) = name.strip_prefix("tile_") {
        Some(("terrain", rest.to_string()))
    } else if let Some(rest) = name.strip_prefix("icon_") {
        Some(("icon", rest.to_string()))
    } else {
        None
    }
}

/// TerrainType 顺序 (from slg-core/src/map/tile.rs::to_u8):
/// 0=Plains, 1=Mountain, 2=Water, 3=Forest, 4=Desert, 5=Swamp, 6=Hills, 7=Pass
const TERRAIN_ORDER: &[&str] = &[
    "plains",     // 0
    "mountain",   // 1
    "water",      // 2
    "forest",     // 3
    "desert",     // 4
    "swamp",      // 5 - 没图, 用 snow 顶
    "hills",      // 6 - 没图, 用 snow 顶
    "pass",       // 7 - 没图, 用 snow 顶
];

/// 哪个 terrain 文件实际存在, 不存在则用 fallback (snow)
const TERRAIN_FILES: &[&str] = &[
    "tile_plains.png",
    "tile_mountain.png",
    "tile_water.png",
    "tile_forest.png",
    "tile_desert.png",
    "tile_snow.png", // swamp
    "tile_snow.png", // hills
    "tile_snow.png", // pass
];

/// hex mask: 把 (px, py) 中心向外, 4 角 alpha=0
///
/// 32x32 tile, hex 中心 (16, 16), 边距 4 px
/// 六边形 (pointy-top) 实际宽度 ≈ 16*sqrt(3) ≈ 27.7
/// 把超出的 4 角透明
fn apply_hex_mask(img: &mut RgbaImage) {
    let cx = TILE_PX as f32 / 2.0; // 16
    let cy = TILE_PX as f32 / 2.0;
    let hex_h = TILE_PX as f32 / 2.0; // 16
    let hex_w = hex_h * (3.0_f32).sqrt() / 2.0; // ≈ 13.86
    for py in 0..TILE_PX {
        for px in 0..TILE_PX {
            let dx = (px as f32 - cx).abs();
            let dy = (py as f32 - cy).abs();
            // 简化的 hex 判定: 矩形 + 上下三角裁切
            // |dy| <= hex_h 时: dx <= hex_w (中央矩形)
            // |dy| > hex_h 时: 已超出, alpha = 0
            // 实际 pointy-top 边距随 y 变化: 顶部 y < hex_h/2, 边距线性增
            // 用简化判定: 在 |dy| > hex_h*0.5 的部分, 边距按比例缩小
            let in_hex = if dy <= hex_h * 0.5 {
                dx <= hex_w
            } else {
                let t = (dy - hex_h * 0.5) / (hex_h * 0.5);
                dx <= hex_w * (1.0 - t)
            };
            if !in_hex {
                let p = img.get_pixel_mut(px, py);
                *p = Rgba([p[0], p[1], p[2], 0]);
            }
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sample_dir = PathBuf::from("assets/textures/sample");
    let out_dir = PathBuf::from("assets/textures");

    if !sample_dir.exists() {
        return Err(format!("找不到输入目录: {}", sample_dir.display()).into());
    }
    std::fs::create_dir_all(&out_dir)?;

    // 创建 atlas (透明背景)
    let mut atlas: RgbaImage = ImageBuffer::from_pixel(ATLAS_PX, ATLAS_PX, Rgba([0, 0, 0, 0]));
    let mut index = AtlasIndex {
        tile_size: TILE_PX,
        atlas_size: ATLAS_PX,
        terrain_uv: vec![None; 8], // 8 个 TerrainType 槽位
        icon_uv: BTreeMap::new(),
        entries: BTreeMap::new(),
    };

    let mut slot = 0u32;

    // ---- 1. 地形 tile: 按 TERRAIN_ORDER 顺序填前 8 格 ----
    for (i, terrain_name) in TERRAIN_ORDER.iter().enumerate() {
        let file_name = TERRAIN_FILES[i];
        let path = sample_dir.join(file_name);
        if !path.exists() {
            return Err(format!("缺地形 tile: {}", path.display()).into());
        }
        let col = slot % TILE_COLS;
        let row = slot / TILE_COLS;
        let dst_x = col * TILE_PX;
        let dst_y = row * TILE_PX;

        // 加载 + resize + hex mask
        let src = ImageReader::open(&path)?
            .with_guessed_format()?
            .decode()?
            .to_rgba8();
        let mut small = resize(&src, TILE_PX, TILE_PX, image::imageops::FilterType::Nearest);
        apply_hex_mask(&mut small);

        // paste
        image::imageops::overlay(&mut atlas, &small, dst_x as i64, dst_y as i64);

        // 索引
        let uv = [
            dst_x as f32 / ATLAS_PX as f32,
            dst_y as f32 / ATLAS_PX as f32,
            (dst_x + TILE_PX) as f32 / ATLAS_PX as f32,
            (dst_y + TILE_PX) as f32 / ATLAS_PX as f32,
        ];
        index.terrain_uv[i] = Some(uv);
        index.entries.insert(
            (*terrain_name).to_string(),
            AtlasEntry {
                x: dst_x,
                y: dst_y,
                w: TILE_PX,
                h: TILE_PX,
                uv_min: [uv[0], uv[1]],
                uv_max: [uv[2], uv[3]],
                kind: "terrain".to_string(),
            },
        );
        slot += 1;
    }

    // ---- 2. icon tile: 8 格后开始, 按字母序 ----
    let mut icon_files: Vec<(String, PathBuf)> = std::fs::read_dir(&sample_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| {
            let path = e.path();
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            stem.starts_with("icon_")
        })
        .filter_map(|e| {
            let path = e.path();
            let stem = path.file_stem()?.to_str()?.to_string();
            Some((stem, path))
        })
        .collect();
    icon_files.sort_by(|a, b| a.0.cmp(&b.0));

    for (stem, path) in icon_files {
        if slot >= TILE_COLS * TILE_COLS {
            return Err(format!("atlas 容量满 ({} 个)", TILE_COLS * TILE_COLS).into());
        }
        let col = slot % TILE_COLS;
        let row = slot / TILE_COLS;
        let dst_x = col * TILE_PX;
        let dst_y = row * TILE_PX;

        let src = ImageReader::open(&path)?
            .with_guessed_format()?
            .decode()?
            .to_rgba8();
        let small = resize(&src, TILE_PX, TILE_PX, image::imageops::FilterType::Nearest);

        image::imageops::overlay(&mut atlas, &small, dst_x as i64, dst_y as i64);

        let (_, name) = classify(&stem)
            .ok_or_else(|| format!("文件命名不规范: {}", stem))?;
        let uv = [
            dst_x as f32 / ATLAS_PX as f32,
            dst_y as f32 / ATLAS_PX as f32,
            (dst_x + TILE_PX) as f32 / ATLAS_PX as f32,
            (dst_y + TILE_PX) as f32 / ATLAS_PX as f32,
        ];
        index.icon_uv.insert(name.clone(), uv);
        index.entries.insert(
            name,
            AtlasEntry {
                x: dst_x,
                y: dst_y,
                w: TILE_PX,
                h: TILE_PX,
                uv_min: [uv[0], uv[1]],
                uv_max: [uv[2], uv[3]],
                kind: "icon".to_string(),
            },
        );
        slot += 1;
    }

    println!("填了 {} 个 tile (8 地形 + {} icon)", slot, index.icon_uv.len());

    // 写 atlas.png
    let atlas_path = out_dir.join("atlas.png");
    atlas.save(&atlas_path)?;
    println!("✓ 写 atlas: {}", atlas_path.display());

    // 写 atlas.json
    let json_path = out_dir.join("atlas.json");
    let json = serde_json::to_string_pretty(&index)?;
    std::fs::write(&json_path, json)?;
    println!("✓ 写索引: {}", json_path.display());

    Ok(())
}
