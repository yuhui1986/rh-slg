//! 容器格式版本迁移链
//!
//! 当容器格式版本升级时，在此添加逐版本迁移函数。
//! 迁移链：v1 -> v2 -> v3 -> ... -> 最新版

use crate::SaveError;

/// 将容器数据从指定版本迁移到当前最新版本。
///
/// 迁移是就地修改 `data` 字节缓冲区。
/// 每个版本的迁移函数负责更新版本号标记。
pub fn migrate(_data: &mut Vec<u8>, from_version: u32) -> Result<(), SaveError> {
    match from_version {
        // 当前版本，无需迁移
        1 => Ok(()),
        _ => Err(SaveError::UnsupportedVersion(from_version)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_migrate_current_version() {
        // 版本 1 是当前版本，应直接成功
        let mut data = Vec::new();
        assert!(migrate(&mut data, 1).is_ok());
    }

    #[test]
    fn test_migrate_unsupported_version() {
        let mut data = Vec::new();
        assert!(matches!(
            migrate(&mut data, 0),
            Err(SaveError::UnsupportedVersion(0))
        ));
        assert!(matches!(
            migrate(&mut data, 99),
            Err(SaveError::UnsupportedVersion(99))
        ));
    }
}
