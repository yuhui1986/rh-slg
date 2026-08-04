//! 六边形坐标系数学（axial + cube）
//!
//! 坐标系：axial (q, r)，pointy-top，6 邻域
//! 参照 Red Blob Games hex grid guide

use serde::{Deserialize, Serialize};

/// axial 坐标（pointy-top）
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct HexCoord {
    pub q: i32,
    pub r: i32,
}

impl HexCoord {
    /// 创建新的 hex 坐标
    pub fn new(q: i32, r: i32) -> Self {
        Self { q, r }
    }

    /// axial → cube 转换
    pub fn to_cube(self) -> (i32, i32, i32) {
        let x = self.q;
        let z = self.r;
        let y = -x - z;
        (x, y, z)
    }

    /// cube → axial 转换
    pub fn from_cube(x: i32, _y: i32, z: i32) -> Self {
        Self { q: x, r: z }
    }

    /// cube 距离（六边形格数）
    pub fn distance(self, other: Self) -> i32 {
        let (ax, ay, az) = self.to_cube();
        let (bx, by, bz) = other.to_cube();
        ((ax - bx).abs() + (ay - by).abs() + (az - bz).abs()) / 2
    }

    /// 6 邻域（pointy-top 方向）
    pub fn neighbors(self) -> [Self; 6] {
        // pointy-top 方向常量：东、东南、西南、西、西北、东北
        const DIRS: [(i32, i32); 6] = [
            (1, 0),  // 东
            (0, 1),  // 东南
            (-1, 1), // 西南
            (-1, 0), // 西
            (0, -1), // 西北
            (1, -1), // 东北
        ];
        DIRS.map(|(dq, dr)| HexCoord::new(self.q + dq, self.r + dr))
    }

    /// cube ring：距离恰好为 radius 的所有坐标
    pub fn ring(self, radius: i32) -> Vec<Self> {
        if radius == 0 {
            return vec![self];
        }
        let mut results = Vec::with_capacity(6 * radius as usize);
        // 从东南方向开始，沿 6 条边走
        let directions: [(i32, i32, i32); 6] = [
            (1, -1, 0), // 右
            (0, -1, 1), // 右下
            (-1, 0, 1), // 左下
            (-1, 1, 0), // 左
            (0, 1, -1), // 左上
            (1, 0, -1), // 右上
        ];
        let (sx, sy, sz) = self.to_cube();
        // 从一个顶点开始（右上方向偏移 radius 步）
        let mut cx = sx + directions[4].0 * radius;
        let mut cy = sy + directions[4].1 * radius;
        let mut cz = sz + directions[4].2 * radius;
        for dir in &directions {
            for _ in 0..radius {
                results.push(HexCoord::from_cube(cx, cy, cz));
                cx += dir.0;
                cy += dir.1;
                cz += dir.2;
            }
        }
        results
    }

    /// cube 视线（line of sight）：从 a 到 b 的近似直线上的坐标
    pub fn line(a: Self, b: Self) -> Vec<Self> {
        let n = a.distance(b);
        if n == 0 {
            return vec![a];
        }
        let (ax, ay, az) = a.to_cube();
        let (bx, by, bz) = b.to_cube();
        let mut results = Vec::with_capacity(n as usize + 1);
        for i in 0..=n {
            let t = i as f64 / n as f64;
            let px = ax as f64 + (bx - ax) as f64 * t;
            let py = ay as f64 + (by - ay) as f64 * t;
            let pz = az as f64 + (bz - az) as f64 * t;
            results.push(HexCoord::round(px, py, pz));
        }
        // 去除连续重复
        results.dedup();
        results
    }

