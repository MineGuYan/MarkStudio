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
#[tauri::command]
pub fn read_file(path: String) -> Result<String, String> {
    crate::services::file_service::read_file_content(&path)
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
    crate::services::file_service::write_file_content(&path, &content)
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
    crate::services::file_service::add_recent_file_record(&path)
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
    crate::services::file_service::get_recent_file_list()
}

// ==================== 多人协作命令 ====================

/// 创建协作房间
///
/// 在当前主机上启动 WebSocket 服务器，创建协作房间。
/// 生成唯一的房间 ID，开始监听指定端口等待其他用户加入。
///
/// # 参数
/// - `port`: WebSocket 服务器监听端口
/// - `password`: 房间密码（空字符串表示无密码）
/// - `username`: 本地用户名
/// - `document`: 当前文档内容（用于同步给新加入者）
///
/// # 返回
/// `Result<RoomInfo, String>` - 成功时返回房间信息，失败时返回错误描述
#[tauri::command]
pub async fn create_collab_room(
    port: u16,
    password: String,
    username: String,
    document: String,
) -> Result<crate::collaboration::session::RoomInfo, String> {
    crate::collaboration::session::create_room(port, &password, &username, &document).await
}

/// 加入协作房间
///
/// 连接到目标主机上的协作房间，加入后自动同步当前文档内容。
///
/// # 参数
/// - `host`: 目标主机 IP 地址
/// - `port`: 目标主机 WebSocket 端口
/// - `room_id`: 房间唯一标识
/// - `password`: 房间密码（空字符串表示无密码）
/// - `username`: 本地用户名
///
/// # 返回
/// `Result<RoomInfo, String>` - 成功时返回房间信息，失败时返回错误描述
#[tauri::command]
pub async fn join_collab_room(
    host: String,
    port: u16,
    room_id: String,
    password: String,
    username: String,
) -> Result<crate::collaboration::session::RoomInfo, String> {
    crate::collaboration::session::join_room(&host, port, &room_id, &password, &username).await?;

    // 加入成功后，构造并返回 RoomInfo
    let session = crate::collaboration::session::get_session()
        .lock()
        .map_err(|e| format!("获取会话锁失败: {}", e))?;
    match session.as_ref() {
        Some(s) => Ok(crate::collaboration::session::RoomInfo {
            room_id: s.room_id.clone(),
            host_ip: host,
            port,
            peer_count: s.peers.len(),
        }),
        None => Err("加入房间后会话状态异常".to_string()),
    }
}

/// 离开协作房间
///
/// 通知其他对等方并清理当前协作会话的所有资源。
/// 如果当前不在协作会话中，此操作无效果。
/// 主机离开时会广播 HostDisconnected 消息给所有客户端，
/// 强制所有成员退出房间。
///
/// # 返回
/// 成功返回 Ok(())，失败返回错误描述字符串
#[tauri::command]
pub async fn leave_collab_room() -> Result<(), String> {
    crate::collaboration::session::leave_room().await
}

/// 发送编辑操作到协作房间
///
/// 将本地的编辑操作（由 OT 模块生成的 Operation）发送给房间中的其他对等方。
/// 操作以 JSON 字符串形式传递，由后端负责序列化/反序列化。
///
/// # 参数
/// - `op_json`: 编辑操作的 JSON 字符串表示
///
/// # 返回
/// 成功返回 Ok(())，失败返回错误描述字符串
#[tauri::command]
pub fn send_collab_operation(op_json: String) -> Result<(), String> {
    // 将 JSON 字符串反序列化为 Operation 对象
    let op: crate::collaboration::ot::Operation =
        serde_json::from_str(&op_json).map_err(|e| format!("操作反序列化失败: {}", e))?;

    crate::collaboration::session::send_operation(&op)
}

/// 发送光标位置同步
///
/// 将当前用户的光标位置广播给房间中的其他对等方。
///
/// # 参数
/// - `position`: 光标在文档中的偏移位置（字符索引）
///
/// # 返回
/// 成功返回 Ok(())，失败返回错误描述字符串
#[tauri::command]
pub fn send_collab_cursor(position: usize) -> Result<(), String> {
    crate::collaboration::session::send_cursor_sync(position)
}

