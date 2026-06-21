//! 文件操作服务模块
//!
//! 本模块提供文件读写、编码检测和最近文件管理的核心业务逻辑。
//! 不依赖 Tauri 框架，提供纯函数式接口，便于单元测试和复用。

/// 读取文件内容，自动检测编码
///
/// 接收文件路径，读取文件内容并以字符串形式返回。
/// 支持多种编码自动检测：
/// 1. 首先尝试 UTF-8 编码读取
/// 2. 若 UTF-8 失败，检测 BOM（字节顺序标记）识别编码
/// 3. 若无 BOM，尝试使用 GBK 编码解码（中文 Windows 常见编码）
/// 4. 若仍失败，尝试使用 GB18030 编码（GBK 的超集）
///
/// # 参数
/// - `path`: 文件路径字符串
///
/// # 返回
/// `Result<String, String>` - 成功时返回文件内容，失败时返回错误描述
pub fn read_file_content(path: &str) -> Result<String, String> {
    // 先以原始字节形式读取文件，避免编码问题导致读取失败
    let bytes = std::fs::read(path).map_err(|e| format!("读取文件失败: {}", e))?;

    // 策略 1：尝试按 UTF-8 解码
    if let Ok(content) = String::from_utf8(bytes.clone()) {
        return Ok(content);
    }

    // 策略 2：检测 BOM（字节顺序标记）来判断编码
    if let Some((encoding, bom_length)) = encoding_rs::Encoding::for_bom(&bytes) {
        // 跳过 BOM 字节，使用检测到的编码解码
        let (decoded, _, had_errors) = encoding.decode(&bytes[bom_length..]);
        if !had_errors {
            return Ok(decoded.into_owned());
        }
    }

    // 策略 3：尝试使用 GBK 编码（覆盖 GB2312，中文 Windows 常见编码）
    let gbk = encoding_rs::Encoding::for_label(b"gbk").unwrap();
    let (decoded, _, had_errors) = gbk.decode(&bytes);
    if !had_errors {
        return Ok(decoded.into_owned());
    }

    // 策略 4：尝试使用 GB18030 编码（GBK 的超集，支持更多字符）
    let gb18030 = encoding_rs::Encoding::for_label(b"gb18030").unwrap();
    let (decoded, _, had_errors) = gb18030.decode(&bytes);
    if !had_errors {
        return Ok(decoded.into_owned());
    }

    // 所有编码尝试均失败，返回错误信息
    Err("读取文件失败: 文件编码无法识别，尝试了 UTF-8、GBK、GB18030 均失败".to_string())
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
pub fn write_file_content(path: &str, content: &str) -> Result<(), String> {
    std::fs::write(path, content).map_err(|e| format!("写入文件失败: {}", e))
}

/// 添加最近打开文件记录
///
/// 将文件路径记录到数据库中，若路径已存在则更新其打开时间。
/// 委托给 database 模块的 add_recent_file 函数。
///
/// # 参数
/// - `path`: 文件的完整路径
///
/// # 返回
/// 成功返回 Ok(())，失败返回错误描述字符串
pub fn add_recent_file_record(path: &str) -> Result<(), String> {
    crate::database::add_recent_file(path)
}

/// 获取最近打开文件列表
///
/// 从数据库中查询最近打开的文件路径，按打开时间降序排列，
/// 最多返回 10 条记录。
/// 委托给 database 模块的 get_recent_files 函数。
///
/// # 返回
/// 最近打开的文件路径列表（字符串向量）
pub fn get_recent_file_list() -> Result<Vec<String>, String> {
    crate::database::get_recent_files()
}

// ==================== 单元测试 ====================

#[cfg(test)]
mod tests {
    use super::*;
    use std::env::temp_dir;
    use std::fs;

    /// 辅助函数：在临时目录中创建唯一的测试文件路径
    fn temp_test_path(name: &str) -> String {
        temp_dir()
            .join(format!("markstudio_test_{}", name))
            .to_string_lossy()
            .to_string()
    }

    /// 测试写入文件后读取回来，验证内容一致
    #[test]
    fn test_write_and_read() {
        let path = temp_test_path("write_and_read.txt");
        let content = "Hello, MarkStudio!\n这是一个测试文件。";

        // 写入文件
        write_file_content(&path, content).expect("写入文件应该成功");

        // 读取文件并验证内容
        let result = read_file_content(&path).expect("读取文件应该成功");
        assert_eq!(result, content, "读取的内容应与写入的内容一致");

        // 清理临时文件
        let _ = fs::remove_file(&path);
    }

    /// 测试写入到无效路径（空字符串），应返回错误
    #[test]
    fn test_write_file_error() {
        // 空字符串路径在 Windows 上会导致写入失败
        let result = write_file_content("", "test content");
        assert!(result.is_err(), "写入空路径应该失败");
    }

    /// 测试读取不存在的文件，应返回错误
    #[test]
    fn test_read_file_not_found() {
        let path = temp_test_path("non_existent_file.txt");

        // 确保文件不存在
        let _ = fs::remove_file(&path);

        let result = read_file_content(&path);
        assert!(result.is_err(), "读取不存在的文件应该返回错误");
    }

    /// 测试读取包含中文字符的 UTF-8 文件
    #[test]
    fn test_read_file_utf8() {
        let path = temp_test_path("utf8_chinese.txt");
        let content = "这是 UTF-8 编码的文件。\n包含中文标点符号：，。！？\n还有英文混合：Hello 世界！\n以及特殊字符：①②③★☆";

        // 写入 UTF-8 文件
        fs::write(&path, content).expect("写入 UTF-8 测试文件应该成功");

        // 读取并验证
        let result = read_file_content(&path).expect("读取 UTF-8 文件应该成功");
        assert_eq!(result, content, "读取的 UTF-8 中文内容应与写入的一致");

        // 清理临时文件
        let _ = fs::remove_file(&path);
    }
}
