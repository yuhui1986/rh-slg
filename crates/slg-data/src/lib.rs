//! slg-data: 《天下策》共享数据结构
//!
//! 定义所有共享类型与 ID 结构，仅依赖 serde。
//! 零引擎依赖——不出现任何 Bevy 类型。

pub mod config;
pub mod ids;
pub mod map_doc;
pub mod save;
