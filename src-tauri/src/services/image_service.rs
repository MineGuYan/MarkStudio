//! 图片处理服务模块
//!
//! 本模块提供图片粘贴处理的核心业务逻辑，包括：
//! - 图片文件名的唯一生成
//! - Base64 图片数据的解码与本地保存
//! - Markdown 图片语法的生成与文档内容插入
//! - 协作模式下 OT Insert 操作的 JSON 序列化

use serde::Serialize;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// 图片粘贴处理结果
///
/// 包含粘贴图片后的文档新内容、保存的文件路径，
/// 以及协作模式下需要的 OT 操作 JSON。
#[derive(Debug, Clone, Serialize)]
pub struct PasteResult {
    /// 插入图片 Markdown 语法后的完整文档内容
    pub new_content: String,
    /// 保存的图片文件路径（使用正斜杠规范化的路径，可直接用于 Markdown 引用）
    pub file_path: String,
    /// OT Insert 操作的 JSON 序列化结果
    ///
    /// - 在协作模式下（`collab_enabled = true`）：包含序列化后的 `Operation::Insert` JSON
    /// - 在非协作模式下：为 `None`
    pub operation_json: Option<String>,
}

/// 生成唯一的图片文件名
///
/// 使用当前 Unix 时间戳与基于纳秒哈希的随机后缀组合，
/// 确保在短时间内生成大量图片也不会产生文件名冲突。
///
/// 文件名格式：`paste_{timestamp}_{random6}.{ext}`
///
/// # 参数
/// - `ext`: 图片文件扩展名（不含点号，如 `"png"`、`"jpg"`）
///
/// # 返回
/// 生成的唯一文件名，例如 `paste_1719000000_a3f2b1.png`
#[allow(dead_code)]
pub fn generate_image_filename(ext: &str) -> String {
    // 获取当前系统时间相对于 UNIX 纪元的时间间隔
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let timestamp = now.as_secs();

    // 使用 DefaultHasher 对纳秒值进行哈希，取低 24 位作为 6 位十六进制随机后缀
    let mut hasher = DefaultHasher::new();
    now.as_nanos().hash(&mut hasher);
    let random_part = format!("{:06x}", hasher.finish() & 0xFFFFFF);

    format!("paste_{}_{}.{}", timestamp, random_part, ext)
}

/// 处理粘贴图片操作
///
/// 完整的图片粘贴处理流程：
/// 1. 将 Base64 编码的图片数据解码为二进制字节
/// 2. 确定并创建图片缓存目录
/// 3. 将图片保存到缓存目录
/// 4. 生成 Markdown 图片语法（路径使用正斜杠）
/// 5. 在文档内容的指定光标位置插入图片 Markdown
/// 6. 若启用协作模式，生成对应的 OT Insert 操作 JSON
///
/// 此函数与 `commands.rs` 中的 `save_image_cache` 使用相同的缓存目录逻辑，
/// 确保图片保存在统一的路径下。
///
/// # 参数
/// - `base64_data`: 图片的 Base64 编码数据（不含 `data:` URI 前缀）
/// - `file_name`: 保存的图片文件名（不含路径，仅文件名）
/// - `content`: 当前的文档文本内容
/// - `cursor_pos`: 光标所在的字符位置（从 0 开始计数），在此位置插入图片 Markdown
/// - `cache_dir`: 用户指定的缓存目录路径（为空字符串时使用默认路径 `data/image_cache/`）
/// - `collab_enabled`: 是否启用协作模式，决定是否生成 OT 操作 JSON
///
/// # 返回
/// - `Ok(PasteResult)`: 处理成功，返回包含新文档内容、文件路径和可选操作 JSON 的结果
/// - `Err(String)`: 处理失败，返回人类可读的错误描述
///
/// # 示例
/// ```
/// let result = process_paste_image(
///     "iVBORw0KGgo...",  // Base64 数据
///     "image.png",
///     "# Hello\n\nWorld",
///     8,                   // 在 "Hello" 之后插入
///     "",                  // 使用默认缓存目录
///     false,               // 非协作模式
/// )?;
/// ```
pub fn process_paste_image(
    base64_data: &str,
    file_name: &str,
    content: &str,
    cursor_pos: usize,
    cache_dir: &str,
    collab_enabled: bool,
) -> Result<PasteResult, String> {
    use base64::Engine;

    // 第一步：解码 Base64 数据为二进制字节
    let data = base64::engine::general_purpose::STANDARD
        .decode(base64_data)
        .map_err(|e| format!("Base64 解码失败: {}", e))?;

    // 第二步：确定缓存目录路径
    // 优先使用用户指定的目录，若为空则使用默认的 data/image_cache/
    let cache_path = if cache_dir.trim().is_empty() {
        get_default_cache_dir()
    } else {
        std::path::PathBuf::from(cache_dir)
    };

    // 第三步：确保缓存目录存在（递归创建所有父目录）
    std::fs::create_dir_all(&cache_path).map_err(|e| format!("创建图片缓存目录失败: {}", e))?;

    // 第四步：将图片数据写入缓存目录中的文件
    let file_path = cache_path.join(file_name);
    std::fs::write(&file_path, &data).map_err(|e| format!("保存图片文件失败: {}", e))?;

    // 第五步：生成 Markdown 图片语法
    // 将 Windows 反斜杠路径转换为正斜杠，确保 Markdown 图片路径在跨平台间一致
    let normalized_path = file_path.to_string_lossy().replace('\\', "/");
    let image_markdown = format!("![image]({})", normalized_path);

    // 第六步：在光标位置插入图片 Markdown 语法到文档内容中
    let new_content = insert_at_position(content, cursor_pos, &image_markdown);

    // 第七步：若启用协作模式，生成 OT Insert 操作的 JSON 序列化结果
    let operation_json = if collab_enabled {
        let insert_op = crate::collaboration::ot::Operation::Insert {
            position: cursor_pos,
            text: image_markdown.clone(),
        };
        Some(serde_json::to_string(&insert_op).map_err(|e| format!("序列化 OT 操作失败: {}", e))?)
    } else {
        None
    };

    Ok(PasteResult {
        new_content,
        file_path: normalized_path,
        operation_json,
    })
}

