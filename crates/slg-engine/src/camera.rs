//! 相机系统：平移缩放 + hex 拾取
//!
//! M1-T13：实现相机平移缩放与屏幕坐标到 hex 坐标的精确拾取。
//!
//! 平移：WASD / 鼠标中键拖拽 / 边缘滚屏
//! 缩放：滚轮缩放，限制 min/max zoom
//! 拾取：屏幕坐标 → 相机射线 → 世界坐标 → axial 坐标 → round()

use bevy::input::mouse::*;
use bevy::prelude::ColorMaterial;
use bevy::prelude::*;
use bevy::render::mesh::{Indices, Mesh, PrimitiveTopology};
use bevy::render::render_asset::RenderAssetUsages;
use bevy::sprite::MeshMaterial2d;
use bevy_egui::EguiContexts;
use slg_core::map::grid::HexCoord;

use crate::render::chunk_mesh::HEX_SIZE;

use bevy::input::ButtonState;

// ---------------------------------------------------------------------------
// 插件
// ---------------------------------------------------------------------------

/// 相机插件：注册相机设置、平移、缩放、拾取、高亮、点击系统
pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<HexClickEvent>()
            .add_event::<HexRightClickEvent>()
            .add_systems(Startup, setup_camera)
            .add_systems(Update, (camera_pan, camera_zoom).chain())
            .add_systems(Update, (hex_pick, hex_highlight, hex_click).chain())
            .add_systems(Update, mouse_button_diagnostic);
    }
}

// ---------------------------------------------------------------------------
// 常量
// ---------------------------------------------------------------------------

/// 相机平移速度（像素/秒，基准值，实际速度乘以 zoom）
const PAN_SPEED: f32 = 500.0;
/// 边缘滚屏触发距离（像素）
const EDGE_SCROLL_DISTANCE: f32 = 50.0;
/// 最小缩放：ortho.scale 的下限，最大放大（看清单格）。语义：scale 不能小于此值。
pub const MIN_ZOOM: f32 = 0.05;
/// 最大缩放：ortho.scale 的上限，最大放远（看全图）。语义：scale 不能大于此值。
///
/// 取 5.0 是为了让 256×256 hex 地图（≈444×384 世界单位）在 1280×720 窗口下能完整显示
/// （需要 scale ≈ 1.5），同时为更大地图（512×512）留出余量。原先 2.0 过小，
/// 256² 地图初始就会被卡在 2.0 上、无法 fit。
pub const MAX_ZOOM: f32 = 5.0;

/// 根据地图世界尺寸 + 窗口像素尺寸，计算让地图填满视口约 `margin` 比例的 ortho scale
///
/// margin=0.8 → 地图占视口 80%；越大地图越小；越小地图越大可能超出视口
///
/// **重要：Bevy 0.15 `OrthographicProjection.scale` 语义 = "可见视口乘数"**
/// - scale=1.0：可见世界 = 1.0 × window（默认）
/// - scale=0.5：可见世界 = 0.5 × window（地图看起来 2 倍大，**放大**）
/// - scale=2.0：可见世界 = 2.0 × window（地图看起来一半大，**缩小**）
/// - scale 越大 → 看到的世界越多 → 地图越小
/// - 与 `camera_zoom` 中"滚轮向上 → scale 变小 → 放大"语义一致
pub fn compute_fit_ortho_scale(map_w: f32, map_h: f32, window_w: f32, window_h: f32, margin: f32) -> f32 {
    if map_w <= 0.0 || map_h <= 0.0 || window_w <= 0.0 || window_h <= 0.0 || margin <= 0.0 {
        return 1.0;
    }
    // 目标：地图占视口的 (1/margin) → visible_world = map / margin
    //   scale × window = visible_world = map / margin
    //   scale = map / (margin × window)
    // 取 width/height 中较大值，保证两个方向都不超出视口
    let scale_for_w = map_w / (margin * window_w);
    let scale_for_h = map_h / (margin * window_h);
    let fit = scale_for_w.max(scale_for_h);
    fit.clamp(MIN_ZOOM, MAX_ZOOM)
}

// ---------------------------------------------------------------------------
// Resource / Component
// ---------------------------------------------------------------------------

