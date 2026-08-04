//! 《天下策》应用入口

use bevy::prelude::*;

fn main() {
    // Windows 控制台 UTF-8 支持
    #[cfg(windows)]
    {
        extern "system" {
            fn SetConsoleOutputCP(wCodePageID: u32) -> i32;
        }
        unsafe { SetConsoleOutputCP(65001); } // 65001 = UTF-8
    }

    // 初始化日志
    tracing_subscriber::fmt::init();

    // 初始化 Steam（可选）
    if let Err(e) = slg_app::init_steam() {
        eprintln!("Steam 初始化失败: {}", e);
    }

    App::new()
        .insert_resource(ClearColor(Color::srgb(0.1, 0.1, 0.15)))
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "天下策".to_string(),
                resolution: (1280.0_f32, 720.0_f32).into(),
                ..default()
            }),
            ..default()
        }).disable::<bevy::log::LogPlugin>())
        .add_plugins(slg_app::SlgAppPlugin)
        .run();
}