/// 获取本机可用 IP 地址列表
///
/// 返回本机所有非回环的 IPv4 地址，用于显示在协作面板中，
/// 供其他用户加入房间时使用。
///
/// # 返回
/// 本机 IP 地址列表（字符串向量）
#[tauri::command]
pub fn get_local_ip() -> Result<Vec<String>, String> {
    crate::collaboration::session::get_local_ip()
}

/// 获取当前协作状态
///
/// 返回当前协作会话的状态信息，包括是否已连接、房间信息、在线用户列表等。
/// 前端可通过此命令轮询或按需获取协作状态。
///
/// # 返回
/// 协作状态信息的 JSON 字符串，包含以下字段：
/// - `connected`: 是否已连接
/// - `room_id`: 房间 ID
/// - `is_host`: 是否为房间主机
/// - `peer_count`: 在线用户数
/// - `peers`: 在线用户列表，每项包含 peer_id、username、cursor_position
/// - `local_peer_id`: 本地对等方 ID（用于前端区分"我"和他人）
/// - `local_username`: 本地用户名
/// - `document`: 当前共享文档内容
#[tauri::command]
pub fn get_collab_status() -> Result<String, String> {
    let session = crate::collaboration::session::get_session()
        .lock()
        .map_err(|e| format!("获取会话锁失败: {}", e))?;

    match session.as_ref() {
        Some(s) => {
            let status = serde_json::json!({
                "connected": s.connected,
                "room_id": s.room_id,
                "is_host": s.is_host,
                "peer_count": s.peers.len(),
                "peers": s.peers.iter().map(|p| serde_json::json!({
                    "peer_id": p.peer_id,
                    "username": p.username,
                    "cursor_position": p.cursor_position,
                    "is_host": p.is_host,
                })).collect::<Vec<_>>(),
                "local_peer_id": s.local_peer_id,
                "local_username": s.local_username,
                "document": s.document,
                "current_document_path": s.current_document_path,
                "shared_files": s.shared_files.iter().map(|f| serde_json::json!({
                    "path": f.path,
                    "title": f.title,
                    "content": f.content,
                })).collect::<Vec<_>>(),
                "disconnect_reason": null,
            });
            Ok(status.to_string())
        }
        None => {
            // 当会话不存在时，检查是否有断开原因记录（如主机关闭房间）
            let disconnect_reason = crate::collaboration::session::get_disconnect_reason();
            // 读取后立即清除，避免重复提示
            crate::collaboration::session::clear_disconnect_reason();
            Ok(serde_json::json!({
                "connected": false,
                "room_id": "",
                "is_host": false,
                "peer_count": 0,
                "peers": [],
                "local_peer_id": "",
                "local_username": "",
                "document": "",
                "disconnect_reason": disconnect_reason,
            })
            .to_string())
        }
    }
}

/// 设置协作会话中的本地用户名
///
/// 在已加入的协作房间中更新本地用户名。
/// 需要在加入或创建房间之后调用。
///
/// # 参数
/// - `username`: 新的用户名
///
/// # 返回
/// 成功返回 Ok(())，失败返回错误描述字符串
#[allow(dead_code)]
#[tauri::command]
pub fn set_collab_username(username: String) -> Result<(), String> {
    crate::collaboration::session::set_username(&username)
}

// ==================== 图片同步命令 ====================