/// 拾取结果 Resource：存储当前鼠标所在的 hex 坐标和世界坐标
#[derive(Resource, Default)]
pub struct HexPickResult {
    /// 当前鼠标所在的 axial hex 坐标（None 表示鼠标在窗口外）
    pub coord: Option<HexCoord>,
    /// 当前鼠标在世界坐标系中的位置
    pub world_pos: Option<Vec2>,
}

/// 高亮组件：标记拾取高亮 entity
#[derive(Component)]
pub struct HexHighlight;

// ---------------------------------------------------------------------------
// Startup：创建相机和高亮 entity
// ---------------------------------------------------------------------------

/// 设置相机和拾取高亮 entity
fn setup_camera(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    // 2D 正交相机
    commands.spawn((
        Camera2d,
        Projection::Orthographic(OrthographicProjection {
            scale: 1.0,
            ..OrthographicProjection::default_2d()
        }),
    ));

    // 拾取结果 Resource
    commands.insert_resource(HexPickResult::default());

    // 拾取高亮 entity（半透明黄色 hex，z=1 渲染在地图上方）
    let highlight_mesh = create_highlight_mesh();
    let highlight_mesh_handle = meshes.add(highlight_mesh);
    let highlight_material = materials.add(ColorMaterial {
        color: Color::srgba(1.0, 1.0, 0.0, 0.35),
        ..default()
    });

    commands.spawn((
        Mesh2d(highlight_mesh_handle),
        MeshMaterial2d(highlight_material),
        Transform::from_xyz(0.0, 0.0, 1.0),
        Visibility::Hidden,
        HexHighlight,
    ));
}

// ---------------------------------------------------------------------------
// 系统：相机平移
// ---------------------------------------------------------------------------

/// 相机平移：WASD / 方向键 + 鼠标中键拖拽
///
/// 平移速度与缩放联动：zoom 越大（看得越远），平移速度越快。
fn camera_pan(
    time: Res<Time>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse_button: Res<ButtonInput<MouseButton>>,
    mut mouse_motion: EventReader<MouseMotion>,
    mut camera_query: Query<&mut Transform, With<Camera2d>>,
    projection_query: Query<&Projection, With<Camera2d>>,
) {
    let Ok(mut transform) = camera_query.get_single_mut() else {
        return;
    };

    // 从 Projection 读取缩放值
    let zoom = match projection_query.get_single() {
        Ok(Projection::Orthographic(ortho)) => ortho.scale,
        _ => 1.0,
    };

    let dt = time.delta_secs();
    let speed = PAN_SPEED * zoom;
    let middle_held = mouse_button.pressed(MouseButton::Middle);

    // ── WASD / 方向键平移 ──
    let mut direction = Vec3::ZERO;
    if keyboard.pressed(KeyCode::KeyW) || keyboard.pressed(KeyCode::ArrowUp) {
        direction.y += 1.0;
    }
    if keyboard.pressed(KeyCode::KeyS) || keyboard.pressed(KeyCode::ArrowDown) {
        direction.y -= 1.0;
    }
    if keyboard.pressed(KeyCode::KeyA) || keyboard.pressed(KeyCode::ArrowLeft) {
        direction.x -= 1.0;
    }
    if keyboard.pressed(KeyCode::KeyD) || keyboard.pressed(KeyCode::ArrowRight) {
        direction.x += 1.0;
    }
    if direction != Vec3::ZERO {
        transform.translation += direction.normalize() * speed * dt;
    }

    // ── 鼠标中键拖拽 ──
    if middle_held {
        for ev in mouse_motion.read() {
            transform.translation.x -= ev.delta.x * zoom;
            transform.translation.y += ev.delta.y * zoom;
        }
    }
}

// ---------------------------------------------------------------------------
// 系统：相机缩放
// ---------------------------------------------------------------------------

/// 相机缩放：滚轮控制，限制 [MIN_ZOOM, MAX_ZOOM] 范围
///
/// OrthographicProjection.scale 含义：1.0 = 标准视图，<1 放大内容，>1 缩小内容。
/// 但我们的 MIN_ZOOM/MAX_ZOOM 是"缩放级别"概念，scale 与缩放成反比。
/// 实际上此处 scale 直接控制正交投影的视口大小。
fn camera_zoom(
    mut scroll_events: EventReader<MouseWheel>,
    mut projection_query: Query<&mut Projection, With<Camera2d>>,
) {
    let Ok(mut projection) = projection_query.get_single_mut() else {
        return;
    };

    for ev in scroll_events.read() {
        if let Projection::Orthographic(ref mut ortho) = *projection {
            // ev.y > 0 表示滚轮向上（放大 → scale 变小）
            ortho.scale -= ev.y * 0.1;
            ortho.scale = ortho.scale.clamp(MIN_ZOOM, MAX_ZOOM);
        }
    }
}

