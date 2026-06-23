//! 图片同步模块
//!
//! 本模块负责协作编辑中的图片文件同步功能，包括：
//! - 图片文件的分片与重组
//! - 多图片接收的并发管理
//! - Markdown 中图片路径的本地化替换
//! - 协作缓存目录的清理

#![allow(dead_code)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use base64::Engine;
use serde::{Deserialize, Serialize};

// ============================================================================
// 常量定义
// ============================================================================

/// 图片分片大小（64KB）
/// 每个分片最大 64KB，确保在 WebSocket 等传输协议中不会因单帧过大
/// 而导致性能问题或连接中断。
const CHUNK_SIZE: usize = 64 * 1024;

// ============================================================================
// 数据结构定义
// ============================================================================

/// 图片同步信息，用于跟踪正在接收的图片文件。
///
/// 当接收方收到 `ImageSyncStart` 消息后，会创建一个 `ImageSyncInfo` 实例，
/// 随后每收到一个 `ImageSyncChunk` 就将分片数据追加到 `chunks` 中，
/// 当 `chunks` 数量达到 `total_chunks` 时，所有分片已到达，可重组为完整文件。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageSyncInfo {
    /// 图片文件名（不含路径）
    pub file_name: String,
    /// 总分片数量
    pub total_chunks: u32,
    /// 文件总大小（字节）
    pub file_size: u64,
    /// 已接收的分片数据列表
    pub chunks: Vec<Vec<u8>>,
}

// ============================================================================
// 全局状态：正在接收的图片缓冲区
// ============================================================================

/// 全局图片接收缓冲区，用于跟踪多个正在接收的图片文件。
///
/// 键为文件名，值为对应的 `ImageSyncInfo` 实例。
/// 使用 `Mutex` 保证线程安全，因为在多线程协作环境中，
/// 可能同时有多个 WebSocket 消息处理任务在写入分片数据。
/// 使用 `LazyLock` 延迟初始化，因为 `HashMap::new()` 不是 const 函数。
static IMAGE_RECEIVE_BUFFER: std::sync::LazyLock<Mutex<HashMap<String, ImageSyncInfo>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

/// 获取全局图片接收缓冲区的引用。
///
/// 返回 `&Mutex<HashMap<String, ImageSyncInfo>>`，调用方可通过 `.lock()` 获取
/// 内部 `HashMap` 的读写访问权。用于外部模块（如 session）在收到
/// `ImageSyncStart` 消息时预初始化缓冲区条目。
pub fn image_receive_buffer() -> &'static Mutex<HashMap<String, ImageSyncInfo>> {
    &IMAGE_RECEIVE_BUFFER
}

// ============================================================================
// 公共 API
// ============================================================================

/// 获取图片缓存目录路径（与 `save_image_cache` 使用同一目录）。
///
/// 返回项目根目录下的 `data/image_cache` 子目录，如果目录不存在则自动创建。
/// 在 Tauri 开发模式下，`current_dir()` 返回 `src-tauri/`，
/// 需要回退到项目根目录以确保路径一致。
///
/// # 返回
/// - `Ok(PathBuf)`: 图片缓存目录的完整路径
/// - `Err(String)`: 创建失败，返回错误描述
pub fn get_image_cache_dir() -> Result<PathBuf, String> {
    let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    // 开发模式下（cargo run），当前工作目录为 src-tauri/，需要回退到项目根目录
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

    let cache_dir = project_root.join("data").join("image_cache");

    // 如果目录不存在则递归创建
    std::fs::create_dir_all(&cache_dir).map_err(|e| format!("创建图片缓存目录失败: {}", e))?;

    Ok(cache_dir)
}

/// 获取协作缓存目录路径。
///
/// 在系统临时目录下创建 `markstudio/collab_cache` 子目录，
/// 用于存放协作过程中接收的图片文件。
/// 如果目录不存在则自动创建。
///
/// # 返回
/// - `Ok(PathBuf)`: 缓存目录的完整路径
/// - `Err(String)`: 创建失败，返回错误描述
pub fn get_collab_cache_dir() -> Result<PathBuf, String> {
    // 使用系统临时目录作为基础路径
    let cache_dir = std::env::temp_dir().join("markstudio").join("collab_cache");

    // 如果目录不存在则递归创建
    std::fs::create_dir_all(&cache_dir).map_err(|e| format!("创建协作缓存目录失败: {}", e))?;

    Ok(cache_dir)
}