/// 发送图片到协作房间
///
/// 读取本地图片文件，将其分片后通过协作会话发送给所有对等方。
/// 发送流程分为三个阶段：
/// 1. 发送 `ImageSyncStart` 通知对等方准备接收
/// 2. 逐个发送 `ImageSyncChunk` 传输分片数据
/// 3. 发送 `ImageSyncEnd` 通知对等方接收完成
///
/// # 参数
/// - `file_path`: 本地图片文件的完整路径
///
/// # 返回
/// - `Ok(())`: 发送成功
/// - `Err(String)`: 发送失败，返回错误描述
#[tauri::command]
pub fn send_collab_image(file_path: String) -> Result<(), String> {
    use base64::Engine;

    // 第一步：调用 sync 模块将图片文件分片
    let (file_name, chunks, file_size) =
        crate::collaboration::sync::prepare_image_sync(&file_path)?;
    let total_chunks = chunks.len() as u32;

    // 获取本地对等方 ID（需要先获取会话锁，克隆后立即释放）
    let local_peer_id = {
        let session_guard = crate::collaboration::session::get_session()
            .lock()
            .map_err(|e| format!("获取会话锁失败: {}", e))?;
        let session = session_guard
            .as_ref()
            .ok_or_else(|| "当前没有活跃的协作会话".to_string())?;
        session.local_peer_id.clone()
    };

    // 第二步：发送 ImageSyncStart 消息，通知对等方准备接收图片
    let start_msg = crate::collaboration::network::CollaborationMessage::ImageSyncStart {
        peer_id: local_peer_id.clone(),
        file_name: file_name.clone(),
        total_chunks,
        file_size,
    };
    crate::collaboration::session::send_message(&start_msg)?;

    // 第三步：逐个发送 ImageSyncChunk 消息，每个分片使用 Base64 编码
    for (i, chunk) in chunks.iter().enumerate() {
        let data_base64 = base64::engine::general_purpose::STANDARD.encode(chunk);
        let chunk_msg = crate::collaboration::network::CollaborationMessage::ImageSyncChunk {
            peer_id: local_peer_id.clone(),
            file_name: file_name.clone(),
            chunk_index: i as u32,
            total_chunks, // 每个分片都携带总分片数，以便接收方判断是否收齐
            data_base64,
        };
        crate::collaboration::session::send_message(&chunk_msg)?;
    }

    // 第四步：发送 ImageSyncEnd 消息，通知对等方图片传输完成
    let end_msg = crate::collaboration::network::CollaborationMessage::ImageSyncEnd {
        peer_id: local_peer_id.clone(),
        file_name: file_name.clone(),
    };
    crate::collaboration::session::send_message(&end_msg)?;

    Ok(())
}

/// 获取协作缓存目录路径
///
/// 返回协作缓存目录的完整路径字符串，前端可通过此路径
/// 访问协作过程中接收到的图片文件。
///
/// # 返回
/// - `Ok(String)`: 缓存目录的完整路径
/// - `Err(String)`: 获取失败，返回错误描述
#[tauri::command]
pub fn get_collab_cache_dir() -> Result<String, String> {
    crate::collaboration::sync::get_collab_cache_dir().map(|p| p.to_string_lossy().to_string())
}

/// 将 Base64 编码的图片数据保存到临时文件
///
/// 前端粘贴图片时，先将图片数据以 Base64 形式发送到后端，
/// 保存到临时目录中，返回保存后的文件路径。
/// 之后前端可调用 `send_collab_image` 将该图片发送给协作对等方。
///
/// # 参数
/// - `data_base64`: 图片的 Base64 编码数据（不含 data URI 前缀）
/// - `file_name`: 保存的文件名（不含路径）
///
/// # 返回
/// - `Ok(String)`: 保存后的完整文件路径
/// - `Err(String)`: 保存失败，返回错误描述
#[tauri::command]
pub fn save_temp_image(data_base64: String, file_name: String) -> Result<String, String> {
    use base64::Engine;

    // 解码 Base64 数据为二进制
    let data = base64::engine::general_purpose::STANDARD
        .decode(&data_base64)
        .map_err(|e| format!("Base64 解码失败: {}", e))?;

    // 在系统临时目录下创建 markstudio 图片临时目录
    let temp_dir = std::env::temp_dir().join("markstudio").join("images");
    std::fs::create_dir_all(&temp_dir).map_err(|e| format!("创建临时目录失败: {}", e))?;

    // 保存图片文件
    let file_path = temp_dir.join(&file_name);
    std::fs::write(&file_path, &data).map_err(|e| format!("保存图片文件失败: {}", e))?;

    Ok(file_path.to_string_lossy().to_string())
}

