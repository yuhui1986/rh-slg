//! 迷雾系统：per-chunk fog 状态 + 揭开规则
//!
//! 率土地图核心体验：**派兵 = 探索 = 揭开未知**。
//! 玩家主城 + 6 邻接永久揭开；行军路径经过 = 揭开；行军到达 = 永久揭开邻接。
//!
//! MVP 简化：fog 只有二态（0 = 黑雾 / 1 = 揭开），不做"已探索但不可见"的灰阶。
//! 后续可加 `FogState::{Unseen, Explored, Visible}` 三态。
//!
//! 数据布局：按 chunk 存 1024 u8，对应 32x32 hex。
//! `(cx, cy)` 决定哪个 chunk；`ly * 32 + lx` 决定 chunk 内 idx。
//! 全图坐标 `(q, r)`: `cx = q / 32, lx = q % 32`；`cy = r / 32, ly = r % 32`。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use slg_data::ids::FactionId;

use crate::map::grid::HexCoord;

// ---------------------------------------------------------------------------
// 常量
// ---------------------------------------------------------------------------

/// fog 状态：黑雾（看不到）
pub const FOG_FOGGED: u8 = 0;
/// fog 状态：揭开（看到地形 + 势力色）
pub const FOG_VISIBLE: u8 = 1;

/// 每 chunk 边长（32x32 = 1024 格）
pub const CHUNK_SIZE: u32 = 32;
/// 每 chunk 总格数
pub const CHUNK_TILES: usize = (CHUNK_SIZE * CHUNK_SIZE) as usize;

// ---------------------------------------------------------------------------
// 单 chunk fog
// ---------------------------------------------------------------------------

/// 单个 fog chunk（32x32 = 1024 格）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FogChunk {
    /// 1024 u8：`FOG_FOGGED` 或 `FOG_VISIBLE`
    pub data: Vec<u8>,
}

impl FogChunk {
    /// 创建全黑（未探索）的迷雾 chunk
    pub fn new_fogged() -> Self {
        Self {
            data: vec![FOG_FOGGED; CHUNK_TILES],
        }
    }

    /// 创建全亮（已探索）的迷雾 chunk
    pub fn new_visible() -> Self {
        Self {
            data: vec![FOG_VISIBLE; CHUNK_TILES],
        }
    }

    /// 把 (cx, cy) 处的 chunk 在全局 (q, r) 对应 idx 揭开
    ///
    /// 越界检查：q, r 必须落在 chunk 内（cx*32 <= q < (cx+1)*32）
    pub fn reveal(&mut self, cx: u32, cy: u32, q: i32, r: i32) -> bool {
        let lx = q - (cx as i32) * (CHUNK_SIZE as i32);
        let ly = r - (cy as i32) * (CHUNK_SIZE as i32);
        if lx < 0 || ly < 0 || lx >= CHUNK_SIZE as i32 || ly >= CHUNK_SIZE as i32 {
            return false;
        }
        let idx = (ly as u32 * CHUNK_SIZE + lx as u32) as usize;
        if idx < self.data.len() {
            self.data[idx] = FOG_VISIBLE;
            true
        } else {
            false
        }
    }

    /// 读 (cx, cy) chunk 内 (q, r) 的 fog 状态
    pub fn get(&self, q: i32, r: i32, cx: u32, cy: u32) -> u8 {
        let lx = q - (cx as i32) * (CHUNK_SIZE as i32);
        let ly = r - (cy as i32) * (CHUNK_SIZE as i32);
        if lx < 0 || ly < 0 || lx >= CHUNK_SIZE as i32 || ly >= CHUNK_SIZE as i32 {
            return FOG_FOGGED;
        }
        let idx = (ly as u32 * CHUNK_SIZE + lx as u32) as usize;
        self.data.get(idx).copied().unwrap_or(FOG_FOGGED)
    }

    /// 全部格是否都揭开（无雾）
    pub fn all_visible(&self) -> bool {
        self.data.iter().all(|&v| v == FOG_VISIBLE)
    }

    /// 全部格是否都黑雾
    pub fn all_fogged(&self) -> bool {
        self.data.iter().all(|&v| v == FOG_FOGGED)
    }
}

