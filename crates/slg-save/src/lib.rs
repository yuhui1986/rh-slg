//! slg-save: 《天下策》存档与地图容器格式
//!
//! 负责 .slgmap/.slgsave 二进制容器读写、bincode+zstd 分节压缩、版本迁移链。

pub mod container;
pub mod migration;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// 容器格式魔数
pub const MAGIC: &[u8; 4] = b"SLGM";

/// 容器格式版本号
pub const VERSION: u32 = 1;

/// 容器文件头
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveHeader {
    pub magic: [u8; 4],
    pub version: u32,
    pub toc_offset: u64,
}

/// TOC 条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TocEntry {
    pub section_type: SectionType,
    pub offset: u64,
    pub size: u64,
    pub crc32: u32,
}

/// 节类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SectionType {
    Meta,
    TerrainLayer,
    ResourceLayer,
    EntityLayer,
    RuleLayer,
    PreviewPng,
    // Save-specific sections
    SaveMeta,
    FactionStates,
    EntitySnapshots,
    TileDeltas,
    EventLog,
}

/// 容器 TOC
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableOfContents {
    pub entries: Vec<TocEntry>,
}

/// 存档/地图容器错误类型
#[derive(Error, Debug)]
pub enum SaveError {
    #[error("无效的容器魔数: expected SLGM, got {0:?}")]
    InvalidMagic([u8; 4]),
    #[error("不支持的容器版本: {0}")]
    UnsupportedVersion(u32),
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),
    #[error("序列化错误: {0}")]
    Serialization(String),
    #[error("压缩错误: {0}")]
    Compression(String),
    #[error("解压错误: {0}")]
    Decompression(String),
    #[error("CRC32 校验失败: section {section_type:?}, expected {expected:#x}, got {actual:#x}")]
    CrcMismatch {
        section_type: SectionType,
        expected: u32,
        actual: u32,
    },
    #[error("节未找到: {0:?}")]
    SectionNotFound(SectionType),
    #[error("地图文件验证失败: {0}")]
    ValidationFailed(String),
}
