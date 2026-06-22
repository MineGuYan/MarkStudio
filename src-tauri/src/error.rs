//! 统一错误处理模块
//!
//! 本模块定义了应用级别的统一错误类型 `AppError`，
//! 用于替代各模块中散落的 `String` 错误返回，
//! 提供结构化的错误信息和更好的错误处理体验。

use std::fmt;

/// 应用统一错误类型
///
/// 包含各类业务错误变体，每个变体携带描述性的错误消息。
/// 实现了 `Display`、`std::error::Error` 和 `From<AppError> for String`，
/// 可无缝集成到 Tauri 命令的错误返回中。
#[derive(Debug)]
#[allow(clippy::enum_variant_names)]
pub enum AppError {
    /// 文件操作相关错误（读取、写入、权限等）
    FileError(String),
    /// 数据库操作相关错误
    DatabaseError(String),
    /// Markdown 解析相关错误
    ParseError(String),
    /// 图片处理相关错误
    ImageError(String),
    /// OT 操作相关错误
    OtError(String),
    /// 协作会话相关错误
    CollaborationError(String),
    /// 设置管理相关错误
    SettingsError(String),
    /// 通用业务逻辑错误
    BusinessError(String),
    /// 输入验证错误
    ValidationError(String),
    /// 收藏夹相关错误
    FavoriteError(String),
    /// 标签页管理相关错误
    TabError(String),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::FileError(msg) => write!(f, "文件操作错误: {}", msg),
            AppError::DatabaseError(msg) => write!(f, "数据库错误: {}", msg),
            AppError::ParseError(msg) => write!(f, "解析错误: {}", msg),
            AppError::ImageError(msg) => write!(f, "图片处理错误: {}", msg),
            AppError::OtError(msg) => write!(f, "OT 操作错误: {}", msg),
            AppError::CollaborationError(msg) => write!(f, "协作错误: {}", msg),
            AppError::SettingsError(msg) => write!(f, "设置错误: {}", msg),
            AppError::BusinessError(msg) => write!(f, "业务错误: {}", msg),
            AppError::ValidationError(msg) => write!(f, "验证错误: {}", msg),
            AppError::FavoriteError(msg) => write!(f, "收藏夹错误: {}", msg),
            AppError::TabError(msg) => write!(f, "标签页错误: {}", msg),
        }
    }
}

impl std::error::Error for AppError {}

/// 从 `AppError` 转换为 `String`，用于 Tauri 命令的错误返回
impl From<AppError> for String {
    fn from(error: AppError) -> Self {
        error.to_string()
    }
}

/// 从 `rusqlite::Error` 转换为 `AppError`
impl From<rusqlite::Error> for AppError {
    fn from(error: rusqlite::Error) -> Self {
        AppError::DatabaseError(error.to_string())
    }
}

/// 从 `std::io::Error` 转换为 `AppError`
impl From<std::io::Error> for AppError {
    fn from(error: std::io::Error) -> Self {
        AppError::FileError(error.to_string())
    }
}

/// 从 `serde_json::Error` 转换为 `AppError`
impl From<serde_json::Error> for AppError {
    fn from(error: serde_json::Error) -> Self {
        AppError::BusinessError(format!("JSON 序列化/反序列化错误: {}", error))
    }
}

/// 便捷类型别名：`Result<T, AppError>`
#[allow(dead_code)]
pub type AppResult<T> = Result<T, AppError>;