/// 将图片数据保存到图片缓存目录（非协作模式下使用）
///
/// 用户粘贴图片时，将图片保存到用户指定的缓存目录中。
/// 默认目录为 `data/image_cache/`，用户可在设置中更改。
/// 返回相对于项目根目录的相对路径，用于 Markdown 图片引用。
///
/// # 参数
/// - `data_base64`: 图片的 Base64 编码数据（不含 data URI 前缀）
/// - `file_name`: 保存的文件名（不含路径）
/// - `cache_dir`: 用户指定的缓存目录路径（可选，为空时使用默认路径）
///
/// # 返回
/// - `Ok(String)`: 保存后的文件路径（相对路径，可直接用于 Markdown 图片语法）
/// - `Err(String)`: 保存失败，返回错误描述
#[tauri::command]
pub fn save_image_cache(
    data_base64: String,
    file_name: String,
    cache_dir: Option<String>,
) -> Result<String, String> {
    use base64::Engine;

    // 解码 Base64 数据为二进制
    let data = base64::engine::general_purpose::STANDARD
        .decode(&data_base64)
        .map_err(|e| format!("Base64 解码失败: {}", e))?;

    // 确定缓存目录：优先使用用户指定的目录，否则使用默认的 data/image_cache/
    let cache_path = if let Some(dir) = cache_dir {
        if dir.trim().is_empty() {
            get_default_cache_dir()
        } else {
            std::path::PathBuf::from(&dir)
        }
    } else {
        get_default_cache_dir()
    };

    // 确保缓存目录存在
    std::fs::create_dir_all(&cache_path).map_err(|e| format!("创建图片缓存目录失败: {}", e))?;

    // 保存图片文件
    let file_path = cache_path.join(&file_name);
    std::fs::write(&file_path, &data).map_err(|e| format!("保存图片文件失败: {}", e))?;

    // 返回相对路径，便于在 Markdown 中使用
    Ok(file_path.to_string_lossy().to_string())
}

/// 获取默认的图片缓存目录路径
///
/// 默认路径为项目根目录下的 `data/image_cache/`。
/// 此函数会确保返回的路径是绝对路径。
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

// ==================== OT 操作命令 ====================

/// 计算 OT 操作（将编辑前后的文本差异转换为 OT 操作列表）
///
/// 此命令接收编辑前后的文本，通过后端的 OT 服务计算差异，
/// 返回一组 Insert/Delete 操作列表。
/// 前端在用户编辑文本时调用此命令获取 OT 操作，而非在前端自行计算。
///
/// # 参数
/// - `old_text`: 编辑前的文本内容
/// - `new_text`: 编辑后的文本内容
///
/// # 返回
/// OT 操作列表的 JSON 字符串
#[tauri::command]
pub fn compute_operation_cmd(old_text: String, new_text: String) -> Result<String, String> {
    let ops = crate::services::ot_service::compute_operation(&old_text, &new_text);
    serde_json::to_string(&ops).map_err(|e| format!("操作序列化失败: {}", e))
}

/// 将 OT 操作应用到文本上
///
/// 接收文本和 OT 操作的 JSON 字符串，在后端应用操作并返回结果文本。
/// 前端在需要合并远程操作或本地操作时调用此命令。
///
/// # 参数
/// - `text`: 当前文本内容
/// - `op_json`: OT 操作的 JSON 字符串
///
/// # 返回
/// 应用操作后的文本内容
#[tauri::command]
pub fn apply_operation_cmd(text: String, op_json: String) -> Result<String, String> {
    let op: crate::services::ot_service::Operation =
        serde_json::from_str(&op_json).map_err(|e| format!("操作反序列化失败: {}", e))?;
    Ok(crate::services::ot_service::apply_operation(&text, &op))
}

// ==================== 图片粘贴命令 ====================

