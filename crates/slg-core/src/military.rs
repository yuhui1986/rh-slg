//! 军事系统：派兵行军 + 军队管理
//!
//! 率土核心动作：**派兵 → 行军 → 落地**。不再是"点空地瞬时变色"，
//! 玩家点邻接空地后，spawn 一个 MarchOrder，兵从主城沿路径走到目标，
//! 走完才触发 occupy。
//!
//! 设计原则：
//! - 纯 Rust，零 Bevy 依赖（行军推进系统放 slg-app 调用）
//! - tick-based：每 hex 固定 `TICKS_PER_HEX` tick，方便和 GameClock 对齐
//! - 路径用 [`HexCoord::line`]（axial line draw）——非最短，但视觉够用
//! - 行军中的目标格锁住，避免被 NPC 抢占

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use slg_data::ids::{FactionId, TileKey};

use crate::map::grid::HexCoord;

// ---------------------------------------------------------------------------
// 常量
// ---------------------------------------------------------------------------

/// 行军每走一个 hex 需要的 tick 数（@TICK_DURATION_MS=100ms → 5 tick = 0.5s/hex）
///
/// MVP 固定值。未来根据兵种 / 武将速度调整。
pub const TICKS_PER_HEX: u64 = 5;

/// MVP 兵力：每队兵固定 100 人
///
/// 真实系统按武将带兵量计算。MVP 简化。
pub const TROOPS_PER_MARCH: u32 = 100;

// ---------------------------------------------------------------------------
// 数据结构
// ---------------------------------------------------------------------------

/// 行军状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarchStatus {
    /// 行军中
    Marching,
    /// 到达（即将触发 occupy）
    Arrived,
    /// 玩家取消（回城 / 撤回）
    Cancelled,
    /// 失败（目标被先占 / 主城被推 / 路径断了）
    Failed,
}

/// 单支行军队
///
/// 一支 MarchOrder = 一队兵从 `from` 到 `to` 的行军。
/// `path` 是 hex 路径（含起点 from 和终点 to）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarchOrder {
    /// 唯一 ID（spawn 时用 `MarchId::new()`）
    pub id: u64,
    /// 所属势力
    pub faction_id: FactionId,
    /// 出发地（主城或当前占领格）
    pub from: HexCoord,
    /// 目标地
    pub to: HexCoord,
    /// 兵力
    pub troops: u32,
    /// hex 路径（含 from 和 to），由 [`compute_path`] 生成
    pub path: Vec<HexCoord>,
    /// 出发 tick
    pub depart_tick: u64,
    /// 预计到达 tick
    pub arrive_tick: u64,
    /// 当前状态
    pub status: MarchStatus,
}

impl MarchOrder {
    /// 创建新的行军
    ///
    /// 路径由 [`HexCoord::line`] 计算（axial line draw），MVP 简化版。
    /// 到达 tick = depart_tick + (path.len() - 1) * [`TICKS_PER_HEX`]
    pub fn new(
        id: u64,
        faction_id: FactionId,
        from: HexCoord,
        to: HexCoord,
        troops: u32,
        current_tick: u64,
    ) -> Self {
        let path = compute_path(from, to);
        let steps = if path.is_empty() { 0 } else { path.len() as u64 - 1 };
        let arrive_tick = current_tick + steps * TICKS_PER_HEX;
        Self {
            id,
            faction_id,
            from,
            to,
            troops,
            path,
            depart_tick: current_tick,
            arrive_tick,
            status: MarchStatus::Marching,
        }
    }

    /// 当前行军进度（0.0 = 出发, 1.0 = 到达）
    ///
    /// 用于渲染插值：sprite 位置 = lerp(from, to, progress)
    pub fn progress(&self, current_tick: u64) -> f32 {
        if self.arrive_tick <= self.depart_tick {
            return 1.0;
        }
        let elapsed = current_tick.saturating_sub(self.depart_tick) as f32;
        let total = (self.arrive_tick - self.depart_tick) as f32;
        (elapsed / total).clamp(0.0, 1.0)
    }

    /// 是否已到达
    pub fn is_arrived(&self, current_tick: u64) -> bool {
        current_tick >= self.arrive_tick
    }
}

// ---------------------------------------------------------------------------
// 全局管理器
// ---------------------------------------------------------------------------

/// 行军 ID 分配器
#[derive(Debug, Clone, Default)]
pub struct MarchIdAllocator {
    next: u64,
}

impl MarchIdAllocator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn next_id(&mut self) -> u64 {
        let id = self.next;
        self.next += 1;
        id
    }
}

/// 全局行军管理器
///
/// 维护所有活跃的 MarchOrder + ID 分配器。
/// `slg-app` 每 tick 调用 [`advance_all`] 推进。
#[derive(Debug, Clone, Default)]
pub struct MarchManager {
    /// 所有行军（按 id 索引）
    pub orders: BTreeMap<u64, MarchOrder>,
    /// ID 分配器
    pub id_alloc: MarchIdAllocator,
}

