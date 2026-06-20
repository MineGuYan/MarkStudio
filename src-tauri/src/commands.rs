//! Tauri IPC 命令模块
//!
//! 本模块定义了前端通过 Tauri IPC 可以调用的 Rust 后端命令。
//! 所有命令函数均使用 #[tauri::command] 属性宏标注，
//! 以便 Tauri 框架自动生成对应的 IPC 绑定。

use crate::parser;

/// 解析 Markdown 文本为 HTML
///
/// 此命令通过 Tauri IPC 暴露给前端，前端调用后，
/// 后端使用 pulldown-cmark 将 Markdown 文本转换为 HTML 并返回。
///
/// # 参数
/// - `markdown`: 前端传入的 Markdown 源文本
///
/// # 返回
/// 解析后生成的 HTML 字符串
#[tauri::command]
pub fn parse_markdown(markdown: String) -> String {
    parser::parse_markdown_to_html(&markdown)
}

/// 读取文件内容
///
/// 接收文件路径，读取文件内容并以字符串形式返回。
/// 使用标准库的 std::fs::read_to_string 读取文件。
///
/// # 参数
/// - `path`: 文件路径字符串
///
/// # 返回
/// `Result<String, String>` - 成功时返回文件内容，失败时返回错误描述
#[tauri::command]
pub fn read_file(path: String) -> Result<String, String> {
    std::fs::read_to_string(&path).map_err(|e| format!("读取文件失败: {}", e))
}

/// 写入内容到文件
///
/// 接收文件路径和内容，将内容写入指定文件。
/// 使用标准库的 std::fs::write 写入文件。
///
/// # 参数
/// - `path`: 目标文件路径字符串
/// - `content`: 要写入的文件内容字符串
///
/// # 返回
/// `Result<(), String>` - 成功时返回 Ok(())，失败时返回错误描述
#[tauri::command]
pub fn write_file(path: String, content: String) -> Result<(), String> {
    std::fs::write(&path, &content).map_err(|e| format!("写入文件失败: {}", e))
}

/// 提取 Markdown 文本的大纲（标题层级结构）
///
/// 解析传入的 Markdown 文本，提取所有标题（H1-H6），
/// 返回包含标题级别、文本内容和行号的大纲条目列表。
///
/// # 参数
/// - `markdown`: 前端传入的 Markdown 源文本
///
/// # 返回
/// 按文档出现顺序排列的大纲条目列表
#[tauri::command]
pub fn extract_outline(markdown: String) -> Vec<parser::OutlineItem> {
    parser::extract_outline(&markdown)
}

// ==================== 数据库操作命令 ====================

/// 保存用户设置到 SQLite 数据库
///
/// 使用 key-value 形式存储用户偏好设置（如主题、窗口大小等）。
/// 若 key 已存在则更新值，不存在则插入新记录。
///
/// # 参数
/// - `key`: 设置项名称
/// - `value`: 设置项的值
///
/// # 返回
/// 成功返回 Ok(())，失败返回错误描述字符串
#[tauri::command]
pub fn save_setting(key: String, value: String) -> Result<(), String> {
    crate::database::set_setting(&key, &value)
}

/// 读取用户设置
///
/// 根据 key 从 SQLite 数据库中查询对应的设置值。
///
/// # 参数
/// - `key`: 要查询的设置项名称
///
/// # 返回
/// - Ok(Some(value))：找到对应的设置值
/// - Ok(None)：未找到该设置项
/// - Err(msg)：数据库操作失败
#[tauri::command]
pub fn get_setting(key: String) -> Result<Option<String>, String> {
    crate::database::get_setting(&key)
}

/// 添加最近打开文件记录
///
/// 将文件路径记录到数据库中，若路径已存在则更新其打开时间。
///
/// # 参数
/// - `path`: 文件的完整路径
///
/// # 返回
/// 成功返回 Ok(())，失败返回错误描述字符串
#[tauri::command]
pub fn add_recent_file(path: String) -> Result<(), String> {
    crate::database::add_recent_file(&path)
}

/// 获取最近打开文件列表
///
/// 从数据库中查询最近打开的文件路径，按打开时间降序排列，
/// 最多返回 10 条记录。
///
/// # 返回
/// 最近打开的文件路径列表（字符串向量）
#[tauri::command]
pub fn get_recent_files() -> Result<Vec<String>, String> {
    crate::database::get_recent_files()
}