/// 将图片文件读入并分片。
///
/// 读取指定路径的图片文件，按照 `CHUNK_SIZE` 将其切分为多个分片，
/// 以便通过协作消息分片传输。
///
/// # 参数
/// - `file_path`: 图片文件的完整路径
///
/// # 返回
/// - `Ok((String, Vec<Vec<u8>>, u64))`: 返回包含 (文件名, 分片列表, 文件大小) 的三元组
/// - `Err(String)`: 读取或分片失败，返回错误描述
///
/// # 示例
/// ```
/// // 假设存在图片文件
/// // let (name, chunks, size) = prepare_image_sync("photo.png").unwrap();
/// // assert!(!chunks.is_empty());
/// // assert!(size > 0);
/// ```
pub fn prepare_image_sync(file_path: &str) -> Result<(String, Vec<Vec<u8>>, u64), String> {
    let path = Path::new(file_path);

    // 验证文件存在
    if !path.exists() {
        return Err(format!("图片文件不存在: {}", file_path));
    }

    // 验证路径指向的是普通文件而非目录
    if !path.is_file() {
        return Err(format!("路径不是文件: {}", file_path));
    }

    // 提取文件名（不含父目录路径）
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| format!("无法提取文件名: {}", file_path))?
        .to_string();

    // 读取文件全部内容到内存
    let file_data = std::fs::read(path).map_err(|e| format!("读取图片文件失败: {}", e))?;

    let file_size = file_data.len() as u64;

    // 将文件数据按 CHUNK_SIZE 切分为多个分片
    let chunks: Vec<Vec<u8>> = file_data
        .chunks(CHUNK_SIZE)
        .map(|chunk| chunk.to_vec())
        .collect();

    Ok((file_name, chunks, file_size))
}

/// 将 Base64 编码的分片数据保存到缓存，当所有分片到达后重组为完整文件。
///
/// 接收方在收到 `ImageSyncChunk` 消息后调用此函数，
/// 将 Base64 编码的分片数据解码并追加到对应文件的接收缓冲区中。
/// 当所有分片到达后，按索引顺序重组为完整文件并保存到缓存目录。
///
/// # 参数
/// - `file_name`: 图片文件名
/// - `chunk_index`: 当前分片索引（从 0 开始）
/// - `total_chunks`: 总分片数量
/// - `data_base64`: Base64 编码的分片数据
///
/// # 返回
/// - `Ok(Some(String))`: 所有分片已到达，返回保存后的本地文件路径
/// - `Ok(None)`: 还有分片未到达，等待更多分片
/// - `Err(String)`: 解码或保存失败，返回错误描述
pub fn receive_image_chunk(
    file_name: &str,
    chunk_index: u32,
    total_chunks: u32,
    data_base64: &str,
) -> Result<Option<String>, String> {
    // 解码 Base64 编码的分片数据
    let chunk_data = base64::engine::general_purpose::STANDARD
        .decode(data_base64)
        .map_err(|e| format!("Base64 解码失败: {}", e))?;

    // 获取全局图片接收缓冲区的互斥锁
    let mut buffer = IMAGE_RECEIVE_BUFFER
        .lock()
        .map_err(|e| format!("获取图片接收缓冲区锁失败: {}", e))?;

    // 查找或创建该文件的接收信息记录
    let info = buffer
        .entry(file_name.to_string())
        .or_insert_with(|| ImageSyncInfo {
            file_name: file_name.to_string(),
            total_chunks,
            file_size: 0,
            chunks: Vec::new(),
        });

    // 确保 chunks 数组足够大，可以容纳当前分片索引
    // 分片可能乱序到达，因此需要按索引位置存储
    let target_index = chunk_index as usize;
    if info.chunks.len() <= target_index {
        info.chunks.resize(target_index + 1, Vec::new());
    }

    // 将解码后的分片数据存入对应索引位置
    info.chunks[target_index] = chunk_data;

    // 统计实际已到达的非空分片数量
    let actual_chunk_count = info.chunks.iter().filter(|c| !c.is_empty()).count() as u32;

    if actual_chunk_count >= total_chunks {
        // 所有分片已到达，按索引顺序拼接分片数据重组为完整文件
        let mut file_data = Vec::new();
        for chunk in &info.chunks {
            if !chunk.is_empty() {
                file_data.extend_from_slice(chunk);
            }
        }

        // 获取图片缓存目录并将重组后的文件保存到磁盘
        // 使用与发送方相同的 image_cache 目录，确保路径一致
        let cache_dir = get_image_cache_dir()?;
        let file_path = cache_dir.join(file_name);

        std::fs::write(&file_path, &file_data).map_err(|e| format!("保存图片文件失败: {}", e))?;

        // 从缓冲区中移除该文件的记录，释放内存
        buffer.remove(file_name);

        // 返回保存后的本地文件路径
        Ok(Some(file_path.to_string_lossy().to_string()))
    } else {
        // 分片尚未到齐，返回 None 表示还需等待
        Ok(None)
    }
}

