//! Bevy 游戏系统：tick_dispatcher、暂停/变速控制
//!
//! 将 slg-core 的纯逻辑时钟桥接到 Bevy ECS 中。
//! 每渲染帧调用 tick_dispatcher，按 accumulator 模型驱动逻辑 tick。

use bevy::prelude::*;
use slg_core::clock::*;
use slg_core::resource::{CommandQueue, GameClock, Speed};

// ---------------------------------------------------------------------------
// Bevy Resource 包装
// ---------------------------------------------------------------------------

/// Bevy Resource 包装 GameClock
#[derive(Resource, Default)]
pub struct GameClockResource {
    pub clock: GameClock,
}

/// Bevy Resource 包装 CommandQueue
#[derive(Resource, Default)]
pub struct CommandQueueResource {
    pub queue: CommandQueue,
}

// ---------------------------------------------------------------------------
// ClockPlugin
// ---------------------------------------------------------------------------

/// 时钟插件，注册到 SlgEnginePlugin
pub struct ClockPlugin;

impl Plugin for ClockPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GameClockResource>()
            .init_resource::<CommandQueueResource>()
            .add_systems(Update, tick_dispatcher)
            .add_systems(Update, handle_speed_controls);
    }
}

// ---------------------------------------------------------------------------
// tick_dispatcher
// ---------------------------------------------------------------------------

/// 每渲染帧调用，推进逻辑时钟。
///
/// 核心循环：
/// 1. 取帧时间增量，乘以速度倍率累加到 accumulator
/// 2. 每积累 TICK_DURATION_MS 就消耗一个逻辑 tick
/// 3. 对每个 tick 按 TICK_PHASES 顺序执行 GameTickSchedule
pub fn tick_dispatcher(
    time: Res<Time>,
    mut clock_res: ResMut<GameClockResource>,
    mut commands_res: ResMut<CommandQueueResource>,
) {
    let frame_delta_ms = time.delta_secs_f64() * 1000.0;
    let ticks = advance_clock(&mut clock_res.clock, frame_delta_ms);

    for _ in 0..ticks {
        run_tick_schedule(&mut clock_res.clock, &mut commands_res.queue);
    }
}

/// 执行单个 tick 的所有阶段（GameTickSchedule）
fn run_tick_schedule(clock: &mut GameClock, commands: &mut CommandQueue) {
    for phase in TICK_PHASES {
        match phase {
            TickPhase::TickStart => {
                // 注入暂停时入队的指令
                // TODO(M1-T08+): 从 commands.commands 取出并分发到各系统
                let _ = commands;
            }
            TickPhase::ResourceProduction => {
                // TODO(M1-T09): 调用经济系统产出资源
                let _ = clock;
            }
            TickPhase::BuildQueue => {
                // TODO(M1-T10): 推进建造队列
            }
            TickPhase::Recruitment => {
                // TODO(M1-T11): 征兵逻辑
            }
            TickPhase::MarchAdvance => {
                // TODO(M1-T12): 行军推进
            }
            TickPhase::CombatResolution => {
                // TODO(M1-T13): 战斗结算
            }
            TickPhase::TerritoryUpdate => {
                // TODO(M1-T14): 领地更新
            }
            TickPhase::AIDecision => {
                // TODO(M1-T15): AI 决策（错峰：faction i 在 tick%10==i 决策）
                // let faction_slot = ...;
                // if should_ai_decide(clock.current_tick, faction_slot) { ... }
            }
            TickPhase::TickEnd => {
                // TODO(M1-T16): 迷雾更新、事件触发
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 键盘控制
// ---------------------------------------------------------------------------

/// 键盘控制：Space 暂停/恢复，1/2/3 变速
fn handle_speed_controls(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut clock_res: ResMut<GameClockResource>,
) {
    if keyboard.just_pressed(KeyCode::Space) {
        clock_res.clock.speed = match clock_res.clock.speed {
            Speed::Paused => Speed::X1,
            _ => Speed::Paused,
        };
    }
    if keyboard.just_pressed(KeyCode::Digit1) {
        clock_res.clock.speed = Speed::X1;
    }
    if keyboard.just_pressed(KeyCode::Digit2) {
        clock_res.clock.speed = Speed::X2;
    }
    if keyboard.just_pressed(KeyCode::Digit3) {
        clock_res.clock.speed = Speed::X3;
    }
}