impl MarchManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// 派出一支新军
    ///
    /// 返回派出的 MarchOrder（id 已分配）。调用方需要把它 spawn 成 entity。
    pub fn dispatch(
        &mut self,
        faction_id: FactionId,
        from: HexCoord,
        to: HexCoord,
        troops: u32,
        current_tick: u64,
    ) -> MarchOrder {
        let id = self.id_alloc.next_id();
        let order = MarchOrder::new(id, faction_id, from, to, troops, current_tick);
        self.orders.insert(id, order.clone());
        order
    }

    /// 推进所有行军
    ///
    /// 每 tick 调用一次。把已到达的 mark 成 `Arrived`（调用方负责触发 occupy + despawn）。
    ///
    /// 返回本 tick 到达的 MarchOrder 列表（id, faction, coord）。
    pub fn advance_all(&mut self, current_tick: u64) -> Vec<MarchArrival> {
        let mut arrivals = Vec::new();
        for order in self.orders.values_mut() {
            if order.status != MarchStatus::Marching {
                continue;
            }
            if order.is_arrived(current_tick) {
                order.status = MarchStatus::Arrived;
                arrivals.push(MarchArrival {
                    id: order.id,
                    faction_id: order.faction_id.clone(),
                    to: order.to,
                    troops: order.troops,
                });
            }
        }
        arrivals
    }

    /// 取消行军
    pub fn cancel(&mut self, id: u64) -> bool {
        if let Some(order) = self.orders.get_mut(&id) {
            order.status = MarchStatus::Cancelled;
            true
        } else {
            false
        }
    }

    /// 标记行军失败（目标被先占等）
    pub fn fail(&mut self, id: u64) {
        if let Some(order) = self.orders.get_mut(&id) {
            order.status = MarchStatus::Failed;
        }
    }

    /// 清理已完成的行军（status != Marching）
    ///
    /// 在 `advance_all` 之后调用，把已到达 / 已取消的清掉，避免累积。
    pub fn cleanup_finished(&mut self) {
        self.orders
            .retain(|_, o| o.status == MarchStatus::Marching);
    }

    /// 获取所有活跃行军
    pub fn active(&self) -> impl Iterator<Item = &MarchOrder> {
        self.orders
            .values()
            .filter(|o| o.status == MarchStatus::Marching)
    }

    /// 检查目标格是否被某支行军锁住
    ///
    /// 防止两支兵同时飞向同一格。
    pub fn is_target_locked(&self, target: HexCoord) -> bool {
        self.active().any(|o| o.to == target)
    }
}

/// 行军到达事件（advance_all 返回）
#[derive(Debug, Clone)]
pub struct MarchArrival {
    pub id: u64,
    pub faction_id: FactionId,
    pub to: HexCoord,
    pub troops: u32,
}

// ---------------------------------------------------------------------------
// 路径计算
// ---------------------------------------------------------------------------

/// 计算从 `from` 到 `to` 的 hex 路径
///
/// MVP 用 [`HexCoord::line`]（axial line draw）。
/// 总是包含 `from` 和 `to`。1-step 路径 = [from, to]（2 个元素）。
pub fn compute_path(from: HexCoord, to: HexCoord) -> Vec<HexCoord> {
    if from == to {
        return vec![from];
    }
    HexCoord::line(from, to)
}

/// 检查目标格是否可以从 `from` 行军过去
///
/// MVP 规则：
/// - 距离 ≥ 1 hex
/// - 路径上的所有格都已注册到 territory（[TileKey] in key_to_index）
///
/// 后续可加：地形（不可通行）、中立主城等。
pub fn can_march_to(
    from: HexCoord,
    to: HexCoord,
    registered_keys: &std::collections::BTreeSet<TileKey>,
) -> bool {
    if from == to {
        return false;
    }
    let path = compute_path(from, to);
    path.iter()
        .all(|c| registered_keys.contains(&c.to_tile_key()))
}

// ---------------------------------------------------------------------------
// AI 决策辅助：找邻接可占领空地
// ---------------------------------------------------------------------------

