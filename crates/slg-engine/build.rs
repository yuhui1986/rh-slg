//! M10.4 build.rs: 把 workspace 根的 `assets/textures/atlas.{png,json}` 拷贝到 OUT_DIR,
//! 编译时 `include_bytes!(concat!(env!("OUT_DIR"), "/atlas.png"))` 读出来嵌入 binary.
//!
//! **改 atlas 流程**:
//! 1. 改 `assets/textures/atlas.png` (或重新跑 `cargo run --bin build_atlas -p slg-assets`)
//! 2. 改 `assets/textures/atlas.json` (build_atlas 会自动重写)
//! 3. `cargo build` 时 build.rs 检测到变化, 自动同步到 OUT_DIR
//!
//! **不再需要** 手工 `cp assets/textures/atlas.png crates/slg-engine/assets/`.

use std::path::Path;
use std::fs;

fn main() {
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR 未设");
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()  // crates/
        .and_then(|p| p.parent())  // workspace root
        .expect("找不到 workspace root");

    let src_dir = workspace_root.join("assets").join("textures");
    let src_png = src_dir.join("atlas.png");
    let src_json = src_dir.join("atlas.json");

    let dst_png = Path::new(&out_dir).join("atlas.png");
    let dst_json = Path::new(&out_dir).join("atlas.json");

    // 源文件必须存在, 否则 panic 出明确错误 (提醒用户跑 build_atlas)
    if !src_png.exists() {
        panic!(
            "M10.4: 找不到 `{}`, 请先跑 `cargo run --bin build_atlas -p slg-assets` 生成 atlas",
            src_png.display()
        );
    }
    if !src_json.exists() {
        panic!(
            "M10.4: 找不到 `{}`, 请先跑 `cargo run --bin build_atlas -p slg-assets` 生成 atlas",
            src_json.display()
        );
    }

    fs::copy(&src_png, &dst_png).unwrap_or_else(|e| {
        panic!(
            "M10.4: 复制 atlas.png 从 {} 到 {} 失败: {}",
            src_png.display(),
            dst_png.display(),
            e
        )
    });
    fs::copy(&src_json, &dst_json).unwrap_or_else(|e| {
        panic!(
            "M10.4: 复制 atlas.json 从 {} 到 {} 失败: {}",
            src_json.display(),
            dst_json.display(),
            e
        )
    });

    // 让 cargo 在源文件变化时重跑 build.rs
    println!("cargo:rerun-if-changed={}", src_png.display());
    println!("cargo:rerun-if-changed={}", src_json.display());

    println!(
        "M10.4: 已同步 atlas → {}/atlas.{{png,json}}",
        out_dir
    );
}