// ============================================================================
// 内部辅助函数
// ============================================================================

/// 在指定字符位置插入文本
///
/// 将 `insert_text` 插入到 `content` 的第 `pos` 个字符之前（`pos` 从 0 开始计数）。
/// 若 `pos` 超出文本总字符数，则追加到文本末尾。
///
/// 此函数正确处理多字节 UTF-8 字符（如中文），基于字符偏移量而非字节偏移量进行分割。
///
/// # 参数
/// - `content`: 原始文本内容
/// - `pos`: 插入位置（字符偏移量，0 表示在第一个字符之前插入）
/// - `insert_text`: 要插入的文本
///
/// # 返回
/// 插入后的新文本
fn insert_at_position(content: &str, pos: usize, insert_text: &str) -> String {
    let char_count = content.chars().count();
    if pos >= char_count {
        // 插入位置在文本末尾或之后，直接追加
        let mut result = String::with_capacity(content.len() + insert_text.len());
        result.push_str(content);
        result.push_str(insert_text);
        result
    } else {
        // 在文本中间插入：将字符偏移量转换为字节偏移量
        // 确保在多字节 UTF-8 字符边界上正确分割
        let byte_pos = content
            .char_indices()
            .nth(pos)
            .map(|(i, _)| i)
            .unwrap_or(content.len());
        let mut result = String::with_capacity(content.len() + insert_text.len());
        result.push_str(&content[..byte_pos]);
        result.push_str(insert_text);
        result.push_str(&content[byte_pos..]);
        result
    }
}