/// 处理粘贴图片：保存图片并生成插入图片后的文档内容
///
/// 接收 Base64 编码的图片数据，保存到缓存目录，
/// 在指定光标位置插入 Markdown 图片语法，返回更新后的文档内容。
/// 协作模式下还会生成对应的 OT Insert 操作。
///
/// # 参数
/// - `base64_data`: 图片的 Base64 编码数据
/// - `file_name`: 文件名（含扩展名）
/// - `content`: 当前文档内容
/// - `cursor_pos`: 光标位置（字符偏移量）
/// - `cache_dir`: 图片缓存目录（可选，为空时使用默认目录）
/// - `collab_enabled`: 是否启用协作模式
///
/// # 返回
/// 包含新文档内容、文件路径和 OT 操作（协作模式）的 JSON 字符串
#[tauri::command]
pub fn paste_image_cmd(
    base64_data: String,
    file_name: String,
    content: String,
    cursor_pos: usize,
    cache_dir: Option<String>,
    collab_enabled: bool,
) -> Result<String, String> {
    let cache = cache_dir.unwrap_or_default();
    let result = crate::services::image_service::process_paste_image(
        &base64_data,
        &file_name,
        &content,
        cursor_pos,
        &cache,
        collab_enabled,
    )?;
    serde_json::to_string(&result).map_err(|e| format!("结果序列化失败: {}", e))
}

// ==================== 文档状态命令 ====================

/// 检查文档是否已被修改（脏状态）
///
/// 比较当前文档内容与上次保存/打开时的内容。
///
/// # 参数
/// - `current`: 当前文档内容
/// - `saved`: 上次保存/打开时的文档内容
///
/// # 返回
/// 文档是否已被修改（true 表示有未保存的更改）
#[tauri::command]
pub fn check_dirty_cmd(current: String, saved: String) -> bool {
    crate::services::document_service::check_dirty(&current, &saved)
}

/// 计算指定行的字符位置偏移量
///
/// 用于大纲导航：当用户点击大纲条目时，计算编辑器需要跳转到的字符位置。
///
/// # 参数
/// - `content`: 文档内容
/// - `line_number`: 目标行号（从 1 开始计数）
///
/// # 返回
/// 该行起始字符的偏移量
#[tauri::command]
pub fn compute_line_position_cmd(content: String, line_number: usize) -> usize {
    crate::services::document_service::compute_line_position(&content, line_number)
}

// ==================== 设置命令 ====================

/// 加载所有设置项（合并数据库值与默认值）
///
/// 从数据库加载已有的设置项，对于缺失的项使用默认值填充。
/// 返回包含所有设置项及其值的 JSON 字符串。
///
/// # 返回
/// 设置项键值对的 JSON 字符串
#[tauri::command]
pub fn load_all_settings_cmd() -> Result<String, String> {
    let settings = crate::services::settings_service::load_all_settings()?;
    serde_json::to_string(&settings).map_err(|e| format!("设置序列化失败: {}", e))
}

// ==================== 收藏夹命令 ====================

/// 获取收藏夹完整目录树
///
/// 递归获取所有目录和其中的文件列表，构建树形结构返回。
///
/// # 返回
/// 收藏夹目录树的 JSON 字符串
#[tauri::command]
pub fn get_favorite_tree() -> Result<String, String> {
    let tree = crate::database::get_favorite_tree()?;
    serde_json::to_string(&tree).map_err(|e| format!("收藏夹目录树序列化失败: {}", e))
}

/// 创建收藏夹目录
///
/// # 参数
/// - `name`: 目录名称
/// - `parent_id`: 父目录 ID（可选，为空表示根目录）
///
/// # 返回
/// 新创建目录的 ID
#[tauri::command]
pub fn create_favorite_dir(name: String, parent_id: Option<i64>) -> Result<i64, String> {
    crate::database::create_favorite_dir(&name, parent_id)
}

/// 删除收藏夹目录（级联删除子目录和文件）
///
/// # 参数
/// - `id`: 要删除的目录 ID
///
/// # 返回
/// 成功返回 Ok(())
#[tauri::command]
pub fn delete_favorite_dir(id: i64) -> Result<(), String> {
    crate::database::delete_favorite_dir(id)
}

