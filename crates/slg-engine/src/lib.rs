//! slg-engine: 《天下策》Bevy 渲染与输入层
//!
//! 负责 Chunk 网格渲染、LOD、相机平移缩放、格子拾取、迷雾渲染、tick_dispatcher 时钟。

use bevy::prelude::*;

pub mod camera;
pub mod render;
pub mod systems;

/// 《天下策》引擎核心插件
///
/// 负责渲染管线、相机系统、时钟调度等基础设施。
/// M0 阶段为空壳，M1 填充实际渲染逻辑。
pub struct SlgEnginePlugin;

impl Plugin for SlgEnginePlugin {
    fn build(&self, app: &mut App) {
        // M1: 注册 Chunk 渲染、相机、tick_dispatcher 等系统
        app.add_plugins((
            systems::ClockPlugin,
            render::ChunkRenderPlugin,
            camera::CameraPlugin,
        ));
    }
}