// ---------------------------------------------------------------------------
// 系统：hex 拾取
// ---------------------------------------------------------------------------

/// hex 拾取：屏幕坐标 → 世界坐标 → axial 坐标
///
/// 流程：
/// 1. 读取鼠标在窗口中的位置
/// 2. camera.viewport_to_world_2d() 转换为世界坐标
/// 3. world_to_hex() 转换为 axial 坐标（pointy-top 公式）
/// 4. HexCoord::round() 取最近六边形中心
///
/// 调试：设置环境变量 `HEX_PICK_DEBUG=1` 可在日志中看到每帧坐标转换链路。
fn hex_pick(
    windows: Query<&Window>,
    camera_query: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
    mouse_button: Res<ButtonInput<MouseButton>>,
    mut pick_result: ResMut<HexPickResult>,
) {
    let Ok(window) = windows.get_single() else {
        return;
    };
    let Ok((camera, camera_transform)) = camera_query.get_single() else {
        return;
    };

    let debug = std::env::var("HEX_PICK_DEBUG").as_deref() == Ok("1");

    if let Some(cursor_pos) = window.cursor_position() {
        if let Ok(world_pos) = camera.viewport_to_world_2d(camera_transform, cursor_pos) {
            let hex = world_to_hex(world_pos);
            if debug {
                info!(
                    "[hex_pick] screen=({:.1},{:.1}) → world=({:.1},{:.1}) → hex=({}, {}) | left={} right={}",
                    cursor_pos.x, cursor_pos.y,
                    world_pos.x, world_pos.y,
                    hex.q, hex.r,
                    mouse_button.pressed(MouseButton::Left),
                    mouse_button.pressed(MouseButton::Right),
                );
            }
            pick_result.coord = Some(hex);
            pick_result.world_pos = Some(world_pos);
        } else {
            if debug {
                info!(
                    "[hex_pick] screen=({:.1},{:.1}) → viewport_to_world FAILED",
                    cursor_pos.x, cursor_pos.y
                );
            }
            pick_result.coord = None;
            pick_result.world_pos = None;
        }
    } else {
        // 鼠标在窗口外
        pick_result.coord = None;
        pick_result.world_pos = None;
    }
}

/// 世界坐标 → axial 坐标转换（pointy-top hex 逆变换）
///
/// 公式来源：Red Blob Games hex grid guide
/// https://www.redblobgames.com/grids/hexagons/#pixel-to-hex
fn world_to_hex(world_pos: Vec2) -> HexCoord {
    let x = world_pos.x;
    let y = world_pos.y;
    let size = HEX_SIZE;

    // pointy-top 逆变换
    let q = (3.0_f32.sqrt() / 3.0 * x - 1.0 / 3.0 * y) / size;
    let r = (2.0 / 3.0 * y) / size;

    HexCoord::round(q as f64, r as f64, (-q - r) as f64)
}

// ---------------------------------------------------------------------------
// 系统：hex 高亮
// ---------------------------------------------------------------------------

