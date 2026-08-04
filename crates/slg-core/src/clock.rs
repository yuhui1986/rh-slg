//! 游戏时钟与 tick 调度
//!
//! 纯 Rust 逻辑，零 Bevy 依赖。
//! 定义 tick 阶段顺序、时钟推进算法、AI 错峰判定、渲染插值。

use serde::{Deserialize, Serialize};

use crate::resource::{GameClock, Speed};

// ---------------------------------------------------------------------------
// 常量
// ---------------------------------------------------------------------------

/// 每 tick 持续时间（毫秒）
pub const TICK_DURATION_MS: f64 = 100.0;

// ---------------------------------------------------------------------------
// Tick 阶段
// ---------------------------------------------------------------------------

/// Tick 调度阶段
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TickPhase {
    TickStart,          // 指令注入
    ResourceProduction, // 资源产出
    BuildQueue,         // 建造队列推进
    Recruitment,        // 征兵
    MarchAdvance,       // 行军推进
    CombatResolution,   // 战斗结算
    TerritoryUpdate,    // 领地更新
    AIDecision,         // AI 决策（错峰）
    TickEnd,            // 迷雾更新、事件触发
}

/// GameTickSchedule：定义每 tick 的执行阶段顺序
pub const TICK_PHASES: &[TickPhase] = &[
    TickPhase::TickStart,
    TickPhase::ResourceProduction,
    TickPhase::BuildQueue,
    TickPhase::Recruitment,
    TickPhase::MarchAdvance,
    TickPhase::CombatResolution,
    TickPhase::TerritoryUpdate,
    TickPhase::AIDecision,
    TickPhase::TickEnd,
];

// ---------------------------------------------------------------------------
// 时钟推进
// ---------------------------------------------------------------------------

/// 推进时钟，返回本次推进产生的 tick 数量。
///
/// - 暂停时 accumulator 不增长，返回 0。
/// - accumulator 每积累 TICK_DURATION_MS 就消耗一个 tick。
pub fn advance_clock(clock: &mut GameClock, frame_delta_ms: f64) -> u32 {
    if clock.speed == Speed::Paused {
        return 0;
    }

    let multiplier = clock.speed.multiplier();
    clock.accumulator += frame_delta_ms * multiplier;

    let mut ticks = 0;
    while clock.accumulator >= TICK_DURATION_MS {
        clock.accumulator -= TICK_DURATION_MS;
        clock.current_tick += 1;
        ticks += 1;
    }

    ticks
}

// ---------------------------------------------------------------------------
// AI 错峰
// ---------------------------------------------------------------------------

/// 检查当前 tick 是否轮到指定势力执行 AI 决策。
///
/// 使用 `tick % 10 == faction_slot` 的简单错峰策略。
pub fn should_ai_decide(tick: u64, faction_slot: u8) -> bool {
    (tick % 10) == faction_slot as u64
}

// ---------------------------------------------------------------------------
// 渲染插值
// ---------------------------------------------------------------------------