    /// hex rounding：浮点 cube 坐标取最近六边形中心
    pub fn round(fx: f64, fy: f64, fz: f64) -> Self {
        let mut rx = fx.round();
        let mut ry = fy.round();
        let mut rz = fz.round();

        let x_diff = (rx - fx).abs();
        let y_diff = (ry - fy).abs();
        let z_diff = (rz - fz).abs();

        if x_diff > y_diff && x_diff > z_diff {
            rx = -ry - rz;
        } else if y_diff > z_diff {
            ry = -rx - rz;
        } else {
            rz = -rx - ry;
        }

        Self::from_cube(rx as i32, ry as i32, rz as i32)
    }

    /// 编码为 TileKey（u64）
    pub fn to_tile_key(self) -> u64 {
        ((self.r as u64) << 32) | (self.q as u64 & 0xFFFF_FFFF)
    }

    /// 从 TileKey 解码
    pub fn from_tile_key(key: u64) -> Self {
        let q = (key & 0xFFFF_FFFF) as i32;
        let r = (key >> 32) as i32;
        Self { q, r }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_distance_self() {
        let a = HexCoord::new(0, 0);
        assert_eq!(a.distance(a), 0);
    }

    #[test]
    fn test_distance_symmetry() {
        let a = HexCoord::new(1, 2);
        let b = HexCoord::new(3, -1);
        assert_eq!(a.distance(b), b.distance(a));
    }

    #[test]
    fn test_distance_triangle_inequality() {
        let a = HexCoord::new(0, 0);
        let b = HexCoord::new(2, 1);
        let c = HexCoord::new(-1, 3);
        assert!(a.distance(c) <= a.distance(b) + b.distance(c));
    }

    #[test]
    fn test_neighbors_count() {
        let center = HexCoord::new(0, 0);
        let neighbors = center.neighbors();
        assert_eq!(neighbors.len(), 6);
        // 所有邻居互不相同
        for i in 0..6 {
            for j in (i + 1)..6 {
                assert_ne!(neighbors[i], neighbors[j]);
            }
        }
    }

    #[test]
    fn test_neighbors_distance() {
        let center = HexCoord::new(0, 0);
        for n in center.neighbors() {
            assert_eq!(center.distance(n), 1);
        }
    }

    #[test]
    fn test_ring_1() {
        let center = HexCoord::new(0, 0);
        let ring = center.ring(1);
        assert_eq!(ring.len(), 6);
        for coord in &ring {
            assert_eq!(center.distance(*coord), 1);
        }
    }

    #[test]
    fn test_ring_2() {
        let center = HexCoord::new(0, 0);
        let ring = center.ring(2);
        assert_eq!(ring.len(), 12);
        for coord in &ring {
            assert_eq!(center.distance(*coord), 2);
        }
    }

    #[test]
    fn test_round_identity() {
        let coord = HexCoord::new(3, -2);
        let (x, y, z) = coord.to_cube();
        let rounded = HexCoord::round(x as f64, y as f64, z as f64);
        assert_eq!(rounded, coord);
    }

    #[test]
    fn test_line_symmetry() {
        let a = HexCoord::new(0, 0);
        let b = HexCoord::new(3, -1);
        let line_ab = HexCoord::line(a, b);
        let line_ba = HexCoord::line(b, a);
        // 两端点相同
        assert_eq!(line_ab.first(), line_ba.last());
        assert_eq!(line_ab.last(), line_ba.first());
    }

    #[test]
    fn test_line_endpoints() {
        let a = HexCoord::new(0, 0);
        let b = HexCoord::new(5, -2);
        let line = HexCoord::line(a, b);
        assert_eq!(line.first(), Some(&a));
        assert_eq!(line.last(), Some(&b));
    }

    #[test]
    fn test_to_from_tile_key() {
        let coord = HexCoord::new(100, -200);
        let key = coord.to_tile_key();
        let back = HexCoord::from_tile_key(key);
        assert_eq!(coord, back);
    }

    #[test]
    fn test_to_from_cube() {
        let coord = HexCoord::new(3, -5);
        let (x, y, z) = coord.to_cube();
        assert_eq!(x + y + z, 0); // cube 坐标约束
        let back = HexCoord::from_cube(x, y, z);
        assert_eq!(coord, back);
    }
}
