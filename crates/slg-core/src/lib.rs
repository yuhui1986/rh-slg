//! slg-core: 《天下策》纯逻辑核心层
//!
//! 零引擎依赖——不出现任何 Bevy 类型。
//! 包含 hex 网格、战斗模拟、经济、领地、AI、程序化生成等游戏核心逻辑。

pub mod ai;
pub mod clock;
pub mod entity;
pub mod event;
pub mod fog;
pub mod gen;
pub mod map;
pub mod military;
pub mod resource;
pub mod rule;
pub mod save_manager;
pub mod scenario_loader;
pub mod statistics;
