//! 游戏统计追踪器
//!
//! 纯数据结构，不依赖 Bevy。
//! 记录玩家在单局游戏中的各项数据，用于游戏结束画面展示。

use serde::{Deserialize, Serialize};

/// 游戏统计
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GameStatistics {
    /// 总 tick 数
    pub total_ticks: u64,
    /// 占领格数
    pub tiles_occupied: u32,
    /// 丢失格数
    pub tiles_lost: u32,
    /// 战斗次数
    pub battles_fought: u32,
    /// 胜利次数
    pub battles_won: u32,
    /// 招募武将数
    pub generals_recruited: u32,
    /// 失去武将数
    pub generals_lost: u32,
    /// 结盟次数
    pub alliances_formed: u32,
    /// 破盟次数
    pub alliances_broken: u32,
    /// 峰值领地格数
    pub peak_territory: u32,
    /// 峰值军队数
    pub peak_armies: u32,
    /// 触发事件数
    pub events_triggered: u32,
    /// 累计获得金币
    pub total_gold_earned: u64,
    /// 累计消耗粮食
    pub total_food_consumed: u64,
}

impl GameStatistics {
    /// 记录战斗
    pub fn record_battle(&mut self, won: bool) {
        self.battles_fought += 1;
        if won {
            self.battles_won += 1;
        }
    }

    /// 记录领地变化
    pub fn record_territory_change(&mut self, gained: u32, lost: u32) {
        self.tiles_occupied += gained;
        self.tiles_lost += lost;
    }

    /// 更新峰值
    pub fn update_peaks(&mut self, current_territory: u32, current_armies: u32) {
        self.peak_territory = self.peak_territory.max(current_territory);
        self.peak_armies = self.peak_armies.max(current_armies);
    }

    /// 获取胜率
    pub fn win_rate(&self) -> f64 {
        if self.battles_fought == 0 {
            0.0
        } else {
            self.battles_won as f64 / self.battles_fought as f64
        }
    }

    /// 获取游戏天数（假设 10 tick = 1 天）
    pub fn game_days(&self) -> u64 {
        self.total_ticks / 10
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_battle() {
        let mut stats = GameStatistics::default();
        stats.record_battle(true);
        stats.record_battle(false);
        stats.record_battle(true);
        assert_eq!(stats.battles_fought, 3);
        assert_eq!(stats.battles_won, 2);
    }

    #[test]
    fn test_win_rate_zero_battles() {
        let stats = GameStatistics::default();
        assert_eq!(stats.win_rate(), 0.0);
    }

    #[test]
    fn test_win_rate() {
        let mut stats = GameStatistics::default();
        stats.record_battle(true);
        stats.record_battle(false);
        assert!((stats.win_rate() - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_game_days() {
        let stats = GameStatistics {
            total_ticks: 150,
            ..Default::default()
        };
        assert_eq!(stats.game_days(), 15);
    }

    #[test]
    fn test_record_territory_change() {
        let mut stats = GameStatistics::default();
        stats.record_territory_change(5, 2);
        assert_eq!(stats.tiles_occupied, 5);
        assert_eq!(stats.tiles_lost, 2);
    }

    #[test]
    fn test_update_peaks() {
        let mut stats = GameStatistics::default();
        stats.update_peaks(10, 50);
        stats.update_peaks(5, 80);
        stats.update_peaks(15, 60);
        assert_eq!(stats.peak_territory, 15);
        assert_eq!(stats.peak_armies, 80);
    }
}