/// 获取渲染插值值，用于平滑动画。
///
/// 返回 `current_tick + accumulator / TICK_DURATION_MS`，
/// 使渲染侧可以在两个逻辑 tick 之间做渐变。
pub fn get_interpolation(clock: &GameClock) -> f64 {
    clock.current_tick as f64 + clock.accumulator / TICK_DURATION_MS
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_clock(speed: Speed) -> GameClock {
        GameClock {
            current_tick: 0,
            speed,
            accumulator: 0.0,
        }
    }

    #[test]
    fn test_advance_clock_x1() {
        let mut clock = make_clock(Speed::X1);
        // 1 秒 = 1000ms / 100ms = 10 tick @ x1
        let ticks = advance_clock(&mut clock, 1000.0);
        assert_eq!(ticks, 10);
        assert_eq!(clock.current_tick, 10);
        assert!((clock.accumulator - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_advance_clock_x2() {
        let mut clock = make_clock(Speed::X2);
        // 1 秒 = 20 tick @ x2
        let ticks = advance_clock(&mut clock, 1000.0);
        assert_eq!(ticks, 20);
        assert_eq!(clock.current_tick, 20);
    }

    #[test]
    fn test_advance_clock_x3() {
        let mut clock = make_clock(Speed::X3);
        // 1 秒 = 30 tick @ x3
        let ticks = advance_clock(&mut clock, 1000.0);
        assert_eq!(ticks, 30);
        assert_eq!(clock.current_tick, 30);
    }

    #[test]
    fn test_paused_no_advance() {
        let mut clock = make_clock(Speed::Paused);
        let ticks = advance_clock(&mut clock, 1000.0);
        assert_eq!(ticks, 0);
        assert_eq!(clock.current_tick, 0);
        assert!((clock.accumulator - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_accumulator_carry_over() {
        let mut clock = make_clock(Speed::X1);
        // 第一帧 150ms => 1 tick, accumulator 剩余 50ms
        let ticks = advance_clock(&mut clock, 150.0);
        assert_eq!(ticks, 1);
        assert_eq!(clock.current_tick, 1);
        assert!((clock.accumulator - 50.0).abs() < f64::EPSILON);

        // 第二帧 60ms => 累积 110ms => 又 1 tick, accumulator 剩余 10ms
        let ticks = advance_clock(&mut clock, 60.0);
        assert_eq!(ticks, 1);
        assert_eq!(clock.current_tick, 2);
        assert!((clock.accumulator - 10.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_pause_and_resume() {
        let mut clock = make_clock(Speed::X1);
        // 正常推进 0.5 秒 => 5 tick
        let ticks = advance_clock(&mut clock, 500.0);
        assert_eq!(ticks, 5);
        assert_eq!(clock.current_tick, 5);

        // 暂停
        clock.speed = Speed::Paused;
        let ticks = advance_clock(&mut clock, 5000.0);
        assert_eq!(ticks, 0);
        assert_eq!(clock.current_tick, 5);

        // 恢复 x1，再推 0.5 秒 => 又 5 tick
        clock.speed = Speed::X1;
        let ticks = advance_clock(&mut clock, 500.0);
        assert_eq!(ticks, 5);
        assert_eq!(clock.current_tick, 10);
    }

    #[test]
    fn test_should_ai_decide() {
        // faction 0: tick 0, 10, 20 ...
        assert!(should_ai_decide(0, 0));
        assert!(should_ai_decide(10, 0));
        assert!(!should_ai_decide(1, 0));

        // faction 1: tick 1, 11, 21 ...
        assert!(should_ai_decide(1, 1));
        assert!(should_ai_decide(11, 1));
        assert!(!should_ai_decide(0, 1));

        // faction 9: tick 9, 19, 29 ...
        assert!(should_ai_decide(9, 9));
        assert!(should_ai_decide(19, 9));
        assert!(!should_ai_decide(0, 9));
    }

    #[test]
    fn test_interpolation() {
        let clock = GameClock {
            current_tick: 5,
            speed: Speed::X1,
            accumulator: 50.0,
        };
        let interp = get_interpolation(&clock);
        // 5 + 50/100 = 5.5
        assert!((interp - 5.5).abs() < 0.01);
    }

    #[test]
    fn test_interpolation_at_tick_boundary() {
        let clock = GameClock {
            current_tick: 10,
            speed: Speed::X2,
            accumulator: 0.0,
        };
        let interp = get_interpolation(&clock);
        assert!((interp - 10.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_tick_phases_order() {
        // 确保 TICK_PHASES 有 9 个阶段且顺序正确
        assert_eq!(TICK_PHASES.len(), 9);
        assert_eq!(TICK_PHASES[0], TickPhase::TickStart);
        assert_eq!(TICK_PHASES[8], TickPhase::TickEnd);
    }
}