/// 更新拾取高亮 entity 的位置和可见性
///
/// 当鼠标悬停在 hex 上时，移动高亮到该 hex 中心并显示；
/// 当鼠标在窗口外时，隐藏高亮。
fn hex_highlight(
    pick_result: Res<HexPickResult>,
    mut highlight_query: Query<(&mut Transform, &mut Visibility), With<HexHighlight>>,
) {
    for (mut transform, mut visibility) in highlight_query.iter_mut() {
        match pick_result.coord {
            Some(coord) => {
                let target = hex_world_position(coord);
                transform.translation.x = target.x;
                transform.translation.y = target.y;
                *visibility = Visibility::Visible;
            }
            None => {
                *visibility = Visibility::Hidden;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 事件 & 系统：hex 点击
// ---------------------------------------------------------------------------

/// 地图左键点击事件：当用户左键点击地图上的 hex 时触发
///
/// 仅在鼠标不在 egui 控件上时触发（bevy_egui 会消费 pointer 事件，
/// 使 Bevy 的 `just_pressed` 不会在 egui 控件区域触发）。
#[derive(Event, Debug)]
pub struct HexClickEvent {
    /// 被点击的 axial hex 坐标
    pub coord: HexCoord,
    /// 点击位置的世界坐标
    pub world_pos: Vec2,
}

/// 地图右键点击事件：当用户右键点击地图上的 hex 时触发
///
/// 用途：显示地块详情、右键菜单、部队指令等。
#[derive(Event, Debug)]
pub struct HexRightClickEvent {
    /// 被点击的 axial hex 坐标
    pub coord: HexCoord,
    /// 点击位置的世界坐标
    pub world_pos: Vec2,
}

/// hex 点击拾取系统：检测左键/右键单击，结合 HexPickResult 发送事件
///
/// 依赖 hex_pick 系统先运行，确保 HexPickResult 已更新。
///
/// 实现要点：
/// 1. **直接读原始 `MouseButtonInput` 事件**，不依赖 `ButtonInput<MouseButton>::just_pressed`。
///    后者在 bevy_egui 0.33 + Bevy 0.15 下可能被 egui 在 InputSystem 之前消费导致永远 false。
/// 2. egui 拦截判定：`is_using_pointer()`（主动按住/拖拽/刚发生 click）**或**
///    `wants_pointer_input()`（鼠标在交互控件上）时跳过本帧。
///    **不**用 `is_pointer_over_area()`——后者会因任意 egui layer（含 label）存在而屏蔽全屏。
/// 3. egui 不需要 pointer 时才把 click 转为 HexClickEvent。
fn hex_click(
    mut mouse_events: EventReader<MouseButtonInput>,
    pick_result: Res<HexPickResult>,
    mut egui_contexts: EguiContexts,
    mut left_click_events: EventWriter<HexClickEvent>,
    mut right_click_events: EventWriter<HexRightClickEvent>,
) {
    // egui 想要这次 pointer 吗？只要任一条件成立就让出
    let ctx = egui_contexts.ctx_mut();
    if ctx.is_using_pointer() || ctx.wants_pointer_input() {
        // 同时排空本帧已读到的鼠标事件，避免重复处理
        for _ in mouse_events.read() {}
        return;
    }

    // 从原始事件流里找出本帧的 L/R Pressed
    let mut left_pressed = false;
    let mut right_pressed = false;
    for ev in mouse_events.read() {
        if ev.state == ButtonState::Pressed {
            match ev.button {
                MouseButton::Left => left_pressed = true,
                MouseButton::Right => right_pressed = true,
                _ => {}
            }
        }
    }

    let Some(coord) = pick_result.coord else { return };
    let Some(world_pos) = pick_result.world_pos else { return };

    if left_pressed {
        info!(
            "[hex_click] 左键 hex=({}, {}), world=({:.1}, {:.1})",
            coord.q, coord.r, world_pos.x, world_pos.y
        );
        left_click_events.send(HexClickEvent { coord, world_pos });
    }

    if right_pressed {
        info!(
            "[hex_click] 右键 hex=({}, {}), world=({:.1}, {:.1})",
            coord.q, coord.r, world_pos.x, world_pos.y
        );
        right_click_events.send(HexRightClickEvent { coord, world_pos });
    }
}

// ---------------------------------------------------------------------------
// 诊断系统：直接检查鼠标按键状态（独立于其他系统）
// ---------------------------------------------------------------------------

/// 鼠标按键诊断系统：直接读取 ButtonInput 和 MouseButtonInput 事件
///
/// 用于验证：
/// 1. Bevy 的 ButtonInput<MouseButton> 是否正确更新
/// 2. MouseButtonInput 事件是否被发送
/// 3. just_pressed 是否正常工作
///
/// 设置 `HEX_PICK_DEBUG=1` 启用。每 60 帧输出一次（避免刷屏），
/// 但在检测到按键变化时立即输出。
fn mouse_button_diagnostic(
    mouse_button: Res<ButtonInput<MouseButton>>,
    mut mouse_events: EventReader<MouseButtonInput>,
    mut frame_counter: Local<u32>,
    mut prev_left: Local<bool>,
    mut prev_right: Local<bool>,
) {
    // 收集本帧的原始事件
    let mut raw_events = Vec::new();
    for ev in mouse_events.read() {
        raw_events.push(format!("{:?}", ev));
    }

    let left = mouse_button.pressed(MouseButton::Left);
    let right = mouse_button.pressed(MouseButton::Right);
    let left_just = mouse_button.just_pressed(MouseButton::Left);
    let right_just = mouse_button.just_pressed(MouseButton::Right);

    // 检测状态变化
    let left_changed = left != *prev_left;
    let right_changed = right != *prev_right;
    *prev_left = left;
    *prev_right = right;

    // 每 60 帧或状态变化时输出
    *frame_counter += 1;
    let should_log = *frame_counter % 60 == 0 || left_changed || right_changed;

    if should_log && std::env::var("HEX_PICK_DEBUG").as_deref() == Ok("1") {
        info!(
            "[MOUSE_DIAG] frame={} | ButtonInput: L={} L_just={} R={} R_just={} | raw_events={} | changed: L={} R={}",
            *frame_counter, left, left_just, right, right_just,
            raw_events.len(), left_changed, right_changed
        );
        for (i, ev_str) in raw_events.iter().enumerate().take(3) {
            info!("[MOUSE_DIAG]   event[{}]: {}", i, ev_str);
        }
    }
}

// ---------------------------------------------------------------------------
// 辅助函数
// ---------------------------------------------------------------------------

/// axial 坐标 → 世界坐标（pointy-top）
///
/// 与 chunk_mesh::hex_center() 数学等价，但直接接受 axial 坐标（i32），
/// 无需转换为 offset 坐标（u32），支持负坐标。
///
/// 公式：
///   x = sqrt(3) * q + sqrt(3)/2 * r
///   y = 1.5 * r
fn hex_world_position(coord: HexCoord) -> Vec2 {
    let q = coord.q as f32;
    let r = coord.r as f32;
    Vec2::new(3.0_f32.sqrt() * q + 3.0_f32.sqrt() / 2.0 * r, 1.5 * r)
}

/// 创建拾取高亮 mesh（半透明六边形，比地形 hex 略大）
fn create_highlight_mesh() -> Mesh {
    let scale = 1.03; // 比地形 hex 大 3%，形成描边效果

    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(7);
    let mut normals: Vec<[f32; 3]> = Vec::with_capacity(7);
    let mut uvs: Vec<[f32; 2]> = Vec::with_capacity(7);
    let mut indices: Vec<u32> = Vec::with_capacity(18);

    // 中心顶点
    positions.push([0.0, 0.0, 0.0]);
    normals.push([0.0, 0.0, 1.0]);
    uvs.push([0.5, 0.5]);

    // 6 个角顶点（pointy-top：起始角 30 度）
    for i in 0..6 {
        let angle = std::f32::consts::FRAC_PI_3 * i as f32 + std::f32::consts::FRAC_PI_6;
        let x = HEX_SIZE * scale * angle.cos();
        let y = HEX_SIZE * scale * angle.sin();
        positions.push([x, y, 0.0]);
        normals.push([0.0, 0.0, 1.0]);
        uvs.push([0.5 + 0.5 * angle.cos(), 0.5 + 0.5 * angle.sin()]);
    }

    // 6 个三角形（扇形）
    for i in 0..6 {
        indices.push(0);
        indices.push(1 + i);
        indices.push(1 + (i + 1) % 6);
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

// ---------------------------------------------------------------------------
// 单元测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// 128×128 hex 地图在 1280×720 窗口下、margin=0.8 的 fit scale
    /// 地图 ≈ 222×192 世界单位
    /// 80% 填充：scale = max(222/(0.8*1280), 192/(0.8*720)) = max(0.217, 0.333) = 0.333
    /// 验算：可见视口 = 0.333 × 720 = 240 世界单位，192/240 = 80% ✓
    #[test]
    fn fit_scale_128_hex_in_720p_80pct() {
        let s = compute_fit_ortho_scale(222.0, 192.0, 1280.0, 720.0, 0.8);
        assert!(
            (s - 0.333).abs() < 0.01,
            "expected ≈ 0.333 for 128x128 hex map in 1280x720 (80% margin), got {}",
            s
        );
    }

    /// 256×256 hex 地图在 1280×720 窗口、margin=0.8
    /// 地图 ≈ 444×384 世界单位
    /// 80% 填充：scale = max(444/(0.8*1280), 384/(0.8*720)) = max(0.434, 0.667) = 0.667
    #[test]
    fn fit_scale_256_hex_in_720p_80pct() {
        let s = compute_fit_ortho_scale(444.0, 384.0, 1280.0, 720.0, 0.8);
        assert!(
            (s - 0.667).abs() < 0.01,
            "expected ≈ 0.667 for 256x256 hex map in 1280x720 (80% margin), got {}",
            s
        );
    }

    /// 128×128 hex 地图 margin=1.0 → 填满视口高度
    /// scale = max(222/1280, 192/720) = max(0.173, 0.267) = 0.267
    /// 验算：可见视口 = 0.267 × 720 = 192 世界单位，192/192 = 100% ✓
    #[test]
    fn fit_scale_128_hex_in_720p_full() {
        let s = compute_fit_ortho_scale(222.0, 192.0, 1280.0, 720.0, 1.0);
        assert!(
            (s - 0.267).abs() < 0.01,
            "expected ≈ 0.267 for 128x128 hex map in 1280x720 (full margin), got {}",
            s
        );
    }

    /// 极端 case：地图比窗口小很多 → scale 应被 clamp 到 MIN_ZOOM（最大放大）
    /// tiny 地图 80% 填充需要 scale ≈ 0.003 < MIN_ZOOM=0.05 → clamp
    #[test]
    fn fit_scale_tiny_map_clamped_to_min_zoom() {
        let s = compute_fit_ortho_scale(2.0, 2.0, 1280.0, 720.0, 0.8);
        assert_eq!(s, MIN_ZOOM, "tiny map should be clamped to MIN_ZOOM (max zoom in)");
    }

    /// 极端 case：地图比窗口大很多 → scale 应被 clamp 到 MAX_ZOOM（最大放远）
    /// huge 地图 80% 填充需要 scale ≈ 34.7 > MAX_ZOOM=5.0 → clamp
    #[test]
    fn fit_scale_huge_map_clamped_to_max_zoom() {
        let s = compute_fit_ortho_scale(20000.0, 20000.0, 1280.0, 720.0, 0.8);
        assert_eq!(s, MAX_ZOOM, "huge map should be clamped to MAX_ZOOM (max zoom out)");
    }

    /// 退化输入：零/负数 → 兜底返回 1.0
    #[test]
    fn fit_scale_degenerate_inputs() {
        assert_eq!(compute_fit_ortho_scale(0.0, 100.0, 1280.0, 720.0, 0.8), 1.0);
        assert_eq!(compute_fit_ortho_scale(100.0, 0.0, 1280.0, 720.0, 0.8), 1.0);
        assert_eq!(compute_fit_ortho_scale(100.0, 100.0, 0.0, 720.0, 0.8), 1.0);
        assert_eq!(compute_fit_ortho_scale(100.0, 100.0, 1280.0, 0.0, 0.8), 1.0);
        assert_eq!(compute_fit_ortho_scale(100.0, 100.0, 1280.0, 720.0, 0.0), 1.0);
        assert_eq!(compute_fit_ortho_scale(100.0, 100.0, 1280.0, 720.0, -0.5), 1.0);
    }

    /// 正方形地图 + 正方形窗口（不触发 clamp）
    /// 100×100 地图塞进 600×600 窗口，80% 填充
    /// scale = 100 / (0.8 × 600) = 0.208
    #[test]
    fn fit_scale_square_into_square() {
        let s = compute_fit_ortho_scale(100.0, 100.0, 600.0, 600.0, 0.8);
        assert!((s - 0.208).abs() < 0.005, "expected ≈ 0.208, got {}", s);
    }

    /// 关键不变量：map 越大，fit scale 越大（看到的世界越多才能装下大地图）
    #[test]
    fn fit_scale_monotonic_in_map_size() {
        let s128 = compute_fit_ortho_scale(222.0, 192.0, 1280.0, 720.0, 1.0);
        let s256 = compute_fit_ortho_scale(444.0, 384.0, 1280.0, 720.0, 1.0);
        let s512 = compute_fit_ortho_scale(888.0, 768.0, 1280.0, 720.0, 1.0);
        assert!(s128 < s256 && s256 < s512, "scale should grow with map size: {} < {} < {}", s128, s256, s512);
    }
}