/// 将 Markdown 中的图片引用路径替换为本地缓存路径。
///
/// 扫描 Markdown 文本中的图片引用语法 `![alt](path)`，
/// 提取每个图片路径中的文件名，将其替换为缓存目录下的对应文件路径。
/// 这样协作方接收到的图片可以正确在本地渲染。
///
/// # 参数
/// - `markdown`: 原始 Markdown 文本
/// - `cache_dir`: 缓存目录路径
///
/// # 返回
/// 替换图片路径后的 Markdown 文本
pub fn replace_image_paths(markdown: &str, cache_dir: &Path) -> String {
    let mut result = String::with_capacity(markdown.len());
    let len = markdown.len();
    let mut i = 0;

    while i < len {
        // 检测 `![` 开头，表示可能是图片引用语法
        if markdown[i..].starts_with("![") {
            // 查找 alt 文本的结束位置 `]`
            if let Some(alt_end) = markdown[i + 2..].find(']') {
                let alt_end_abs = i + 2 + alt_end;

                // 检查 `]` 之后是否紧跟 `(`
                if alt_end_abs + 1 < len && markdown[alt_end_abs + 1..].starts_with('(') {
                    // 查找路径的结束位置 `)`
                    let path_start = alt_end_abs + 2;
                    if let Some(path_end) = markdown[path_start..].find(')') {
                        let path_end_abs = path_start + path_end;
                        let remote_path = &markdown[path_start..path_end_abs];

                        // 从远程路径中提取文件名
                        let file_name = Path::new(remote_path)
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or(remote_path);

                        // 构建本地缓存路径
                        let local_path = cache_dir.join(file_name);
                        let local_path_str = local_path.to_string_lossy();

                        // 仅当缓存文件实际存在时才替换路径，
                        // 防止在图片分片尚未到达时错误地替换为不存在的路径
                        if local_path.exists() {
                            // 重构图片引用：保持 alt 文本不变，替换为本地缓存路径
                            result.push_str("![");
                            result.push_str(&markdown[i + 2..alt_end_abs]);
                            result.push_str("](");
                            result.push_str(&local_path_str);
                            result.push(')');
                        } else {
                            // 文件尚未到达，保持原始路径不变
                            result.push_str(&markdown[i..=path_end_abs]);
                        }

                        i = path_end_abs + 1; // 跳过 `)`
                        continue;
                    }
                }
            }
        }

        // 普通字符，逐字符追加（正确处理 UTF-8 多字节字符）
        if let Some(c) = markdown[i..].chars().next() {
            result.push(c);
            i += c.len_utf8();
        } else {
            break;
        }
    }

    result
}

