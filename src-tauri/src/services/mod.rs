//! 业务逻辑服务层模块
//!
//! 本模块包含应用的核心业务逻辑实现，按职责划分为独立的子模块。
//! 每个子模块负责一个特定的业务领域，提供纯函数式的服务接口，
//! 不依赖 Tauri 框架，便于单元测试和维护。

/// OT 操作服务：计算和应用 Operational Transformation 操作
pub mod ot_service;

/// 图片处理服务：图片粘贴、保存、Markdown 语法生成
pub mod image_service;

/// 文档状态管理服务：脏状态检测、字符位置计算
pub mod document_service;

/// 设置管理服务：设置项加载、保存、默认值管理
pub mod settings_service;

/// 文件操作服务：文件读写、编码检测、最近文件管理
pub mod file_service;