/// 重命名收藏夹目录
///
/// # 参数
/// - `id`: 目录 ID
/// - `name`: 新名称
///
/// # 返回
/// 成功返回 Ok(())
#[tauri::command]
pub fn rename_favorite_dir(id: i64, name: String) -> Result<(), String> {
    crate::database::rename_favorite_dir(id, &name)
}

/// 添加文件到收藏夹目录
///
/// # 参数
/// - `path`: 文件路径
/// - `dir_id`: 目标目录 ID
///
/// # 返回
/// 新创建收藏文件记录的 ID
#[tauri::command]
pub fn add_favorite_file(path: String, dir_id: i64) -> Result<i64, String> {
    crate::database::add_favorite_file(&path, dir_id)
}

/// 从收藏夹移除文件
///
/// # 参数
/// - `id`: 收藏文件记录 ID
///
/// # 返回
/// 成功返回 Ok(())
#[tauri::command]
pub fn remove_favorite_file(id: i64) -> Result<(), String> {
    crate::database::remove_favorite_file(id)
}

// ==================== 标签页管理命令 ====================

/// 保存当前打开的标签页信息
///
/// 在应用关闭时调用，保存所有标签页信息到数据库，
/// 以便下次启动时恢复。
///
/// # 参数
/// - `tabs_json`: 标签页信息的 JSON 字符串
/// - `active_index`: 当前激活的标签页索引
///
/// # 返回
/// 成功返回 Ok(())
#[tauri::command]
pub fn save_open_tabs(tabs_json: String, active_index: usize) -> Result<(), String> {
    crate::database::save_open_tabs(&tabs_json, active_index)
}

/// 获取上次打开的标签页信息
///
/// 在应用启动时调用，读取上次保存的标签页信息。
///
/// # 返回
/// 标签页信息的 JSON 字符串
#[tauri::command]
pub fn get_open_tabs() -> Result<String, String> {
    crate::database::get_open_tabs()
}

// ==================== 最近文件命令 ====================

/// 从最近文件列表中移除指定路径的记录
///
/// # 参数
/// - `path`: 要移除的文件路径
///
/// # 返回
/// 成功返回 Ok(())
#[tauri::command]
pub fn remove_recent_file(path: String) -> Result<(), String> {
    crate::database::remove_recent_file(&path)
}

// ==================== 文件检查命令 ====================

/// 检查文件是否存在
///
/// # 参数
/// - `path`: 文件路径
///
/// # 返回
/// 文件是否存在
#[tauri::command]
pub fn check_file_exists(path: String) -> bool {
    std::path::Path::new(&path).exists()
}

// ==================== 协作共享文件命令 ====================

/// 添加共享文件到协作房间
///
/// 仅主机可调用此命令。将文件添加到共享文件列表，
/// 并广播更新给所有客户端。
///
/// # 参数
/// - `path`: 文件完整路径
/// - `title`: 文件显示名称
/// - `content`: 文件内容
///
/// # 返回
/// 成功返回 Ok(())
#[tauri::command]
pub fn add_shared_file(path: String, title: String, content: String) -> Result<(), String> {
    crate::collaboration::session::add_shared_file(&path, &title, &content)
}

/// 从协作房间移除共享文件
///
/// 仅主机可调用此命令。从共享文件列表中移除指定文件，
/// 并广播更新给所有客户端。
///
/// # 参数
/// - `path`: 要移除的文件路径
///
/// # 返回
/// 成功返回 Ok(())
#[tauri::command]
pub fn remove_shared_file(path: String) -> Result<(), String> {
    crate::collaboration::session::remove_shared_file(&path)
}

/// 获取当前协作房间的共享文件列表
///
/// # 返回
/// 共享文件列表的 JSON 字符串
#[tauri::command]
pub fn get_shared_files() -> Result<String, String> {
    let files = crate::collaboration::session::get_shared_files()?;
    serde_json::to_string(&files).map_err(|e| format!("共享文件列表序列化失败: {}", e))
}