/// 清理协作缓存目录。
///
/// 删除协作缓存目录及其所有内容，用于在退出协作会话时清理临时文件。
/// 同时清空内存中的图片接收缓冲区。
/// 如果目录不存在，此操作不报错（静默忽略）。
///
/// # 返回
/// - `Ok(())`: 清理成功或目录不存在
/// - `Err(String)`: 清理失败，返回错误描述
pub fn clear_collab_cache() -> Result<(), String> {
    // 获取缓存目录路径
    let cache_dir = match get_collab_cache_dir() {
        Ok(dir) => dir,
        Err(_) => return Ok(()), // 如果连目录都获取不到，说明无需清理
    };

    // 如果目录存在则删除整个目录树
    if cache_dir.exists() {
        std::fs::remove_dir_all(&cache_dir).map_err(|e| format!("清理协作缓存目录失败: {}", e))?;
    }

    // 同时清空内存中的接收缓冲区
    let mut buffer = IMAGE_RECEIVE_BUFFER
        .lock()
        .map_err(|e| format!("获取图片接收缓冲区锁失败: {}", e))?;
    buffer.clear();

    Ok(())
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
#[allow(clippy::needless_borrows_for_generic_args)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::Mutex as StdMutex;

    /// 测试级互斥锁——由于所有测试共享全局 `IMAGE_RECEIVE_BUFFER` 和缓存目录，
    /// 必须确保同一时间只有一个测试访问这些共享资源，避免竞态条件。
    static TEST_MUTEX: StdMutex<()> = StdMutex::new(());

    // ========================================================================
    // 测试辅助函数与结构体
    // ========================================================================

    /// 临时目录结构体，在创建时生成唯一目录，在析构时自动清理。
    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        /// 创建一个新的临时目录用于测试。
        fn new() -> Self {
            let dir =
                std::env::temp_dir().join(format!("markstudio_sync_test_{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&dir).expect("创建测试临时目录失败");
            TempDir { path: dir }
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            if self.path.exists() {
                let _ = std::fs::remove_dir_all(&self.path);
            }
        }
    }

    /// 在指定目录中创建一个测试图片文件，写入指定数据。
    fn create_test_image(dir: &Path, name: &str, data: &[u8]) -> PathBuf {
        let path = dir.join(name);
        let mut file = std::fs::File::create(&path).expect("创建测试文件失败");
        file.write_all(data).expect("写入测试数据失败");
        path
    }

    // ========================================================================
    // prepare_image_sync 测试
    // ========================================================================

    #[test]
    fn test_prepare_image_sync_small_file() {
        let temp = TempDir::new();
        let test_data = b"Hello, this is a small test image!";
        let file_path = create_test_image(&temp.path, "test.png", test_data);

        let (name, chunks, size) =
            prepare_image_sync(&file_path.to_string_lossy()).expect("分片准备应成功");

        // 验证返回值
        assert_eq!(name, "test.png");
        assert_eq!(size, test_data.len() as u64);
        // 小文件（小于 64KB）只有一个分片
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], test_data);
    }

    #[test]
    fn test_prepare_image_sync_large_file() {
        let temp = TempDir::new();
        // 创建一个大于 CHUNK_SIZE 的文件（约 128KB + 500 字节）
        let test_data = vec![0x42u8; CHUNK_SIZE * 2 + 500];
        let file_path = create_test_image(&temp.path, "large.png", &test_data);

        let (name, chunks, size) =
            prepare_image_sync(&file_path.to_string_lossy()).expect("分片准备应成功");

        assert_eq!(name, "large.png");
        assert_eq!(size, test_data.len() as u64);
        // 应产生 3 个分片：2 个完整的 64KB + 1 个 500 字节
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].len(), CHUNK_SIZE);
        assert_eq!(chunks[1].len(), CHUNK_SIZE);
        assert_eq!(chunks[2].len(), 500);

        // 验证拼接后的数据与原始数据完全一致
        let combined: Vec<u8> = chunks.iter().flat_map(|c| c.iter().copied()).collect();
        assert_eq!(combined, test_data);
    }

    #[test]
    fn test_prepare_image_sync_exact_chunk_size() {
        let temp = TempDir::new();
        // 创建恰好一个分片大小的文件
        let test_data = vec![0xABu8; CHUNK_SIZE];
        let file_path = create_test_image(&temp.path, "exact.png", &test_data);

        let (name, chunks, size) =
            prepare_image_sync(&file_path.to_string_lossy()).expect("分片准备应成功");

        assert_eq!(name, "exact.png");
        assert_eq!(size, CHUNK_SIZE as u64);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].len(), CHUNK_SIZE);
    }

    #[test]
    fn test_prepare_image_sync_file_not_found() {
        let result = prepare_image_sync("nonexistent_file_12345_xyz.png");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("不存在"));
    }

    #[test]
    fn test_prepare_image_sync_path_is_directory() {
        let temp = TempDir::new();
        // 传入目录路径而非文件路径
        let result = prepare_image_sync(&temp.path.to_string_lossy());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("不是文件"));
    }

    #[test]
    fn test_prepare_image_sync_empty_file() {
        let temp = TempDir::new();
        let file_path = create_test_image(&temp.path, "empty.png", &[]);

        let (name, chunks, size) =
            prepare_image_sync(&file_path.to_string_lossy()).expect("分片准备应成功");

        assert_eq!(name, "empty.png");
        assert_eq!(size, 0);
        // 空文件的 chunks 迭代器返回空，因此 chunks 为空
        assert!(chunks.is_empty());
    }

    // ========================================================================
    // receive_image_chunk 测试
    // ========================================================================

    #[test]
    fn test_receive_image_chunk_single_chunk() {
        let _test_guard = TEST_MUTEX.lock().unwrap();
        // 清理可能残留的缓冲区数据
        {
            let mut buffer = IMAGE_RECEIVE_BUFFER.lock().unwrap();
            buffer.clear();
        }

        let test_data = b"Small test image data for single chunk test";
        let base64_data = base64::engine::general_purpose::STANDARD.encode(test_data);

        // 接收单个分片（总数为 1，索引为 0）
        let result =
            receive_image_chunk("single_test.png", 0, 1, &base64_data).expect("接收分片应成功");

        // 所有分片已到达，应返回文件路径
        assert!(result.is_some());
        let file_path = result.unwrap();

        // 验证文件已正确保存
        let saved_data = std::fs::read(&file_path).expect("应能读取保存的文件");
        assert_eq!(saved_data, test_data);

        // 清理保存的文件
        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn test_receive_image_chunk_multiple_chunks() {
        let _test_guard = TEST_MUTEX.lock().unwrap();
        // 清理可能残留的缓冲区数据
        {
            let mut buffer = IMAGE_RECEIVE_BUFFER.lock().unwrap();
            buffer.clear();
        }

        let chunk1 = b"First chunk of data ";
        let chunk2 = b"Second chunk of data ";
        let chunk3 = b"Third chunk of data";

        let b64_1 = base64::engine::general_purpose::STANDARD.encode(chunk1);
        let b64_2 = base64::engine::general_purpose::STANDARD.encode(chunk2);
        let b64_3 = base64::engine::general_purpose::STANDARD.encode(chunk3);

        // 接收第 0 个分片 — 不应触发完成
        let result1 =
            receive_image_chunk("multi_test.png", 0, 3, &b64_1).expect("接收分片 0 应成功");
        assert!(result1.is_none(), "第一个分片不应触发完成");

        // 接收第 1 个分片 — 不应触发完成
        let result2 =
            receive_image_chunk("multi_test.png", 1, 3, &b64_2).expect("接收分片 1 应成功");
        assert!(result2.is_none(), "第二个分片不应触发完成");

        // 接收第 2 个分片（最后一个）— 应触发完成
        let result3 =
            receive_image_chunk("multi_test.png", 2, 3, &b64_3).expect("接收分片 2 应成功");
        assert!(result3.is_some(), "最后一个分片应触发完成");

        let file_path = result3.unwrap();
        let saved_data = std::fs::read(&file_path).expect("应能读取保存的文件");
        let expected = b"First chunk of data Second chunk of data Third chunk of data";
        assert_eq!(saved_data, expected.to_vec());

        // 清理
        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn test_receive_image_chunk_out_of_order() {
        let _test_guard = TEST_MUTEX.lock().unwrap();
        // 清理可能残留的缓冲区数据
        {
            let mut buffer = IMAGE_RECEIVE_BUFFER.lock().unwrap();
            buffer.clear();
        }

        let chunk0 = b"AAAA";
        let chunk1 = b"BBBB";
        let chunk2 = b"CCCC";

        let b64_0 = base64::engine::general_purpose::STANDARD.encode(chunk0);
        let b64_1 = base64::engine::general_purpose::STANDARD.encode(chunk1);
        let b64_2 = base64::engine::general_purpose::STANDARD.encode(chunk2);

        // 乱序接收分片：先收分片 2，再收分片 0，最后收分片 1
        let r2 = receive_image_chunk("out_of_order.png", 2, 3, &b64_2).unwrap();
        assert!(r2.is_none(), "分片 2 不应触发完成");

        let r0 = receive_image_chunk("out_of_order.png", 0, 3, &b64_0).unwrap();
        assert!(r0.is_none(), "分片 0 不应触发完成");

        let r1 = receive_image_chunk("out_of_order.png", 1, 3, &b64_1).unwrap();
        assert!(r1.is_some(), "分片 1 应触发完成（最后一个到达）");

        let file_path = r1.unwrap();
        let saved_data = std::fs::read(&file_path).unwrap();
        // 验证拼接顺序正确（按索引 0, 1, 2 顺序，而非接收顺序）
        let expected = b"AAAABBBBCCCC";
        assert_eq!(saved_data, expected.to_vec());

        // 清理
        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn test_receive_image_chunk_invalid_base64() {
        let _test_guard = TEST_MUTEX.lock().unwrap();
        let result = receive_image_chunk("bad.png", 0, 1, "!!!这不是合法的 Base64 字符串!!!");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Base64"));
    }

    #[test]
    fn test_receive_image_chunk_two_files_concurrently() {
        let _test_guard = TEST_MUTEX.lock().unwrap();
        // 清理可能残留的缓冲区数据
        {
            let mut buffer = IMAGE_RECEIVE_BUFFER.lock().unwrap();
            buffer.clear();
        }

        let data_a = b"File A content";
        let data_b = b"File B content here";

        let b64_a = base64::engine::general_purpose::STANDARD.encode(data_a);
        let b64_b = base64::engine::general_purpose::STANDARD.encode(data_b);

        // 交错接收两个文件的分片
        let r_a = receive_image_chunk("file_a.png", 0, 1, &b64_a).unwrap();
        assert!(r_a.is_some(), "文件 A 应立即完成");

        let r_b = receive_image_chunk("file_b.png", 0, 1, &b64_b).unwrap();
        assert!(r_b.is_some(), "文件 B 应立即完成");

        // 验证两个文件内容均正确
        let saved_a = std::fs::read(r_a.unwrap()).unwrap();
        let saved_b = std::fs::read(r_b.unwrap()).unwrap();
        assert_eq!(saved_a, data_a);
        assert_eq!(saved_b, data_b);

        // 清理
        let _ = std::fs::remove_file(&get_image_cache_dir().unwrap().join("file_a.png"));
        let _ = std::fs::remove_file(&get_image_cache_dir().unwrap().join("file_b.png"));
    }

    // ========================================================================
    // replace_image_paths 测试
    // ========================================================================

    #[test]
    fn test_replace_image_paths_single_image() {
        let temp_dir = std::env::temp_dir().join("ms_test_replace_single");
        let _ = std::fs::create_dir_all(&temp_dir);
        // 必须在缓存目录中创建同名文件，replace_image_paths 才会替换路径
        std::fs::write(temp_dir.join("image.png"), b"fake").unwrap();

        let markdown = "这是文本 ![描述](https://example.com/image.png) 更多文本";

        let result = replace_image_paths(markdown, &temp_dir);

        let expected = format!(
            "这是文本 ![描述]({}) 更多文本",
            temp_dir.join("image.png").display()
        );
        assert_eq!(result, expected);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_replace_image_paths_multiple_images() {
        let temp_dir = std::env::temp_dir().join("ms_test_replace_multi");
        let _ = std::fs::create_dir_all(&temp_dir);
        std::fs::write(temp_dir.join("1.png"), b"fake").unwrap();
        std::fs::write(temp_dir.join("2.jpg"), b"fake").unwrap();

        let markdown = "![a](http://a.com/1.png) 文本 ![b](http://b.com/2.jpg)";

        let result = replace_image_paths(markdown, &temp_dir);

        let expected_path_1 = temp_dir.join("1.png");
        let expected_path_2 = temp_dir.join("2.jpg");
        assert!(
            result.contains(&expected_path_1.to_string_lossy().to_string()),
            "结果应包含路径: {}",
            expected_path_1.display()
        );
        assert!(
            result.contains(&expected_path_2.to_string_lossy().to_string()),
            "结果应包含路径: {}",
            expected_path_2.display()
        );
        assert!(!result.contains("http://"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_replace_image_paths_no_images() {
        let temp_dir = std::env::temp_dir().join("ms_test_replace_none");
        let markdown = "这是普通文本，没有图片引用。\n\n还有 [链接](http://example.com) 但不是图片";

        let result = replace_image_paths(markdown, &temp_dir);

        assert_eq!(result, markdown);
    }

    #[test]
    fn test_replace_image_paths_local_path() {
        let temp_dir = std::env::temp_dir().join("ms_test_replace_local");
        let _ = std::fs::create_dir_all(&temp_dir);
        std::fs::write(temp_dir.join("pic.png"), b"fake").unwrap();

        let markdown = "![local](C:/Users/test/pic.png)";

        let result = replace_image_paths(markdown, &temp_dir);

        let expected = temp_dir.join("pic.png");
        assert!(
            result.contains(&expected.to_string_lossy().to_string()),
            "结果应包含路径: {}",
            expected.display()
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_replace_image_paths_empty_markdown() {
        let temp_dir = std::env::temp_dir().join("ms_test_replace_empty");
        let result = replace_image_paths("", &temp_dir);
        assert_eq!(result, "");
    }

    #[test]
    fn test_replace_image_paths_image_without_alt() {
        let temp_dir = std::env::temp_dir().join("ms_test_replace_noalt");
        let _ = std::fs::create_dir_all(&temp_dir);
        std::fs::write(temp_dir.join("photo.png"), b"fake").unwrap();

        let markdown = "![](photo.png)";

        let result = replace_image_paths(markdown, &temp_dir);

        let expected = format!("![]({})", temp_dir.join("photo.png").display());
        assert_eq!(result, expected);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    /// 测试文件不存在时不替换路径的行为（新增）
    #[test]
    fn test_replace_image_paths_file_not_exists() {
        let temp_dir = std::env::temp_dir().join("ms_test_replace_missing");
        // 不创建文件，确保路径不会被替换
        let markdown = "![missing](https://example.com/nonexistent.png)";

        let result = replace_image_paths(markdown, &temp_dir);

        // 文件不存在，路径应保持原样
        assert_eq!(result, markdown);
    }

    // ========================================================================
    // clear_collab_cache 测试
    // ========================================================================

    #[test]
    fn test_clear_collab_cache_clears_buffer() {
        let _test_guard = TEST_MUTEX.lock().unwrap();
        // 先在缓冲区中放入一些测试数据
        {
            let mut buffer = IMAGE_RECEIVE_BUFFER.lock().unwrap();
            buffer.insert(
                "test.png".to_string(),
                ImageSyncInfo {
                    file_name: "test.png".to_string(),
                    total_chunks: 3,
                    file_size: 100,
                    chunks: vec![vec![1, 2, 3]],
                },
            );
            buffer.insert(
                "test2.jpg".to_string(),
                ImageSyncInfo {
                    file_name: "test2.jpg".to_string(),
                    total_chunks: 1,
                    file_size: 50,
                    chunks: vec![vec![4, 5, 6]],
                },
            );
        }

        // 执行清理
        clear_collab_cache().expect("清理缓存应成功");

        // 验证缓冲区已完全清空
        let buffer = IMAGE_RECEIVE_BUFFER.lock().unwrap();
        assert!(buffer.is_empty(), "缓冲区在清理后应为空");
    }

    // ========================================================================
    // get_collab_cache_dir 测试
    // ========================================================================

    #[test]
    fn test_get_collab_cache_dir_returns_valid_path() {
        let result = get_collab_cache_dir();
        assert!(result.is_ok(), "获取缓存目录应成功");

        let dir = result.unwrap();
        // 验证路径包含预期的子目录名
        let dir_str = dir.to_string_lossy();
        assert!(dir_str.contains("markstudio"), "路径应包含 'markstudio'");
        assert!(
            dir_str.contains("collab_cache"),
            "路径应包含 'collab_cache'"
        );
        // 验证目录已实际创建在磁盘上
        assert!(dir.exists(), "缓存目录应已创建");
    }
}