// ---------------------------------------------------------------------------
// 全局迷雾
// ---------------------------------------------------------------------------

/// 全图迷雾：按 (chunk_x, chunk_y) 索引 FogChunk
///
/// chunk 坐标 (cx, cy) → FogChunk。
/// 用 BTreeMap 而不是 Vec 索引，因为部分地图（如 130x100）chunks_x/chunks_y
/// 不一定整除；用 map 存存在的 chunk 更灵活。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FogOfWar {
    pub chunks: BTreeMap<(u32, u32), FogChunk>,
}

impl FogOfWar {
    pub fn new() -> Self {
        Self::default()
    }

    /// 根据地图尺寸 + 主城列表初始化
    ///
    /// 全部 chunk 初始全黑（未探索），主城及其 6 邻域揭开。
    pub fn init_with_cities(
        map_w: u32,
        map_h: u32,
        cities: &[(HexCoord, FactionId)],
        player_faction: &FactionId,
    ) -> Self {
        let chunks_x = map_w.div_ceil(CHUNK_SIZE);
        let chunks_y = map_h.div_ceil(CHUNK_SIZE);
        let mut fog = Self {
            chunks: BTreeMap::new(),
        };

        // 1. 全部 chunk 先建出来（全黑）
        for cy in 0..chunks_y {
            for cx in 0..chunks_x {
                fog.chunks.insert((cx, cy), FogChunk::new_fogged());
            }
        }

        // 2. 玩家主城 + 6 邻域揭开
        for (city, fid) in cities {
            if fid == player_faction {
                fog.reveal_around(*city, 1);
            }
        }

        fog
    }

    /// 揭开 (q, r) 周围 radius 圈的所有格
    ///
    /// radius=1: 自身 + 6 邻接
    /// radius=2: 自身 + 6 邻接 + 外圈 12 格
    pub fn reveal_around(&mut self, center: HexCoord, radius: i32) {
        // 收集要揭开的格
        let mut to_reveal = vec![center];
        for r in 1..=radius {
            to_reveal.extend(center.ring(r));
        }

        for coord in to_reveal {
            self.reveal_one(coord);
        }
    }

    /// 揭开一行军路径（所有中间格 + 起点 + 终点）
    pub fn reveal_path(&mut self, path: &[HexCoord]) {
        for coord in path {
            self.reveal_one(*coord);
        }
    }

    /// 揭开单格
    pub fn reveal_one(&mut self, coord: HexCoord) {
        if coord.q < 0 || coord.r < 0 {
            return;
        }
        let q = coord.q as u32;
        let r = coord.r as u32;
        let cx = q / CHUNK_SIZE;
        let cy = r / CHUNK_SIZE;
        let entry = self
            .chunks
            .entry((cx, cy))
            .or_insert_with(FogChunk::new_fogged);
        entry.reveal(cx, cy, coord.q, coord.r);
    }

    /// 读 (q, r) 的 fog 状态
    pub fn get(&self, q: i32, r: i32) -> u8 {
        if q < 0 || r < 0 {
            return FOG_FOGGED;
        }
        let cx = (q as u32) / CHUNK_SIZE;
        let cy = (r as u32) / CHUNK_SIZE;
        self.chunks
            .get(&(cx, cy))
            .map(|c| c.get(q, r, cx, cy))
            .unwrap_or(FOG_FOGGED)
    }

    /// 该 chunk 是否在 fog 里有数据
    pub fn chunk(&self, cx: u32, cy: u32) -> Option<&FogChunk> {
        self.chunks.get(&(cx, cy))
    }

    /// 整图是否有任何揭开（debug 用）
    pub fn any_visible(&self) -> bool {
        self.chunks.values().any(|c| !c.all_fogged())
    }
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fog_chunk_new_fogged_all_black() {
        let c = FogChunk::new_fogged();
        assert!(c.all_fogged());
        assert!(!c.all_visible());
        assert_eq!(c.data.len(), 1024);
    }