/// AI 找该势力的扩张目标：从主城邻接 6 格里找第一个可占领（can_occupy）的空地
///
/// # 决策策略（M0 简化版）
/// - 遍历主城 6 邻接
/// - 用 territory.can_occupy 判定（要求：空地 / 6 邻有己方 / 与主城连通）
/// - 第一个满足的格作为目标
///
/// 返回 `Some(coord)` 表示有目标可派兵，`None` 表示满了/被卡。
///
/// # 后续可加
/// - persona（魏更激进，吴更保守）
/// - 资源约束（粮 < 100 不扩张）
/// - 优先占领资源格
pub fn ai_expansion_target(
    faction_id: &slg_data::ids::FactionId,
    faction_main_city: HexCoord,
    territory: &mut crate::map::territory::TerritoryManager,
    terrain_map: &std::collections::BTreeMap<TileKey, crate::map::tile::TerrainType>,
) -> Option<HexCoord> {
    faction_main_city
        .neighbors()
        .iter()
        .find(|n| territory.can_occupy(**n, faction_id, terrain_map))
        .copied()
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_path_adjacent() {
        let from = HexCoord::new(0, 0);
        let to = HexCoord::new(1, 0);
        let path = compute_path(from, to);
        // 邻接：1 步路径
        assert_eq!(path.first(), Some(&from));
        assert_eq!(path.last(), Some(&to));
        // 至少 2 个点（from + to）
        assert!(path.len() >= 2, "1-step path should have at least 2 points");
    }

    #[test]
    fn test_compute_path_same() {
        let p = HexCoord::new(5, 5);
        let path = compute_path(p, p);
        assert_eq!(path, vec![p]);
    }

    #[test]
    fn test_arrive_tick_one_hex() {
        let order = MarchOrder::new(
            1,
            "faction_1".to_string(),
            HexCoord::new(0, 0),
            HexCoord::new(1, 0),
            TROOPS_PER_MARCH,
            100, // depart_tick
        );
        // 1 步 → 5 tick
        assert_eq!(order.arrive_tick, 105);
        assert_eq!(order.path.len(), 2);
    }

    #[test]
    fn test_arrive_tick_five_hex() {
        let order = MarchOrder::new(
            1,
            "faction_1".to_string(),
            HexCoord::new(0, 0),
            HexCoord::new(5, 0),
            TROOPS_PER_MARCH,
            100,
        );
        // 5 步 → 25 tick
        assert_eq!(order.arrive_tick, 125);
    }

    #[test]
    fn test_progress_interpolation() {
        let order = MarchOrder::new(
            1,
            "faction_1".to_string(),
            HexCoord::new(0, 0),
            HexCoord::new(5, 0),
            TROOPS_PER_MARCH,
            100,
        );
        // 100 出发, 125 到达 (5 步 × 5 tick = 25 tick 总长)
        assert!((order.progress(100) - 0.0).abs() < 0.01);
        // 中点 (12.5 ticks) 用 tick 112 (progress = 12/25 = 0.48) 验证在 [0.4, 0.6]
        let mid = order.progress(112);
        assert!(mid > 0.4 && mid < 0.6, "mid progress should be ~0.5, got {}", mid);
        assert!((order.progress(125) - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_march_manager_dispatch_and_advance() {
        let mut mgr = MarchManager::new();
        let order = mgr.dispatch(
            "faction_1".to_string(),
            HexCoord::new(0, 0),
            HexCoord::new(1, 0),
            TROOPS_PER_MARCH,
            100,
        );
        assert_eq!(order.arrive_tick, 105);
        assert_eq!(mgr.orders.len(), 1);

        // tick 100: 行军中，无 arrival
        let arrivals = mgr.advance_all(100);
        assert!(arrivals.is_empty());
        assert_eq!(
            mgr.orders.get(&order.id).unwrap().status,
            MarchStatus::Marching
        );

        // tick 105: 到达
        let arrivals = mgr.advance_all(105);
        assert_eq!(arrivals.len(), 1);
        assert_eq!(arrivals[0].to, HexCoord::new(1, 0));
        assert_eq!(
            mgr.orders.get(&order.id).unwrap().status,
            MarchStatus::Arrived
        );
    }

    #[test]
    fn test_march_manager_cancel() {
        let mut mgr = MarchManager::new();
        let order = mgr.dispatch(
            "faction_1".to_string(),
            HexCoord::new(0, 0),
            HexCoord::new(1, 0),
            TROOPS_PER_MARCH,
            100,
        );
        assert!(mgr.cancel(order.id));
        assert_eq!(
            mgr.orders.get(&order.id).unwrap().status,
            MarchStatus::Cancelled
        );
    }

    #[test]
    fn test_march_manager_cleanup() {
        let mut mgr = MarchManager::new();
        let order = mgr.dispatch(
            "faction_1".to_string(),
            HexCoord::new(0, 0),
            HexCoord::new(1, 0),
            TROOPS_PER_MARCH,
            100,
        );
        mgr.advance_all(105); // mark Arrived
        mgr.cleanup_finished();
        assert!(mgr.orders.is_empty(), "Arrived 之后 cleanup_finished 应清掉");
        let _ = order;
    }

    #[test]
    fn test_target_locked() {
        let mut mgr = MarchManager::new();
        let _order = mgr.dispatch(
            "faction_1".to_string(),
            HexCoord::new(0, 0),
            HexCoord::new(1, 0),
            TROOPS_PER_MARCH,
            100,
        );
        // (1, 0) 正在被飞向
        assert!(mgr.is_target_locked(HexCoord::new(1, 0)));
        // (2, 0) 没被锁
        assert!(!mgr.is_target_locked(HexCoord::new(2, 0)));
    }

    #[test]
    fn test_can_march_to_path_check() {
        let mut registered = std::collections::BTreeSet::new();
        registered.insert(HexCoord::new(0, 0).to_tile_key());
        registered.insert(HexCoord::new(1, 0).to_tile_key());
        registered.insert(HexCoord::new(2, 0).to_tile_key());
        assert!(can_march_to(
            HexCoord::new(0, 0),
            HexCoord::new(2, 0),
            &registered
        ));
        // 目标格未注册 → false
        let mut partial = registered.clone();
        partial.remove(&HexCoord::new(2, 0).to_tile_key());
        assert!(!can_march_to(
            HexCoord::new(0, 0),
            HexCoord::new(2, 0),
            &partial
        ));
    }

    #[test]
    fn test_id_allocator_unique() {
        let mut alloc = MarchIdAllocator::new();
        let a = alloc.next_id();
        let b = alloc.next_id();
        let c = alloc.next_id();
        assert_ne!(a, b);
        assert_ne!(b, c);
        assert!(a < b && b < c);
    }

    // -----------------------------------------------------------------------
    // AI 决策辅助：ai_expansion_target
    // -----------------------------------------------------------------------

    fn key(q: i32, r: i32) -> TileKey {
        HexCoord::new(q, r).to_tile_key()
    }

    fn make_full_plains_terrain(w: i32, h: i32) -> std::collections::BTreeMap<TileKey, crate::map::tile::TerrainType> {
        let mut m = std::collections::BTreeMap::new();
        for q in 0..w {
            for r in 0..h {
                m.insert(key(q, r), crate::map::tile::TerrainType::Plains);
            }
        }
        m
    }

    fn make_empty_territory(w: i32, h: i32) -> crate::map::territory::TerritoryManager {
        let mut mgr = crate::map::territory::TerritoryManager::new((w * h) as usize);
        for r in 0..h {
            for q in 0..w {
                mgr.register_tile(HexCoord::new(q, r));
            }
        }
        mgr
    }

    #[test]
    fn test_ai_expansion_target_finds_neighbor() {
        let fid = "faction_2".to_string();
        let main = HexCoord::new(10, 10);
        let mut territory = make_empty_territory(128, 128);
        territory.set_main_city(&fid, main);
        territory.occupy(main, &fid);
        let terrain = make_full_plains_terrain(128, 128);

        // 邻接 1 步有 6 个空地，任意一个都行
        let target = ai_expansion_target(&fid, main, &mut territory, &terrain);
        assert!(target.is_some(), "玩家主城旁应该有可占领格");
        let t = target.unwrap();
        // target 必须是主城的 6 邻之一
        let neighbors: Vec<HexCoord> = main.neighbors().to_vec();
        assert!(neighbors.contains(&t), "target 必须是主城 6 邻接之一");
    }

    #[test]
    fn test_ai_expansion_target_no_target_when_all_own() {
        // AI 主城 + 6 邻全部 own 自己 → 无可扩张目标
        // (M0 can_occupy 允许攻占敌方格, 所以用 "6 邻都是自己" 来测 None)
        let fid = "faction_2".to_string();
        let main = HexCoord::new(10, 10);
        let mut territory = make_empty_territory(128, 128);
        territory.set_main_city(&fid, main);
        territory.occupy(main, &fid);
        for n in main.neighbors() {
            territory.occupy(n, &fid);
        }

        let terrain = make_full_plains_terrain(128, 128);
        let target = ai_expansion_target(&fid, main, &mut territory, &terrain);
        assert!(target.is_none(), "主城 + 6 邻都 own 时 AI 应无目标");
    }

    #[test]
    fn test_ai_expansion_target_no_overwrite_own_territory() {
        // AI target 不应覆盖已 own 的格（即使是 AI 自己的）
        let fid = "faction_3".to_string();
        let main = HexCoord::new(50, 50);
        let mut territory = make_empty_territory(128, 128);
        territory.set_main_city(&fid, main);
        territory.occupy(main, &fid);
        // 占一个邻接
        let owned_neighbor = main.neighbors()[0];
        territory.occupy(owned_neighbor, &fid);
        let terrain = make_full_plains_terrain(128, 128);

        let target = ai_expansion_target(&fid, main, &mut territory, &terrain);
        assert!(target.is_some());
        let t = target.unwrap();
        // target 不应是已 own 的格
        assert_ne!(t, owned_neighbor, "AI 不应攻占自己的格");
    }
}
