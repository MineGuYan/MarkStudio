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
///
/// # 返回
/// 成功返回 Ok(())，失败返回错误描述字符串
#[tauri::command]
pub fn leave_collab_room() -> Result<(), String> {
    crate::collaboration::session::leave_room()
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
                })).collect::<Vec<_>>(),
                "local_peer_id": s.local_peer_id,
                "local_username": s.local_username,
                "document": s.document,
            });
            Ok(status.to_string())
        }
        None => Ok(serde_json::json!({
            "connected": false,
            "room_id": "",
            "is_host": false,
            "peer_count": 0,
            "peers": [],
            "local_peer_id": "",
            "local_username": "",
            "document": "",
        })
        .to_string()),
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