/// 获取默认的图片缓存目录路径
///
/// 返回项目根目录下的 `data/image_cache/` 子目录的绝对路径。
/// 此函数与 `commands.rs` 中的 `get_default_cache_dir` 和
/// `collaboration/sync.rs` 中的 `get_image_cache_dir` 使用相同的逻辑：
///
/// - 在开发模式下（`cargo run`），`current_dir()` 返回 `src-tauri/`，
///   需要回退到项目根目录以确保路径正确
/// - 在打包发布后，直接使用当前工作目录
fn get_default_cache_dir() -> std::path::PathBuf {
    let current_dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));

    // 开发模式下（cargo run），当前工作目录为 src-tauri/
    // 需要回退到项目根目录
    let project_root = if current_dir
        .file_name()
        .map(|n| n == "src-tauri")
        .unwrap_or(false)
    {
        current_dir
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| current_dir.clone())
    } else {
        current_dir
    };

    project_root.join("data").join("image_cache")
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ================================================================
    // generate_image_filename 测试
    // ================================================================

    /// 测试文件名生成：验证格式为 `paste_{timestamp}_{random6}.{ext}`
    #[test]
    fn test_generate_filename() {
        let filename = generate_image_filename("png");

        // 验证文件名以 "paste_" 开头
        assert!(
            filename.starts_with("paste_"),
            "文件名应以 paste_ 开头，实际: {}",
            filename
        );

        // 验证文件名以 ".png" 结尾
        assert!(
            filename.ends_with(".png"),
            "文件名应以 .png 结尾，实际: {}",
            filename
        );

        // 验证文件名格式：paste_{timestamp}_{6位随机hex}.png
        let without_prefix = filename
            .strip_prefix("paste_")
            .expect("文件名应包含 paste_ 前缀");
        let without_ext = without_prefix
            .strip_suffix(".png")
            .expect("文件名应包含 .png 后缀");
        let parts: Vec<&str> = without_ext.split('_').collect();

        assert_eq!(
            parts.len(),
            2,
            "文件名应包含 timestamp 和 random 两部分，用下划线分隔，实际: {:?}",
            parts
        );

        // 时间戳部分应为纯数字
        assert!(
            parts[0].chars().all(|c| c.is_ascii_digit()),
            "时间戳部分应为纯数字，实际: {}",
            parts[0]
        );

        // 随机部分应为 6 位十六进制字符
        assert_eq!(
            parts[1].len(),
            6,
            "随机部分应为 6 位十六进制字符，实际长度: {}",
            parts[1].len()
        );
        assert!(
            parts[1].chars().all(|c| c.is_ascii_hexdigit()),
            "随机部分应全部为十六进制字符，实际: {}",
            parts[1]
        );
    }

    /// 测试同一扩展名生成的文件名不同（验证随机性确保唯一性）
    #[test]
    fn test_generate_filename_unique() {
        let name1 = generate_image_filename("png");
        // 短暂等待以确保纳秒时间戳不同
        std::thread::sleep(std::time::Duration::from_millis(10));
        let name2 = generate_image_filename("png");
        assert_ne!(
            name1, name2,
            "两次连续生成的文件名应不同，实际均为: {}",
            name1
        );
    }

    // ================================================================
    // process_paste_image 测试
    // ================================================================

    /// 测试 process_paste_image：使用最小有效 PNG 进行完整的粘贴处理流程
    ///
    /// 验证点：
    /// - 图片文件被正确保存到磁盘
    /// - 文档内容在光标位置正确插入了图片 Markdown 语法
    /// - 非协作模式下 operation_json 为 None
    #[test]
    fn test_process_paste_image() {
        use base64::Engine;

        // 最小有效 PNG 图片的二进制数据（1x1 像素，RGB 颜色）
        // 包含完整的 PNG 签名、IHDR、IDAT、IEND 数据块
        let png_data: &[u8] = &[
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // PNG 签名
            0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, // IHDR 长度(13) + "IHDR"
            0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, // 宽度 1, 高度 1
            0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77,
            0x53, // 位深 8, 颜色类型 2(RGB), CRC
            0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, // IDAT 长度(12) + "IDAT"
            0x54, 0x08, 0xD7, 0x63, 0xF8, 0xCF, 0xC0, 0x00, // 压缩像素数据
            0x00, 0x00, 0x03, 0x00, 0x01, 0x47, 0x53, 0x75, // 压缩数据续 + CRC
            0x3E, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, // IEND 长度(0) + "IEND"
            0x44, 0xAE, 0x42, 0x60, 0x82, // IEND CRC
        ];

        let base64_data = base64::engine::general_purpose::STANDARD.encode(png_data);
        let file_name = "test_image.png";
        let content = "Hello World";
        let cursor_pos = 6; // 在 "Hello " 之后（第 6 个字符位置）插入

        // 使用临时目录作为缓存目录，避免污染项目目录
        let temp_dir = std::env::temp_dir().join("markstudio_test_image");
        let cache_dir = temp_dir.to_string_lossy().to_string();

        let result = process_paste_image(
            &base64_data,
            file_name,
            content,
            cursor_pos,
            &cache_dir,
            false, // 非协作模式
        )
        .expect("图片粘贴处理应成功");

        // 验证文件已保存到磁盘
        let saved_path = std::path::Path::new(&result.file_path);
        assert!(
            saved_path.exists(),
            "图片文件应被保存到磁盘，路径: {}",
            result.file_path
        );

        // 验证保存的文件内容与原始数据一致
        let saved_data = std::fs::read(saved_path).expect("应能读取保存的图片文件");
        assert_eq!(saved_data, png_data, "保存的图片数据应与原始数据完全一致");

        // 验证文档内容已更新：在光标位置插入了图片 Markdown
        assert!(
            result.new_content.starts_with("Hello "),
            "新内容应以 'Hello ' 开头，实际: {}",
            result.new_content
        );
        assert!(
            result.new_content.contains("![image]("),
            "新内容应包含 Markdown 图片语法 ![image](...)，实际: {}",
            result.new_content
        );
        assert!(
            result.new_content.ends_with("World"),
            "新内容应以 'World' 结尾，实际: {}",
            result.new_content
        );
        assert!(
            result.new_content.len() > content.len(),
            "新内容长度({})应大于原始内容长度({})",
            result.new_content.len(),
            content.len()
        );

        // 验证非协作模式下 operation_json 为 None
        assert!(
            result.operation_json.is_none(),
            "非协作模式下 operation_json 应为 None"
        );

        // 清理测试产生的临时文件
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    /// 测试 process_paste_image 在协作模式下的行为
    ///
    /// 验证点：
    /// - 协作模式下 operation_json 不为 None
    /// - operation_json 可反序列化为有效的 OT Insert 操作
    /// - Insert 操作的位置和文本内容正确
    #[test]
    fn test_process_paste_image_collab() {
        use base64::Engine;

        // 最小有效 PNG 图片（与上一测试相同）
        let png_data: &[u8] = &[
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00,
            0x00, 0x90, 0x77, 0x53, 0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x08,
            0xD7, 0x63, 0xF8, 0xCF, 0xC0, 0x00, 0x00, 0x00, 0x03, 0x00, 0x01, 0x47, 0x53, 0x75,
            0x3E, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
        ];

        let base64_data = base64::engine::general_purpose::STANDARD.encode(png_data);
        let file_name = "test_collab_image.png";
        let content = "# 标题\n\n这是正文内容。";
        let cursor_pos = 4; // 在 "# 标题" 之后（第 4 个字符位置）插入

        let temp_dir = std::env::temp_dir().join("markstudio_test_collab_image");
        let cache_dir = temp_dir.to_string_lossy().to_string();

        let result = process_paste_image(
            &base64_data,
            file_name,
            content,
            cursor_pos,
            &cache_dir,
            true, // 启用协作模式
        )
        .expect("协作模式下的图片粘贴处理应成功");

        // 验证 operation_json 不为 None
        assert!(
            result.operation_json.is_some(),
            "协作模式下 operation_json 应不为 None"
        );

        // 验证 operation_json 可以反序列化为有效的 OT Insert 操作
        let json_str = result.operation_json.as_ref().unwrap();
        let op: crate::collaboration::ot::Operation =
            serde_json::from_str(json_str).expect("operation_json 应能成功反序列化为 Operation");

        // 验证反序列化后的操作类型和内容
        match op {
            crate::collaboration::ot::Operation::Insert { position, text } => {
                assert_eq!(
                    position, cursor_pos,
                    "Insert 操作的 position 应与 cursor_pos 一致"
                );
                assert!(
                    text.contains("![image]("),
                    "Insert 操作的 text 应包含图片 Markdown 语法，实际: {}",
                    text
                );
                assert!(
                    text.contains("test_collab_image.png"),
                    "Insert 操作的 text 应包含图片文件名，实际: {}",
                    text
                );
            }
            _ => panic!("期望 Insert 操作，但得到其他操作类型"),
        }

        // 验证文档内容已更新
        assert!(
            result.new_content.starts_with("# 标题"),
            "新内容应以 '# 标题' 开头，实际: {}",
            result.new_content
        );
        assert!(
            result.new_content.contains("![image]("),
            "新内容应包含图片 Markdown 语法，实际: {}",
            result.new_content
        );

        // 清理测试文件
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    // ================================================================
    // insert_at_position 辅助函数测试
    // ================================================================

    /// 测试 insert_at_position：在开头插入
    #[test]
    fn test_insert_at_beginning() {
        assert_eq!(insert_at_position("World", 0, "Hello "), "Hello World");
    }

    /// 测试 insert_at_position：在末尾插入
    #[test]
    fn test_insert_at_end() {
        assert_eq!(insert_at_position("Hello", 5, " World"), "Hello World");
    }

    /// 测试 insert_at_position：在中间插入
    #[test]
    fn test_insert_in_middle() {
        assert_eq!(insert_at_position("HelloWorld", 5, " "), "Hello World");
    }

    /// 测试 insert_at_position：插入位置超出文本长度时追加到末尾
    #[test]
    fn test_insert_beyond_length() {
        assert_eq!(insert_at_position("Hi", 100, "!"), "Hi!");
    }

    /// 测试 insert_at_position：向空文本插入
    #[test]
    fn test_insert_empty_text() {
        assert_eq!(insert_at_position("", 0, "Hello"), "Hello");
    }

    /// 测试 insert_at_position：正确处理中文等多字节 UTF-8 字符
    #[test]
    fn test_insert_with_chinese_chars() {
        // "你好世界" = 4 个字符，在位置 2（"你好"之后）插入 "，"
        assert_eq!(insert_at_position("你好世界", 2, "，"), "你好，世界");
    }
}