    #[test]
    fn test_fog_chunk_reveal_one() {
        let mut c = FogChunk::new_fogged();
        // chunk(0, 0), reveal (5, 3)
        assert!(c.reveal(0, 0, 5, 3));
        assert_eq!(c.get(5, 3, 0, 0), FOG_VISIBLE);
        assert_eq!(c.get(0, 0, 0, 0), FOG_FOGGED);
        assert!(!c.all_fogged());
    }

    #[test]
    fn test_fog_chunk_reveal_out_of_bounds() {
        let mut c = FogChunk::new_fogged();
        // chunk(0, 0) covers q,r in [0, 32) — reveal (40, 0) should fail
        assert!(!c.reveal(0, 0, 40, 0));
        // negative
        assert!(!c.reveal(0, 0, -1, 0));
    }

    #[test]
    fn test_fog_of_war_init_with_cities() {
        // 64x64 地图, 2 城
        let cities = vec![
            (HexCoord::new(10, 10), "faction_1".to_string()),
            (HexCoord::new(50, 50), "faction_2".to_string()),
        ];
        let fog = FogOfWar::init_with_cities(64, 64, &cities, &"faction_1".to_string());
        // 64x64 -> 2x2 = 4 chunks
        assert_eq!(fog.chunks.len(), 4);
        // 玩家主城 (10, 10) 揭开
        assert_eq!(fog.get(10, 10), FOG_VISIBLE);
        // 邻接 (11, 10) 揭开
        assert_eq!(fog.get(11, 10), FOG_VISIBLE);
        // 邻接 (10, 11) 揭开
        assert_eq!(fog.get(10, 11), FOG_VISIBLE);
        // 远处 (60, 60) 仍然黑
        assert_eq!(fog.get(60, 60), FOG_FOGGED);
        // AI 主城 (50, 50) 没揭（不是玩家）
        assert_eq!(fog.get(50, 50), FOG_FOGGED);
    }

    #[test]
    fn test_fog_reveal_around_radius_2() {
        let mut fog = FogOfWar::new();
        fog.reveal_around(HexCoord::new(10, 10), 2);
        // 中心
        assert_eq!(fog.get(10, 10), FOG_VISIBLE);
        // 1 圈
        assert_eq!(fog.get(11, 10), FOG_VISIBLE);
        // 2 圈（东南方 2 步）
        assert_eq!(fog.get(11, 11), FOG_VISIBLE);
        // 3 步之外：黑
        assert_eq!(fog.get(13, 10), FOG_FOGGED);
    }

    #[test]
    fn test_fog_reveal_path() {
        let mut fog = FogOfWar::new();
        // 路径 (0,0) -> (1,0) -> (2,0) -> (3,0)
        fog.reveal_path(&[
            HexCoord::new(0, 0),
            HexCoord::new(1, 0),
            HexCoord::new(2, 0),
            HexCoord::new(3, 0),
        ]);
        assert_eq!(fog.get(0, 0), FOG_VISIBLE);
        assert_eq!(fog.get(1, 0), FOG_VISIBLE);
        assert_eq!(fog.get(2, 0), FOG_VISIBLE);
        assert_eq!(fog.get(3, 0), FOG_VISIBLE);
        // 旁边
        assert_eq!(fog.get(0, 1), FOG_FOGGED);
    }

    #[test]
    fn test_fog_chunk_x_boundary() {
        // chunk(0,0) covers q in [0, 32), chunk(1,0) covers q in [32, 64)
        let mut fog = FogOfWar::new();
        // 强制创建 2 个 chunk
        fog.chunks.insert((0, 0), FogChunk::new_fogged());
        fog.chunks.insert((1, 0), FogChunk::new_fogged());
        // reveal (32, 0) 应该进 chunk(1, 0)
        fog.reveal_one(HexCoord::new(32, 0));
        assert_eq!(fog.get(32, 0), FOG_VISIBLE);
        assert_eq!(fog.chunk(1, 0).unwrap().get(32, 0, 1, 0), FOG_VISIBLE);
    }

    #[test]
    fn test_fog_negative_returns_fogged() {
        let fog = FogOfWar::new();
        assert_eq!(fog.get(-1, 0), FOG_FOGGED);
        assert_eq!(fog.get(0, -1), FOG_FOGGED);
    }
}
