//! AI 人格预设常量
//!
//! FactionPersonality 结构体已在 entity::faction 中定义，此处提供常见预设。

use crate::entity::faction::FactionPersonality;

/// 好战型人格
pub const AGGRESSIVE: FactionPersonality = FactionPersonality {
    aggression: 0.9,
    expansion: 0.7,
    diplomacy: 0.2,
    caution: 0.3,
};

/// 外交型人格
pub const DIPLOMATIC: FactionPersonality = FactionPersonality {
    aggression: 0.3,
    expansion: 0.5,
    diplomacy: 0.9,
    caution: 0.6,
};

/// 保守型人格
pub const CAUTIOUS: FactionPersonality = FactionPersonality {
    aggression: 0.2,
    expansion: 0.3,
    diplomacy: 0.5,
    caution: 0.9,
};

/// 扩张型人格
pub const EXPANSIONIST: FactionPersonality = FactionPersonality {
    aggression: 0.6,
    expansion: 0.9,
    diplomacy: 0.4,
    caution: 0.4,
};

// ---------------------------------------------------------------------------
// 势力专属人格预设
// ---------------------------------------------------------------------------

/// 魏 - 扩张好战
pub const WEI: FactionPersonality = FactionPersonality {
    aggression: 0.8,
    expansion: 0.9,
    diplomacy: 0.4,
    caution: 0.5,
};

/// 蜀 - 外交温和
pub const SHU: FactionPersonality = FactionPersonality {
    aggression: 0.4,
    expansion: 0.6,
    diplomacy: 0.8,
    caution: 0.6,
};

/// 吴 - 防御稳健
pub const WU: FactionPersonality = FactionPersonality {
    aggression: 0.3,
    expansion: 0.5,
    diplomacy: 0.6,
    caution: 0.8,
};

/// 辽东 - 投机冒险
pub const LIAODONG: FactionPersonality = FactionPersonality {
    aggression: 0.7,
    expansion: 0.7,
    diplomacy: 0.3,
    caution: 0.3,
};

/// 南中 - 保守自守
pub const NANZHONG: FactionPersonality = FactionPersonality {
    aggression: 0.2,
    expansion: 0.3,
    diplomacy: 0.5,
    caution: 0.9,
};
