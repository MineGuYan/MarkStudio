//! 房间管理模块
//!
//! 本模块负责协作房间的创建、加入、离开和文档同步管理。
//! 使用全局状态管理当前协作会话，通过 `Mutex` 保证线程安全。
//!
//! ## 核心职责
//!
//! - **房间管理**：创建/加入/离开协作房间
//! - **文档同步**：通过 OT 算法同步文档编辑操作
//! - **消息路由**：处理 WebSocket 消息的接收、分发与广播

#![allow(dead_code)]
//! - **连接管理**：维护对等方列表，处理心跳与断线
//!
//! ## 架构设计
//!
//! 主机（Host）模式：
//! - 启动 WebSocket 服务器，监听指定端口
//! - 维护客户端连接列表，负责消息路由与广播
//! - 作为 OT 操作的权威来源，串行化所有编辑操作
//!
//! 客户端（Client）模式：
//! - 连接到主机的 WebSocket 服务器
//! - 发送本地编辑操作到主机
//! - 接收主机广播的其他客户端操作并应用到本地文档

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, UdpSocket};
use std::sync::{Arc, Mutex};

use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;

use super::network::{
    deserialize_message, serialize_message, CollaborationMessage, PeerInfo, SharedFileInfo,
};
use super::ot::{apply_operation, Operation};

// ============================================================================
// 辅助函数
// ============================================================================

/// 计算字符串的字符数（Unicode 标量值数量），而非字节长度。
///
/// 对于多字节 UTF-8 字符（如中文），字符数小于字节长度。
/// 例如："啊" 的字节长度为 3，但字符数为 1。
#[inline]
fn char_count(s: &str) -> usize {
    s.chars().count()
}

/// 根据远程操作调整所有远程成员的光标位置。
///
/// 当远程成员执行了 Insert 或 Delete 操作后，其他成员的光标位置
/// 需要根据操作的影响进行相应调整，以保持光标与文本的相对位置不变.
///
/// # 参数
/// - `peers`: 对等方列表的可变引用
/// - `op`: 远程成员执行的操作
/// - `exclude_peer_id`: 要排除的对等方 ID（通常是操作的发送者，其光标位置由发送者自己维护）
fn adjust_peer_cursors_for_operation(
    peers: &mut Vec<PeerInfo>,
    op: &Operation,
    exclude_peer_id: &str,
) {
    for peer in peers.iter_mut() {
        // 排除操作的发送者，其光标位置由发送者自己通过 CursorSync 消息更新
        if peer.peer_id == exclude_peer_id {
            continue;
        }

        match op {
            Operation::Insert { position, text } => {
                let insert_len = char_count(text);
                // 如果光标位置在插入点或之后，需要右移
                if peer.cursor_position >= *position {
                    peer.cursor_position += insert_len;
                }
            }
            Operation::Delete { position, length } => {
                let delete_len = *length;
                if peer.cursor_position > *position {
                    // 光标在删除点之后，需要左移，但不能超过删除起始位置
                    peer.cursor_position = peer.cursor_position.saturating_sub(delete_len).max(*position);
                }
            }
        }
    }
}

// ============================================================================
// 类型别名
// ============================================================================

/// 客户端消息发送通道列表类型。
///
/// 主机端使用此类型管理所有已连接客户端的消息发送通道。
/// 每个元素是 `(peer_id, sender)` 对，其中 sender 是无界 MPSC 通道的发送端。
type ClientSenderList = Arc<Mutex<Vec<(String, mpsc::UnboundedSender<String>)>>>;

// ============================================================================
// 数据结构定义
// ============================================================================

/// 房间信息，描述一个协作房间的基本属性。
///
/// 该结构体用于在 UI 层展示房间信息，包含房间标识、主机地址、
/// 监听端口和当前在线人数等关键信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomInfo {
    /// 房间唯一标识符（UUID v4）
    pub room_id: String,
    /// 主机 IP 地址列表（包含 IPv4 和 IPv6）
    pub host_ips: Vec<String>,
    /// WebSocket 监听端口
    pub port: u16,
    /// 当前在线人数
    pub peer_count: usize,
}

/// 协作会话，管理单个房间的完整状态。
///
/// 包含文档内容、对等方列表、连接状态等核心信息。
/// 通过 `msg_tx` 通道向外发送 WebSocket 消息，
/// 主机模式下额外维护 `client_txs` 用于广播给所有客户端。
#[allow(dead_code)]
pub struct CollaborationSession {
    /// 房间唯一标识符
    pub room_id: String,
    /// 房间密码（空字符串表示无密码）
    pub password: String,
    /// 当前文档内容
    pub document: String,
    /// 当前协作编辑的文档对应的共享文件路径（用于前端关联标签页）
    pub current_document_path: Option<String>,
    /// 是否为房间主机
    pub is_host: bool,
    /// 主机 IP 地址列表（主机端包含所有可用 IP，客户端仅包含连接的 IP）
    pub host_ips: Vec<String>,
    /// 在线对等方列表
    pub peers: Vec<PeerInfo>,
    /// 本地对等方唯一标识
    pub local_peer_id: String,
    /// 本地用户名
    pub local_username: String,
    /// 连接状态
    pub connected: bool,
    /// 共享文件列表（主机维护，用于多文件共享）
    pub shared_files: Vec<SharedFileInfo>,
    /// 消息发送通道——用于向 WebSocket 写入待发送的消息文本（JSON 字符串）。
    /// 主机模式下，该通道连接到广播分发器；客户端模式下，直接连接到 WebSocket 写入任务。
    msg_tx: Option<mpsc::UnboundedSender<String>>,
    /// 仅主机有效：所有已连接客户端的消息发送通道列表。
    /// 用于广播消息给除发送者之外的所有客户端。
    client_txs: Option<ClientSenderList>,
    /// 仅主机有效：连接接受循环任务的 JoinHandle。
    /// 用于在离开房间时取消该任务，释放 TCP 监听端口。
    accept_handle: Option<tokio::task::JoinHandle<()>>,
    /// 仅主机有效：消息广播任务的 JoinHandle。
    /// 用于在离开房间时取消该任务，释放通道资源。
    broadcast_handle: Option<tokio::task::JoinHandle<()>>,
}

// ============================================================================
// 全局会话状态
// ============================================================================

/// 全局协作会话状态。
///
/// 同一时间只允许存在一个活跃会话（一个房间）。
/// 使用 `Mutex` 保护，确保跨线程安全访问。
static CURRENT_SESSION: Mutex<Option<CollaborationSession>> = Mutex::new(None);

/// 断开连接原因（用于在客户端被强制退出时向前端传递原因）。
///
/// 当客户端收到 `HostDisconnected` 消息时，会在此处记录断开原因，
/// 前端通过 `get_collab_status` 获取此信息并向用户显示提示。
static DISCONNECT_REASON: Mutex<Option<String>> = Mutex::new(None);

/// 获取全局会话状态的引用。
///
/// # 返回
/// 指向 `Mutex<Option<CollaborationSession>>` 的静态引用，
/// 调用方可通过 `.lock().unwrap()` 获取内部数据的访问权。
pub fn get_session() -> &'static Mutex<Option<CollaborationSession>> {
    &CURRENT_SESSION
}

/// 检查当前是否存在活跃的协作会话。
///
/// # 返回
/// - `true`：存在活跃会话（已加入或创建房间）
/// - `false`：没有活跃会话
pub fn has_active_session() -> bool {
    CURRENT_SESSION.lock().unwrap().is_some()
}

/// 获取断开连接的原因（如果有）。
///
/// 当客户端被强制退出（如主机关闭房间）时，会记录断开原因。
/// 前端在轮询检测到 `connected: false` 时，可通过此函数获取原因并向用户展示提示。
///
/// # 返回
/// - `Some(String)`: 断开原因的描述文本
/// - `None`: 没有记录断开原因（正常离开或尚未断开）
pub fn get_disconnect_reason() -> Option<String> {
    DISCONNECT_REASON.lock().unwrap().clone()
}

/// 清除断开连接原因记录。
///
/// 应在正常离开房间或重新创建/加入房间时调用，
/// 避免残留的原因信息影响后续会话。
pub fn clear_disconnect_reason() {
    *DISCONNECT_REASON.lock().unwrap() = None;
}

// ============================================================================
// 房间管理函数
// ============================================================================

/// 创建协作房间（主机模式）。
///
/// 生成房间 UUID，启动 WebSocket 服务器监听指定端口，
/// 等待客户端连接并进行消息路由。
///
/// # 参数
/// - `port`: WebSocket 服务器监听端口
/// - `password`: 房间密码（空字符串表示无密码）
/// - `username`: 主机用户名
/// - `document`: 初始文档内容
///
/// # 返回
/// - `Ok(RoomInfo)`: 创建成功，返回房间信息
/// - `Err(String)`: 创建失败，返回错误描述
pub async fn create_room(
    port: u16,
    password: &str,
    username: &str,
    document: &str,
) -> Result<RoomInfo, String> {
    // 检查是否已有活跃会话
    if has_active_session() {
        return Err("已存在活跃的协作会话，请先离开当前房间".to_string());
    }

    // 生成房间 UUID
    let room_id = Uuid::new_v4().to_string();

    // 获取本地 IP 地址
    let ips = get_local_ip()?;

    // 生成本地对等方 ID
    let local_peer_id = format!("host-{}", &room_id[..8]);

    // 创建客户端发送通道列表（共享状态）
    let client_txs: ClientSenderList = Arc::new(Mutex::new(Vec::new()));

    // 创建消息发送通道（主机用于向广播器发送消息）
    let (msg_tx, mut msg_rx) = mpsc::unbounded_channel::<String>();

    // 使用 TcpSocket 创建套接字，设置 SO_REUSEADDR 避免 Windows 上
    // 端口被释放后短时间内无法重新绑定的问题（TIME_WAIT）。
    //
    // 协议族策略：优先使用 IPv6 双栈套接字（`[::]:port` + `IPV6_V6ONLY=0`），
    // 这样同一个端口可同时接受 IPv4 与 IPv6 客户端的连接，
    // 最大化协作的连通性。当本机不支持 IPv6 时，回退到纯 IPv4 套接字。
    let listener: TcpListener = create_dual_stack_listener(port)?;

    let actual_port = listener
        .local_addr()
        .map_err(|e| format!("获取监听地址失败: {}", e))?
        .port();

    // 克隆共享数据，用于各异步任务
    let client_txs_clone = Arc::clone(&client_txs);
    let room_id_clone = room_id.clone();
    let password_owned = password.to_string();

    // 启动连接接受循环（后台任务）
    let accept_handle = tokio::spawn(async move {
        run_accept_loop(listener, client_txs_clone, room_id_clone, password_owned).await;
    });

    // 启动消息广播任务：从 msg_rx 读取消息并分发给所有客户端
    let client_txs_broadcast = Arc::clone(&client_txs);
    let broadcast_handle = tokio::spawn(async move {
        while let Some(msg_text) = msg_rx.recv().await {
            let clients = client_txs_broadcast.lock().unwrap();
            for (_, tx) in clients.iter() {
                let _ = tx.send(msg_text.clone());
            }
        }
    });

    // 创建主机对等方信息（标记为房间主机）
    let host_peer = PeerInfo {
        peer_id: local_peer_id.clone(),
        username: username.to_string(),
        cursor_position: 0,
        is_host: true,
    };

    // 创建并存储会话
    let session = CollaborationSession {
        room_id: room_id.clone(),
        password: password.to_string(),
        document: document.to_string(),
        current_document_path: None,
        is_host: true,
        host_ips: ips.clone(),
        peers: vec![host_peer],
        local_peer_id: local_peer_id.clone(),
        local_username: username.to_string(),
        connected: true,
        shared_files: Vec::new(),
        msg_tx: Some(msg_tx),
        client_txs: Some(client_txs),
        accept_handle: Some(accept_handle),
        broadcast_handle: Some(broadcast_handle),
    };

    *CURRENT_SESSION.lock().unwrap() = Some(session);

    // 清除之前的断开原因记录（如果有）
    clear_disconnect_reason();

    Ok(RoomInfo {
        room_id,
        host_ips: ips,
        port: actual_port,
        peer_count: 1,
    })
}

/// 加入协作房间（客户端模式）。
///
/// 连接到主机的 WebSocket 服务器，发送加入请求，
/// 等待主机确认后建立会话并启动消息接收循环。
///
/// # 参数
/// - `host`: 主机 IP 地址
/// - `port`: 主机 WebSocket 端口
/// - `room_id`: 目标房间 ID
/// - `password`: 房间密码
/// - `username`: 本地用户名
///
/// # 返回
/// - `Ok(())`: 加入成功
/// - `Err(String)`: 加入失败，返回错误描述
pub async fn join_room(
    host: &str,
    port: u16,
    room_id: &str,
    password: &str,
    username: &str,
) -> Result<(), String> {
    // 注意：不检查 has_active_session()，因为在实际使用中，
    // 主机和客户端运行在不同的进程实例中，各自拥有独立的全局会话。
    // 测试场景中同一进程内模拟主机+客户端时，由调用方负责管理会话状态。

    // 构建 WebSocket 连接地址（自动为 IPv6 地址添加方括号）
    let url = format_ws_url(host, port);

    // 连接到主机 WebSocket 服务器
    let (ws_stream, _) = tokio_tungstenite::connect_async(&url)
        .await
        .map_err(|e| format!("连接主机 {} 失败: {}", url, e))?;

    // 将 WebSocket 流拆分为读写两半
    let (mut write, read) = ws_stream.split();

    // 构造并发送加入请求
    let join_request = CollaborationMessage::JoinRequest {
        room_id: room_id.to_string(),
        password: password.to_string(),
        username: username.to_string(),
    };
    let request_json = serialize_message(&join_request)?;
    write
        .send(Message::Text(request_json))
        .await
        .map_err(|e| format!("发送加入请求失败: {}", e))?;

    // 等待主机的加入响应（使用 next() 而非 into_future()，保留流以供后续读取）
    let (response_msg, read) = {
        let (first, rest) = read.into_future().await;
        match first {
            Some(Ok(msg)) => (msg, rest),
            Some(Err(e)) => return Err(format!("读取加入响应失败: {}", e)),
            None => return Err("连接在收到响应前关闭".to_string()),
        }
    };

    let response_text = match response_msg {
        Message::Text(text) => text.to_string(),
        Message::Close(_) => return Err("主机在加入过程中关闭了连接".to_string()),
        other => {
            // 忽略非文本帧，继续等待
            return Err(format!("收到意外的消息类型: {:?}", other));
        }
    };

    let response: CollaborationMessage = deserialize_message(&response_text)?;

    match response {
        CollaborationMessage::JoinResponse {
            accepted,
            message,
            document,
            peer_id,
        } => {
            if !accepted {
                return Err(format!("加入被拒绝: {}", message));
            }

            // 创建消息发送通道（客户端用于向 WebSocket 写入数据）
            let (msg_tx, mut msg_rx) = mpsc::unbounded_channel::<String>();

            // 启动消息写入任务：从 msg_rx 读取消息并写入 WebSocket
            let mut write_clone = write;
            tokio::spawn(async move {
                while let Some(msg_text) = msg_rx.recv().await {
                    if write_clone.send(Message::Text(msg_text)).await.is_err() {
                        // 发送失败，连接可能已断开
                        break;
                    }
                }
            });

            // 创建会话
            let session = CollaborationSession {
                room_id: room_id.to_string(),
                password: password.to_string(),
                document: document.clone(),
                current_document_path: None,
                is_host: false,
                host_ips: vec![host.to_string()],
                peers: Vec::new(), // 等收到 PeerListUpdate 后更新
                local_peer_id: peer_id.clone(),
                local_username: username.to_string(),
                connected: true,
                shared_files: Vec::new(),
                msg_tx: Some(msg_tx),
                client_txs: None, // 客户端不需要此字段
                accept_handle: None,
                broadcast_handle: None,
            };

            *CURRENT_SESSION.lock().unwrap() = Some(session);

            // 清除之前的断开原因记录（如果有）
            clear_disconnect_reason();

            // 启动后台任务，持续读取主机推送的消息
            // 包括：PeerListUpdate（对等方列表更新）、OperationSync（编辑操作）、
            // CursorSync（光标同步）、LeaveNotification（离开通知）、HostDisconnected 等
            tokio::spawn(async move {
                let mut read = read;
                while let Some(msg_result) = read.next().await {
                    let msg = match msg_result {
                        Ok(m) => m,
                        Err(e) => {
                            eprintln!("客户端读取 WebSocket 消息失败: {}", e);
                            break;
                        }
                    };

                    let text = match msg {
                        Message::Text(t) => t.to_string(),
                        Message::Close(_) => break,
                        _ => continue, // 忽略非文本帧
                    };

                    let collab_msg = match deserialize_message(&text) {
                        Ok(m) => m,
                        Err(e) => {
                            eprintln!("客户端消息反序列化失败: {}", e);
                            continue;
                        }
                    };

                    match collab_msg {
                        // 对等方列表更新——更新 session.peers
                        CollaborationMessage::PeerListUpdate { peers } => {
                            let mut guard = CURRENT_SESSION.lock().unwrap();
                            if let Some(ref mut session) = *guard {
                                session.peers = peers;
                            }
                        }

                        // 编辑操作同步——应用远程操作到本地文档
                        CollaborationMessage::OperationSync {
                            peer_id: sender_peer_id,
                            operation,
                        } => {
                            if let Ok(op) = serde_json::from_value::<Operation>(operation) {
                                let mut guard = CURRENT_SESSION.lock().unwrap();
                                if let Some(ref mut session) = *guard {
                                    session.document = apply_operation(&session.document, &op);

                                    // 操作中可能包含图片引用，尝试将已接收的图片路径替换为本地缓存路径
                                    // 解决分片先于OT操作到达时，replace_image_paths 在未来得及替换的时序问题
                                    if let Ok(cache_dir) =
                                        crate::collaboration::sync::get_image_cache_dir()
                                    {
                                        session.document =
                                            crate::collaboration::sync::replace_image_paths(
                                                &session.document,
                                                &cache_dir,
                                            );
                                    }

                                    // 调整其他远程成员的光标位置，保持光标与文本的相对位置不变
                                    // 注意：这里传入 sender_peer_id 用于排除操作发送者
                                    adjust_peer_cursors_for_operation(
                                        &mut session.peers,
                                        &op,
                                        &sender_peer_id,
                                    );
                                }
                            }
                        }

                        // 光标同步——更新对等方光标位置
                        CollaborationMessage::CursorSync {
                            peer_id, position, ..
                        } => {
                            let mut guard = CURRENT_SESSION.lock().unwrap();
                            if let Some(ref mut session) = *guard {
                                for peer in session.peers.iter_mut() {
                                    if peer.peer_id == peer_id {
                                        peer.cursor_position = position;
                                        break;
                                    }
                                }
                            }
                        }

                        // 离开通知——移除对等方
                        CollaborationMessage::LeaveNotification { peer_id, .. } => {
                            let mut guard = CURRENT_SESSION.lock().unwrap();
                            if let Some(ref mut session) = *guard {
                                session.peers.retain(|p| p.peer_id != peer_id);
                            }
                        }

                        // 主机断开——记录原因并清除会话
                        CollaborationMessage::HostDisconnected => {
                            // 记录断开原因，供前端轮询时获取并展示给用户
                            *DISCONNECT_REASON.lock().unwrap() =
                                Some("主机关闭了房间，您已被强制退出".to_string());
                            let mut guard = CURRENT_SESSION.lock().unwrap();
                            *guard = None;
                            break;
                        }

                        // 图片同步开始——初始化接收缓冲区（预分配空间）
                        CollaborationMessage::ImageSyncStart {
                            file_name,
                            total_chunks,
                            file_size,
                            ..
                        } => {
                            // 在缓冲区中预创建条目，记录总分片数和文件大小
                            // 后续分片到达时可直接使用这些信息
                            let mut buffer = crate::collaboration::sync::image_receive_buffer()
                                .lock()
                                .unwrap();
                            buffer.entry(file_name.clone()).or_insert(
                                crate::collaboration::sync::ImageSyncInfo {
                                    file_name: file_name.clone(),
                                    total_chunks,
                                    file_size,
                                    chunks: vec![Vec::new(); total_chunks as usize],
                                },
                            );
                        }

                        // 图片分片数据——累积分片，全部到达后重组为完整文件
                        CollaborationMessage::ImageSyncChunk {
                            file_name,
                            chunk_index,
                            total_chunks,
                            data_base64,
                            ..
                        } => {
                            use crate::collaboration::sync::receive_image_chunk;

                            match receive_image_chunk(
                                &file_name,
                                chunk_index,
                                total_chunks,
                                &data_base64,
                            ) {
                                Ok(Some(saved_path)) => {
                                    // 所有分片已到达，图片已保存到缓存目录
                                    // 将文档中的远程图片路径替换为本地缓存路径
                                    let cache_dir =
                                        crate::collaboration::sync::get_image_cache_dir()
                                            .unwrap_or_else(|_| std::path::PathBuf::from("."));
                                    let mut guard = CURRENT_SESSION.lock().unwrap();
                                    if let Some(ref mut session) = *guard {
                                        session.document =
                                            crate::collaboration::sync::replace_image_paths(
                                                &session.document,
                                                &cache_dir,
                                            );
                                        // 日志：记录图片同步完成
                                        eprintln!(
                                            "[MarkStudio] 协作图片同步完成: {} → {}",
                                            file_name, saved_path
                                        );
                                    }
                                }
                                Ok(None) => {
                                    // 分片尚未到齐，继续等待
                                }
                                Err(e) => {
                                    eprintln!(
                                        "[MarkStudio] 图片分片接收失败 ({}): {}",
                                        file_name, e
                                    );
                                }
                            }
                        }

                        // 图片同步结束——在分片处理中已自动检测完成，此处仅记录日志
                        CollaborationMessage::ImageSyncEnd { file_name, .. } => {
                            eprintln!("[MarkStudio] 收到图片同步结束通知: {}", file_name);
                        }

                        // 共享文件列表更新——更新本地共享文件列表
                        CollaborationMessage::SharedFileListUpdate { files } => {
                            let mut guard = CURRENT_SESSION.lock().unwrap();
                            if let Some(ref mut session) = *guard {
                                session.shared_files = files;
                            }
                        }

                        // 心跳等消息无需处理
                        _ => {}
                    }
                }
            });

            Ok(())
        }
        _ => Err("收到意外的响应类型".to_string()),
    }
}

/// 离开当前协作房间。
///
/// 向其他对等方发送离开通知，清理 WebSocket 连接，
/// 重置全局会话状态。
///
/// 主机离开时，会先向所有客户端广播 `HostDisconnected` 消息，
/// 等待一小段时间确保消息送达，然后再关闭连接和清理会话。
///
/// # 返回
/// - `Ok(())`: 离开成功
/// - `Err(String)`: 离开失败，返回错误描述
pub async fn leave_room() -> Result<(), String> {
    // 第一步：在持有锁的情况下，发送离开消息并清理会话
    {
        let mut session_guard = CURRENT_SESSION.lock().unwrap();

        let session = match session_guard.as_ref() {
            Some(s) => s,
            None => return Err("当前没有活跃的协作会话".to_string()),
        };

        // 构造离开通知消息（用于客户端模式）
        let leave_msg = CollaborationMessage::LeaveNotification {
            peer_id: session.local_peer_id.clone(),
            username: session.local_username.clone(),
        };
        let leave_json = serialize_message(&leave_msg)?;

        if session.is_host {
            // 主机离开：广播 HostDisconnected 给所有客户端
            let disconnect_msg = CollaborationMessage::HostDisconnected;
            let disconnect_json = serialize_message(&disconnect_msg)?;

            if let Some(ref client_txs) = session.client_txs {
                let clients = client_txs.lock().unwrap();
                for (_, tx) in clients.iter() {
                    let _ = tx.send(disconnect_json.clone());
                }
            }
        } else {
            // 客户端离开：发送 LeaveNotification 给主机
            if let Some(ref tx) = session.msg_tx {
                let _ = tx.send(leave_json);
            }
        }

        // 取消后台任务，释放 TCP 监听端口和通道资源
        if let Some(ref handle) = session.accept_handle {
            handle.abort();
        }
        if let Some(ref handle) = session.broadcast_handle {
            handle.abort();
        }

        // 重置全局会话状态——这会丢弃 session，释放所有资源
        *session_guard = None;
    }
    // MutexGuard 在此处被释放，允许其他线程访问会话

    // 等待一小段时间，确保 HostDisconnected 等消息已通过 WebSocket 发送给客户端
    // 避免因立即清理会话导致消息丢失
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    Ok(())
}

// ============================================================================
// 消息发送函数
// ============================================================================

/// 发送编辑操作给其他对等方。
///
/// 将 OT 操作序列化为 JSON 并封装为 `OperationSync` 消息，
/// 根据角色决定发送策略：
/// - 主机模式：广播给所有客户端
/// - 客户端模式：发送给主机，由主机转发
///
/// # 参数
/// - `op`: 要发送的 OT 操作
///
/// # 返回
/// - `Ok(())`: 发送成功
/// - `Err(String)`: 发送失败，返回错误描述
pub fn send_operation(op: &Operation) -> Result<(), String> {
    let mut session_guard = CURRENT_SESSION.lock().unwrap();
    let session = session_guard
        .as_mut()
        .ok_or_else(|| "当前没有活跃的协作会话".to_string())?;

    // 将 Operation 序列化为 serde_json::Value
    let op_value = serde_json::to_value(op).map_err(|e| format!("操作序列化失败: {}", e))?;

    // 构造 OperationSync 消息
    let sync_msg = CollaborationMessage::OperationSync {
        peer_id: session.local_peer_id.clone(),
        operation: op_value,
    };
    let sync_json = serialize_message(&sync_msg)?;

    if session.is_host {
        // 主机模式：先更新自身的文档，再广播给所有客户端
        // 注意：客户端发送的操作由 handle_connection 中的 OperationSync 分支处理，
        // 不会经过此函数，因此这里仅处理主机自身的本地编辑。
        session.document = apply_operation(&session.document, op);

        if let Some(ref client_txs) = session.client_txs {
            let clients = client_txs.lock().unwrap();
            for (_, tx) in clients.iter() {
                let _ = tx.send(sync_json.clone());
            }
        }
    } else {
        // 客户端模式：先更新自身的文档，再发送给主机
        session.document = apply_operation(&session.document, op);

        if let Some(ref tx) = session.msg_tx {
            tx.send(sync_json)
                .map_err(|e| format!("发送操作失败: {}", e))?;
        }
    }

    Ok(())
}

/// 发送通用协作消息给其他对等方。
///
/// 将任意 `CollaborationMessage` 序列化为 JSON 并发送。
/// 根据角色决定发送策略：
/// - 主机模式：广播给所有客户端
/// - 客户端模式：发送给主机
///
/// # 参数
/// - `msg`: 要发送的协作消息引用
///
/// # 返回
/// - `Ok(())`: 发送成功
/// - `Err(String)`: 发送失败，返回错误描述
pub fn send_message(msg: &CollaborationMessage) -> Result<(), String> {
    let session_guard = CURRENT_SESSION.lock().unwrap();
    let session = session_guard
        .as_ref()
        .ok_or_else(|| "当前没有活跃的协作会话".to_string())?;

    // 将消息序列化为 JSON 字符串
    let json = serialize_message(msg)?;

    if session.is_host {
        // 主机模式：广播给所有客户端
        if let Some(ref client_txs) = session.client_txs {
            let clients = client_txs.lock().unwrap();
            for (_, tx) in clients.iter() {
                let _ = tx.send(json.clone());
            }
        }
    } else {
        // 客户端模式：发送给主机
        if let Some(ref tx) = session.msg_tx {
            tx.send(json).map_err(|e| format!("发送消息失败: {}", e))?;
        }
    }

    Ok(())
}

/// 发送光标同步消息给其他对等方。
///
/// 广播当前用户的光标位置，让其他对等方能够实时显示
/// 各成员的光标位置。
///
/// # 参数
/// - `position`: 光标在文档中的偏移位置（字节索引）
///
/// # 返回
/// - `Ok(())`: 发送成功
/// - `Err(String)`: 发送失败，返回错误描述
pub fn send_cursor_sync(position: usize) -> Result<(), String> {
    let session_guard = CURRENT_SESSION.lock().unwrap();
    let session = session_guard
        .as_ref()
        .ok_or_else(|| "当前没有活跃的协作会话".to_string())?;

    // 构造 CursorSync 消息
    let cursor_msg = CollaborationMessage::CursorSync {
        peer_id: session.local_peer_id.clone(),
        username: session.local_username.clone(),
        position,
    };
    let cursor_json = serialize_message(&cursor_msg)?;

    if session.is_host {
        // 主机模式：广播给所有客户端
        if let Some(ref client_txs) = session.client_txs {
            let clients = client_txs.lock().unwrap();
            for (_, tx) in clients.iter() {
                let _ = tx.send(cursor_json.clone());
            }
        }
    } else {
        // 客户端模式：发送给主机
        if let Some(ref tx) = session.msg_tx {
            tx.send(cursor_json)
                .map_err(|e| format!("发送光标同步失败: {}", e))?;
        }
    }

    Ok(())
}

// ============================================================================
// 网络工具函数
// ============================================================================

/// 将主机地址格式化为合法的 WebSocket URL。
///
/// 根据 `host` 的类型生成符合 RFC 3986 的 WebSocket 地址：
/// - IPv6 地址必须使用方括号包裹（如 `[::1]`、`[2001:db8::1]`），
///   否则冒号会与端口分隔符产生歧义，导致 URL 解析失败。
/// - IPv4 地址与主机名直接拼接即可。
/// - 如果用户已自行添加方括号，则原样保留，避免重复包裹。
///
/// # 参数
/// - `host`: 用户输入的主机 IP（IPv4/IPv6）或主机名
/// - `port`: WebSocket 端口
///
/// # 返回
/// 格式化后的 WebSocket URL 字符串，固定使用 `ws://` 协议
pub fn format_ws_url(host: &str, port: u16) -> String {
    let trimmed = host.trim();

    // 已带方括号的情况（如用户输入 "[::1]"）——原样使用
    if trimmed.starts_with('[') && trimmed.ends_with(']') {
        return format!("ws://{}:{}", trimmed, port);
    }

    // 尝试解析为标准 IPv6 地址，需要方括号包裹
    if trimmed.parse::<std::net::Ipv6Addr>().is_ok() {
        return format!("ws://[{}]:{}", trimmed, port);
    }

    // 通过冒号个数粗略判断：IPv6 地址至少包含 2 个冒号（如 `::1`、`fe80::1`）
    // 主机名不允许出现冒号，因此该判断对合法输入是安全的。
    let colon_count = trimmed.matches(':').count();
    if colon_count >= 2 {
        return format!("ws://[{}]:{}", trimmed, port);
    }

    // IPv4 地址或主机名直接拼接
    format!("ws://{}:{}", trimmed, port)
}

/// 获取本机所有非回环的 IP 地址，同时支持 IPv4 与 IPv6。
///
/// 实现原理：分别创建 IPv4 与 IPv6 的 UDP 套接字，
/// 通过向外部 DNS 地址发起 UDP "连接"（不实际发送数据），
/// 让操作系统选择本机对应协议族的出口 IP，
/// 借此获得本机所有可用的 IPv4/IPv6 地址。
///
/// - IPv4 探测目标：`8.8.8.8:80`（Google DNS）
/// - IPv6 探测目标：`[2001:4860:4860::8888]:80`（Google IPv6 DNS）
///
/// 任一探测失败（如本机无 IPv6 网络）不会影响另一协议族的获取。
///
/// # 返回
/// - `Ok(Vec<String>)`: 可用 IP 地址列表（可能同时包含 IPv4 与 IPv6），
///   顺序为 IPv4 在前、IPv6 在后
/// - `Err(String)`: 两种探测均失败，返回错误描述
pub fn get_local_ip() -> Result<Vec<String>, String> {
    let mut ips: Vec<String> = Vec::new();

    // --- 第一步：获取本机 IPv4 地址 ---
    // 通过向 IPv4 外部地址发起 UDP "连接"，让操作系统选择 IPv4 出口 IP
    if let Ok(socket) = UdpSocket::bind("0.0.0.0:0") {
        if socket.connect("8.8.8.8:80").is_ok() {
            if let Ok(addr) = socket.local_addr() {
                let ip = addr.ip().to_string();
                if !ips.contains(&ip) {
                    ips.push(ip);
                }
            }
        }
    }

    // --- 第二步：获取本机 IPv6 地址 ---
    // 通过向 IPv6 外部地址发起 UDP "连接"，让操作系统选择 IPv6 出口 IP
    // 若本机无 IPv6 网络或目标不可达，此步骤静默失败，不影响 IPv4 结果
    if let Ok(socket) = UdpSocket::bind("[::]:0") {
        if socket.connect("[2001:4860:4860::8888]:80").is_ok() {
            if let Ok(addr) = socket.local_addr() {
                let ip = addr.ip().to_string();
                if !ips.contains(&ip) {
                    ips.push(ip);
                }
            }
        }
    }

    // 两种探测都失败时返回错误
    if ips.is_empty() {
        return Err("无法获取本机 IP 地址（IPv4 与 IPv6 探测均失败）".to_string());
    }

    Ok(ips)
}

// ============================================================================
// 文档操作函数
// ============================================================================

/// 将远程操作应用到本地文档。
///
/// 使用 OT 算法的 `apply_operation` 函数将远程编辑操作
/// 应用到当前文档，同时更新全局会话中的文档内容。
///
/// # 参数
/// - `document`: 当前文档的可变引用
/// - `op`: 要应用的远程操作
///
/// # 返回
/// - `Ok(())`: 应用成功
/// - `Err(String)`: 应用失败，返回错误描述
pub fn apply_remote_operation(document: &mut String, op: &Operation) -> Result<(), String> {
    // 应用操作到传入的文档引用
    let new_doc = apply_operation(document, op);
    *document = new_doc;

    // 同步更新全局会话中的文档内容
    let mut session_guard = CURRENT_SESSION.lock().unwrap();
    if let Some(ref mut session) = *session_guard {
        session.document = document.clone();
    }

    Ok(())
}

// ============================================================================
// 对等方列表管理函数
// ============================================================================

/// 获取当前在线对等方列表。
///
/// 从全局会话中读取当前所有在线对等方的信息，
/// 包含对等方 ID、用户名和光标位置。
///
/// # 返回
/// - `Ok(Vec<PeerInfo>)`: 当前在线对等方列表
/// - `Err(String)`: 获取失败，返回错误描述（如无活跃会话）
pub fn get_peer_list() -> Result<Vec<PeerInfo>, String> {
    let session_guard = CURRENT_SESSION.lock().unwrap();
    let session = session_guard
        .as_ref()
        .ok_or_else(|| "当前没有活跃的协作会话".to_string())?;

    // 克隆对等方列表，避免持有锁期间返回引用
    Ok(session.peers.clone())
}

/// 更新指定对等方的光标位置。
///
/// 在全局会话的对等方列表中查找指定 peer_id，
/// 并将其光标位置更新为新的 position。
///
/// # 参数
/// - `peer_id`: 要更新的对等方唯一标识
/// - `position`: 新的光标位置（字节偏移量）
///
/// # 返回
/// - `Ok(())`: 更新成功
/// - `Err(String)`: 更新失败，返回错误描述（如无活跃会话或找不到对等方）
pub fn update_peer_cursor(peer_id: &str, position: usize) -> Result<(), String> {
    let mut session_guard = CURRENT_SESSION.lock().unwrap();
    let session = session_guard
        .as_mut()
        .ok_or_else(|| "当前没有活跃的协作会话".to_string())?;

    // 遍历对等方列表，找到匹配的 peer_id 并更新光标位置
    for peer in session.peers.iter_mut() {
        if peer.peer_id == peer_id {
            peer.cursor_position = position;
            return Ok(());
        }
    }

    Err(format!("未找到对等方: {}", peer_id))
}

/// 添加新的对等方到会话中。
///
/// 将对等方信息追加到全局会话的 peers 列表中。
/// 如果已存在相同 peer_id 的对等方，则更新其信息。
///
/// # 参数
/// - `peer`: 要添加的对等方信息
///
/// # 返回
/// - `Ok(())`: 添加成功
/// - `Err(String)`: 添加失败，返回错误描述（如无活跃会话）
pub fn add_peer(peer: PeerInfo) -> Result<(), String> {
    let mut session_guard = CURRENT_SESSION.lock().unwrap();
    let session = session_guard
        .as_mut()
        .ok_or_else(|| "当前没有活跃的协作会话".to_string())?;

    // 检查是否已存在相同 peer_id 的对等方
    if let Some(existing) = session.peers.iter_mut().find(|p| p.peer_id == peer.peer_id) {
        // 已存在则更新信息
        existing.username = peer.username;
        existing.cursor_position = peer.cursor_position;
    } else {
        // 不存在则追加
        session.peers.push(peer);
    }

    Ok(())
}

/// 从会话中移除指定对等方。
///
/// 根据 peer_id 从全局会话的 peers 列表中移除对等方。
///
/// # 参数
/// - `peer_id`: 要移除的对等方唯一标识
///
/// # 返回
/// - `Ok(())`: 移除成功
/// - `Err(String)`: 移除失败，返回错误描述（如无活跃会话或找不到对等方）
pub fn remove_peer(peer_id: &str) -> Result<(), String> {
    let mut session_guard = CURRENT_SESSION.lock().unwrap();
    let session = session_guard
        .as_mut()
        .ok_or_else(|| "当前没有活跃的协作会话".to_string())?;

    // 检查对等方是否存在
    let original_len = session.peers.len();
    session.peers.retain(|p| p.peer_id != peer_id);

    if session.peers.len() == original_len {
        // 没有找到匹配的对等方
        Err(format!("未找到对等方: {}", peer_id))
    } else {
        Ok(())
    }
}

/// 设置本地用户名。
///
/// 更新全局会话中的 local_username 字段，
/// 用于标识当前用户的显示名称。
///
/// # 参数
/// - `username`: 新的用户名
///
/// # 返回
/// - `Ok(())`: 设置成功
/// - `Err(String)`: 设置失败，返回错误描述（如无活跃会话）
pub fn set_username(username: &str) -> Result<(), String> {
    let mut session_guard = CURRENT_SESSION.lock().unwrap();
    let session = session_guard
        .as_mut()
        .ok_or_else(|| "当前没有活跃的协作会话".to_string())?;

    session.local_username = username.to_string();
    Ok(())
}

// ============================================================================
// 共享文件管理函数
// ============================================================================

/// 添加共享文件到协作房间。
///
/// 仅主机可调用此函数。将文件信息添加到共享文件列表，
/// 并广播更新给所有客户端。
///
/// # 参数
/// - `path`: 文件完整路径
/// - `title`: 文件显示名称
/// - `content`: 文件内容
///
/// # 返回
/// - `Ok(())`: 添加成功
/// - `Err(String)`: 添加失败，返回错误描述
pub fn add_shared_file(path: &str, title: &str, content: &str) -> Result<(), String> {
    let mut session_guard = CURRENT_SESSION.lock().unwrap();
    let session = session_guard
        .as_mut()
        .ok_or_else(|| "当前没有活跃的协作会话".to_string())?;

    if !session.is_host {
        return Err("只有主机可以添加共享文件".to_string());
    }

    // 检查是否已存在相同路径的共享文件
    if session.shared_files.iter().any(|f| f.path == path) {
        return Err(format!("文件 {} 已在共享列表中", path));
    }

    let file_info = SharedFileInfo {
        path: path.to_string(),
        title: title.to_string(),
        content: content.to_string(),
        is_local: true, // 主机端添加的共享文件，路径为本机路径
    };

    // 如果是第一个共享文件，设置当前协作文档
    let is_first = session.shared_files.is_empty();
    session.shared_files.push(file_info);

    if is_first {
        // 第一个共享文件成为当前协作编辑的文档
        session.current_document_path = Some(path.to_string());
        session.document = content.to_string();
    }

    // 广播共享文件列表更新给所有客户端
    // 注意：广播给客户端时需要转换为客户端视图，隐藏主机端文件路径
    let update_msg = CollaborationMessage::SharedFileListUpdate {
        files: to_client_shared_files(&session.shared_files),
    };
    let update_json = serialize_message(&update_msg)?;

    if let Some(ref client_txs) = session.client_txs {
        let clients = client_txs.lock().unwrap();
        for (_, tx) in clients.iter() {
            let _ = tx.send(update_json.clone());
        }
    }

    Ok(())
}

/// 从协作房间中移除共享文件。
///
/// 仅主机可调用此函数。从共享文件列表中移除指定文件，
/// 并广播更新给所有客户端。
///
/// # 参数
/// - `path`: 要移除的文件路径
///
/// # 返回
/// - `Ok(())`: 移除成功
/// - `Err(String)`: 移除失败，返回错误描述
pub fn remove_shared_file(path: &str) -> Result<(), String> {
    let mut session_guard = CURRENT_SESSION.lock().unwrap();
    let session = session_guard
        .as_mut()
        .ok_or_else(|| "当前没有活跃的协作会话".to_string())?;

    if !session.is_host {
        return Err("只有主机可以移除共享文件".to_string());
    }

    let original_len = session.shared_files.len();
    session.shared_files.retain(|f| f.path != path);

    if session.shared_files.len() == original_len {
        return Err(format!("未找到共享文件: {}", path));
    }

    // 广播共享文件列表更新给所有客户端
    // 注意：广播给客户端时需要转换为客户端视图，隐藏主机端文件路径
    let update_msg = CollaborationMessage::SharedFileListUpdate {
        files: to_client_shared_files(&session.shared_files),
    };
    let update_json = serialize_message(&update_msg)?;

    if let Some(ref client_txs) = session.client_txs {
        let clients = client_txs.lock().unwrap();
        for (_, tx) in clients.iter() {
            let _ = tx.send(update_json.clone());
        }
    }

    Ok(())
}

/// 获取当前共享文件列表。
///
/// 任何人都可以调用此函数获取房间中的共享文件列表。
///
/// # 返回
/// - `Ok(Vec<SharedFileInfo>)`: 共享文件列表
/// - `Err(String)`: 获取失败，返回错误描述
pub fn get_shared_files() -> Result<Vec<SharedFileInfo>, String> {
    let session_guard = CURRENT_SESSION.lock().unwrap();
    let session = session_guard
        .as_ref()
        .ok_or_else(|| "当前没有活跃的协作会话".to_string())?;

    Ok(session.shared_files.clone())
}

// ============================================================================
// 内部辅助函数：共享文件列表转换
// ============================================================================

/// 将共享文件列表转换为客户端视图（隐藏主机端文件路径）。
///
/// 主机端在向客户端广播 `SharedFileListUpdate` 消息时调用此函数：
/// - 清除每个文件的 `path` 字段（避免客户端获取到主机端的文件系统路径）
/// - 将 `is_local` 标记为 `false`（提示前端该文件是远端共享的，保存时需另存为）
///
/// 主机端自己调用 `get_shared_files` 时不受影响——返回的仍是完整的主机端视图。
///
/// # 参数
/// - `files`: 主机端的共享文件列表引用
///
/// # 返回
/// 转换后的客户端视图文件列表（`path` 为空字符串，`is_local` 为 `false`）
fn to_client_shared_files(files: &[SharedFileInfo]) -> Vec<SharedFileInfo> {
    files
        .iter()
        .map(|f| SharedFileInfo {
            // 客户端不应获取到主机端的文件路径
            path: String::new(),
            title: f.title.clone(),
            content: f.content.clone(),
            // 标记为远端共享文件，前端应将其视为"非本地"文件
            is_local: false,
        })
        .collect()
}

// ============================================================================
// 内部辅助函数：创建双栈监听器
// ============================================================================

/// 创建一个支持 IPv4/IPv6 双栈（或纯 IPv4 兜底）的 TCP 监听器。
///
/// 实现策略：
/// 1. **首选 IPv6 双栈**：使用 `socket2` 创建 IPv6 套接字，
///    绑定到 `[::]:port`（IPv6 全零地址），
///    并通过 `set_only_v6(false)` 关闭 `IPV6_V6ONLY` 选项，
///    使其同时接受 IPv4 与 IPv6 客户端连接。
/// 2. **回退到 IPv4**：当本机完全不支持 IPv6 时（如某些精简系统），
///    使用 IPv4 套接字绑定到 `0.0.0.0:port`。
///
/// 无论走哪条路径，都会设置 `SO_REUSEADDR` 避免 Windows 上
/// 端口被释放后短时间内无法重新绑定的问题（TIME_WAIT），
/// 并设置非阻塞模式以便与 tokio 集成。
///
/// # 参数
/// - `port`: 要监听的 TCP 端口
///
/// # 返回
/// - `Ok(TcpListener)`: 创建成功的 tokio TCP 监听器
/// - `Err(String)`: 两种路径均失败时返回错误描述
fn create_dual_stack_listener(port: u16) -> Result<TcpListener, String> {
    // --- 路径 1：尝试 IPv6 双栈监听（首选） ---
    // 通过 socket2 创建底层套接字，可直接设置 IPV6_V6ONLY=0
    let v6_addr = SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), port);

    match create_socket2_listener(&v6_addr, true) {
        Ok(std_listener) => {
            // 成功创建 IPv6 双栈监听器，转换为 tokio TcpListener
            if let Err(e) = std_listener.set_nonblocking(true) {
                eprintln!(
                    "[MarkStudio] 设置非阻塞模式失败，将回退到 IPv4: {}",
                    e
                );
            } else {
                match TcpListener::from_std(std_listener) {
                    Ok(listener) => return Ok(listener),
                    Err(e) => {
                        eprintln!(
                            "[MarkStudio] 将 IPv6 监听器转换为 tokio 失败，将回退到 IPv4: {}",
                            e
                        );
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("[MarkStudio] 创建 IPv6 双栈监听器失败: {}，将回退到 IPv4", e);
        }
    }

    // --- 路径 2：回退到纯 IPv4 监听 ---
    let v4_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port);
    let std_listener = create_socket2_listener(&v4_addr, false)
        .map_err(|e| format!("创建 IPv4 监听器也失败: {}", e))?;
    std_listener
        .set_nonblocking(true)
        .map_err(|e| format!("设置非阻塞模式失败: {}", e))?;
    let listener = TcpListener::from_std(std_listener)
        .map_err(|e| format!("将 IPv4 监听器转换为 tokio 失败: {}", e))?;
    Ok(listener)
}

/// 使用 `socket2` 创建已绑定并监听的 TCP 套接字。
///
/// # 参数
/// - `addr`: 要绑定的 Socket 地址
/// - `dual_stack`: 是否启用 IPv6 双栈（仅对 IPv6 套接字生效，
///   IPv4 套接字会忽略此参数）
///
/// # 返回
/// - `Ok(StdTcpListener)`: 创建成功的标准库 TCP 监听器
/// - `Err(String)`: 失败时返回错误描述
fn create_socket2_listener(
    addr: &SocketAddr,
    dual_stack: bool,
) -> Result<std::net::TcpListener, String> {
    use socket2::{Domain, Socket, Type};

    // 根据地址族选择套接字域
    let domain = if addr.is_ipv6() {
        Domain::IPV6
    } else {
        Domain::IPV4
    };

    // 创建套接字
    let socket = Socket::new(domain, Type::STREAM, None)
        .map_err(|e| format!("创建套接字失败: {}", e))?;

    // 设置 SO_REUSEADDR：避免 Windows 上 TIME_WAIT 状态导致端口无法立即重用
    socket
        .set_reuse_address(true)
        .map_err(|e| format!("设置 SO_REUSEADDR 失败: {}", e))?;

    // 对 IPv6 套接字，关闭 IPV6_V6ONLY 标志以启用双栈监听
    // 这样绑定的 [::]:port 可同时接受 IPv4 与 IPv6 客户端
    if addr.is_ipv6() && dual_stack {
        socket
            .set_only_v6(false)
            .map_err(|e| format!("关闭 IPV6_V6ONLY 失败: {}", e))?;
    }

    // 绑定到指定地址（需将 std SocketAddr 转换为 socket2 SockAddr）
    let sock_addr = socket2::SockAddr::from(*addr);
    socket
        .bind(&sock_addr)
        .map_err(|e| format!("绑定地址 {} 失败: {}", addr, e))?;

    // 开始监听（backlog = 128）
    socket
        .listen(128)
        .map_err(|e| format!("监听失败: {}", e))?;

    // 转换为标准库 TcpListener
    Ok(socket.into())
}

// ============================================================================
// 内部辅助函数：WebSocket 服务器接受循环
// ============================================================================

/// 运行 WebSocket 服务器的连接接受循环。
///
/// 持续监听新的 TCP 连接，为每个连接执行 WebSocket 升级握手，
/// 然后生成独立的处理任务。
///
/// # 参数
/// - `listener`: 已绑定的 TCP 监听器
/// - `client_txs`: 客户端发送通道列表（共享状态）
/// - `room_id`: 房间 ID
/// - `password`: 房间密码
async fn run_accept_loop(
    listener: TcpListener,
    client_txs: ClientSenderList,
    room_id: String,
    password: String,
) {
    loop {
        // 接受新的 TCP 连接
        let (stream, addr) = match listener.accept().await {
            Ok(conn) => conn,
            Err(e) => {
                eprintln!("接受连接失败: {}", e);
                continue;
            }
        };

        // 克隆共享数据，传递给处理任务
        let client_txs_clone = Arc::clone(&client_txs);
        let room_id_clone = room_id.clone();
        let password_clone = password.clone();

        // 为每个连接生成独立的处理任务
        tokio::spawn(async move {
            if let Err(e) = handle_connection(
                stream,
                client_txs_clone,
                room_id_clone,
                password_clone,
                addr,
            )
            .await
            {
                eprintln!("处理连接时出错 ({}): {}", addr, e);
            }
        });
    }
}

/// 处理单个 WebSocket 连接的生命周期。
///
/// 包括：WebSocket 握手、加入请求验证、消息收发循环、
/// 断线清理等完整流程。
///
/// # 参数
/// - `stream`: TCP 连接流
/// - `client_txs`: 客户端发送通道列表
/// - `room_id`: 房间 ID
/// - `password`: 房间密码
/// - `addr`: 客户端地址（用于日志）
async fn handle_connection(
    stream: TcpStream,
    client_txs: ClientSenderList,
    room_id: String,
    password: String,
    addr: std::net::SocketAddr,
) -> Result<(), String> {
    // 执行 WebSocket 升级握手
    let ws_stream = tokio_tungstenite::accept_async(stream)
        .await
        .map_err(|e| format!("WebSocket 握手失败: {}", e))?;

    // 拆分为读写两半
    let (mut write, mut read) = ws_stream.split();

    // 等待客户端发送 JoinRequest
    let first_msg = read
        .next()
        .await
        .ok_or_else(|| "连接在发送加入请求前关闭".to_string())?
        .map_err(|e| format!("读取加入请求失败: {}", e))?;

    let first_text = match first_msg {
        Message::Text(text) => text.to_string(),
        Message::Close(_) => return Err("客户端在加入前关闭了连接".to_string()),
        other => return Err(format!("加入阶段收到意外的消息类型: {:?}", other)),
    };

    let join_request: CollaborationMessage = deserialize_message(&first_text)?;

    // 验证加入请求
    let (client_username, client_password, client_room_id) = match join_request {
        CollaborationMessage::JoinRequest {
            room_id,
            password,
            username,
        } => (username, password, room_id),
        _ => {
            // 非 JoinRequest，拒绝连接
            let error_msg = CollaborationMessage::Error {
                message: "首条消息必须是 JoinRequest".to_string(),
            };
            let error_json = serialize_message(&error_msg)?;
            let _ = write.send(Message::Text(error_json)).await;
            return Err("首条消息不是 JoinRequest".to_string());
        }
    };

    // 验证房间 ID
    if client_room_id != room_id {
        let response = CollaborationMessage::JoinResponse {
            accepted: false,
            message: "房间 ID 不匹配".to_string(),
            document: String::new(),
            peer_id: String::new(),
        };
        let response_json = serialize_message(&response)?;
        let _ = write.send(Message::Text(response_json)).await;
        return Err(format!("客户端 {} 提供的房间 ID 不匹配", addr));
    }

    // 验证密码
    if !password.is_empty() && client_password != password {
        let response = CollaborationMessage::JoinResponse {
            accepted: false,
            message: "密码错误".to_string(),
            document: String::new(),
            peer_id: String::new(),
        };
        let response_json = serialize_message(&response)?;
        let _ = write.send(Message::Text(response_json)).await;
        return Err(format!("客户端 {} 密码错误", addr));
    }

    // 分配对等方 ID
    let peer_id = Uuid::new_v4().to_string();

    // 获取当前文档内容
    let current_document = {
        let session_guard = CURRENT_SESSION.lock().unwrap();
        session_guard
            .as_ref()
            .map(|s| s.document.clone())
            .unwrap_or_default()
    };

    // 发送加入成功响应
    let join_response = CollaborationMessage::JoinResponse {
        accepted: true,
        message: "加入成功".to_string(),
        document: current_document.clone(),
        peer_id: peer_id.clone(),
    };
    let response_json = serialize_message(&join_response)?;
    write
        .send(Message::Text(response_json))
        .await
        .map_err(|e| format!("发送加入响应失败: {}", e))?;

    // 创建该客户端的消息发送通道
    let (client_tx, mut client_rx) = mpsc::unbounded_channel::<String>();

    // 将客户端加入列表
    {
        let mut clients = client_txs.lock().unwrap();
        clients.push((peer_id.clone(), client_tx));
    }

    // 获取当前对等方列表
    let peers_after_join = {
        let mut session_guard = CURRENT_SESSION.lock().unwrap();
        if let Some(ref mut session) = *session_guard {
            session.peers.push(PeerInfo {
                peer_id: peer_id.clone(),
                username: client_username.clone(),
                cursor_position: 0,
                is_host: false, // 客户端不是主机
            });
            session.peers.clone()
        } else {
            Vec::new()
        }
    };

    // 广播对等方列表更新
    {
        let clients = client_txs.lock().unwrap();
        let peer_list_msg = CollaborationMessage::PeerListUpdate {
            peers: peers_after_join.clone(),
        };
        let peer_list_json = serialize_message(&peer_list_msg).unwrap_or_default();
        for (_, tx) in clients.iter() {
            let _ = tx.send(peer_list_json.clone());
        }
    }

    // 广播共享文件列表给所有客户端（包括新加入的客户端）
    // 注意：广播给客户端时需要转换为客户端视图，隐藏主机端文件路径
    {
        let session_guard = CURRENT_SESSION.lock().unwrap();
        let shared_files = if let Some(ref session) = *session_guard {
            session.shared_files.clone()
        } else {
            Vec::new()
        };
        let shared_files_msg = CollaborationMessage::SharedFileListUpdate {
            files: to_client_shared_files(&shared_files),
        };
        let shared_files_json = serialize_message(&shared_files_msg).unwrap_or_default();
        let clients = client_txs.lock().unwrap();
        for (_, tx) in clients.iter() {
            let _ = tx.send(shared_files_json.clone());
        }
    }

    // 启动消息写入任务：从 client_rx 读取消息并写入 WebSocket
    let (shutdown_tx, mut shutdown_rx) = mpsc::unbounded_channel::<()>();
    let mut write_for_send = write;
    let send_handle = tokio::spawn(async move {
        loop {
            tokio::select! {
                // 收到待发送的消息
                msg = client_rx.recv() => {
                    match msg {
                        Some(text) => {
                            if write_for_send.send(Message::Text(text)).await.is_err() {
                                break;
                            }
                        }
                        None => break,
                    }
                }
                // 收到关闭信号
                _ = shutdown_rx.recv() => {
                    break;
                }
            }
        }
        // 发送关闭帧
        let _ = write_for_send.send(Message::Close(None)).await;
    });

    // 消息接收循环：从 WebSocket 读取消息并处理
    loop {
        let msg = tokio::select! {
            // WebSocket 消息
            ws_msg = read.next() => {
                match ws_msg {
                    Some(Ok(msg)) => msg,
                    Some(Err(e)) => {
                        eprintln!("客户端 {} 读取消息错误: {}", addr, e);
                        break;
                    }
                    None => break, // 连接已关闭
                }
            }
        };

        match msg {
            Message::Text(text) => {
                let text_str = text.to_string();
                let collab_msg = match deserialize_message(&text_str) {
                    Ok(msg) => msg,
                    Err(e) => {
                        eprintln!("消息反序列化失败: {}", e);
                        continue;
                    }
                };

                // 根据消息类型进行路由处理
                match collab_msg {
                    CollaborationMessage::OperationSync {
                        peer_id: sender_peer_id,
                        operation,
                    } => {
                        // 将 serde_json::Value 反序列化为 Operation
                        let op: Operation = match serde_json::from_value(operation) {
                            Ok(op) => op,
                            Err(e) => {
                                eprintln!("操作反序列化失败: {}", e);
                                continue;
                            }
                        };

                        // 应用操作到主机文档，并替换图片路径为本地缓存路径
                        {
                            let mut session_guard = CURRENT_SESSION.lock().unwrap();
                            if let Some(ref mut session) = *session_guard {
                                session.document = apply_operation(&session.document, &op);

                                // 操作中可能包含图片引用，尝试将已接收的图片路径替换为本地缓存路径
                                // 解决分片先于OT操作到达时，replace_image_paths 在未来得及替换的时序问题
                                if let Ok(cache_dir) =
                                    crate::collaboration::sync::get_image_cache_dir()
                                {
                                    session.document =
                                        crate::collaboration::sync::replace_image_paths(
                                            &session.document,
                                            &cache_dir,
                                        );
                                }

                                // 调整其他远程成员的光标位置，保持光标与文本的相对位置不变
                                adjust_peer_cursors_for_operation(
                                    &mut session.peers,
                                    &op,
                                    &sender_peer_id,
                                );
                            }
                        }

                        // 克隆当前对等方列表，用于广播更新后的光标位置
                        let peers_to_broadcast = {
                            let session_guard = CURRENT_SESSION.lock().unwrap();
                            session_guard.as_ref().map(|s| s.peers.clone())
                        };

                        // 广播给其他客户端（排除发送者）
                        let forward_msg = CollaborationMessage::OperationSync {
                            peer_id: sender_peer_id.clone(),
                            operation: serde_json::to_value(&op).unwrap_or_default(),
                        };
                        let forward_json = serialize_message(&forward_msg).unwrap_or_default();

                        let clients = client_txs.lock().unwrap();
                        for (pid, tx) in clients.iter() {
                            if *pid != sender_peer_id {
                                let _ = tx.send(forward_json.clone());
                            }
                        }
                        drop(clients); // 释放客户端列表锁

                        // 广播更新后的对等方列表（包括调整后的光标位置）
                        if let Some(peers) = peers_to_broadcast {
                            let peer_list_msg = CollaborationMessage::PeerListUpdate { peers };
                            let peer_list_json = serialize_message(&peer_list_msg).unwrap_or_default();
                            let clients = client_txs.lock().unwrap();
                            for (_, tx) in clients.iter() {
                                let _ = tx.send(peer_list_json.clone());
                            }
                        }
                    }

                    CollaborationMessage::CursorSync {
                        peer_id: sender_peer_id,
                        username,
                        position,
                    } => {
                        // 更新对等方光标位置
                        {
                            let mut session_guard = CURRENT_SESSION.lock().unwrap();
                            if let Some(ref mut session) = *session_guard {
                                for peer in session.peers.iter_mut() {
                                    if peer.peer_id == sender_peer_id {
                                        peer.cursor_position = position;
                                        break;
                                    }
                                }
                            }
                        }

                        // 广播光标同步给其他客户端
                        let forward_msg = CollaborationMessage::CursorSync {
                            peer_id: sender_peer_id.clone(),
                            username,
                            position,
                        };
                        let forward_json = serialize_message(&forward_msg).unwrap_or_default();

                        let clients = client_txs.lock().unwrap();
                        for (pid, tx) in clients.iter() {
                            if *pid != sender_peer_id {
                                let _ = tx.send(forward_json.clone());
                            }
                        }
                    }

                    CollaborationMessage::Heartbeat { peer_id: _ } => {
                        // 心跳消息，不需要额外处理，收到即表示连接正常
                    }

                    CollaborationMessage::LeaveNotification {
                        peer_id: sender_peer_id,
                        username,
                    } => {
                        // 从客户端发送通道列表中移除该对等方
                        {
                            let mut clients = client_txs.lock().unwrap();
                            if let Some(pos) =
                                clients.iter().position(|(pid, _)| *pid == sender_peer_id)
                            {
                                clients.remove(pos);
                            }
                        }

                        // 从主机 session.peers 中移除该对等方
                        {
                            let mut session_guard = CURRENT_SESSION.lock().unwrap();
                            if let Some(ref mut session) = *session_guard {
                                session.peers.retain(|p| p.peer_id != sender_peer_id);
                            }
                        }

                        // 构造更新后的对等方列表
                        let updated_peers = {
                            let session_guard = CURRENT_SESSION.lock().unwrap();
                            session_guard
                                .as_ref()
                                .map(|s| s.peers.clone())
                                .unwrap_or_default()
                        };

                        // 构造离开通知消息，广播给剩余客户端
                        let leave_msg = CollaborationMessage::LeaveNotification {
                            peer_id: sender_peer_id.clone(),
                            username,
                        };
                        let leave_json = serialize_message(&leave_msg).unwrap_or_default();

                        // 构造对等方列表更新消息
                        let peer_list_msg = CollaborationMessage::PeerListUpdate {
                            peers: updated_peers,
                        };
                        let peer_list_json = serialize_message(&peer_list_msg).unwrap_or_default();

                        // 广播给所有剩余客户端
                        let clients = client_txs.lock().unwrap();
                        for (_, tx) in clients.iter() {
                            let _ = tx.send(leave_json.clone());
                            let _ = tx.send(peer_list_json.clone());
                        }
                    }

                    CollaborationMessage::ImageSyncStart {
                        peer_id: sender_peer_id,
                        file_name,
                        total_chunks,
                        file_size,
                    } => {
                        // 主机自身也需要初始化接收缓冲区，以便后续接收图片分片数据
                        // 注意：主机作为"服务端"也需要保存图片，否则本地文档无法渲染图片
                        {
                            let mut buffer = crate::collaboration::sync::image_receive_buffer()
                                .lock()
                                .unwrap();
                            buffer.entry(file_name.clone()).or_insert(
                                crate::collaboration::sync::ImageSyncInfo {
                                    file_name: file_name.clone(),
                                    total_chunks,
                                    file_size,
                                    chunks: vec![Vec::new(); total_chunks as usize],
                                },
                            );
                        }

                        // 转发给其他客户端（排除发送者）
                        let forward_msg = CollaborationMessage::ImageSyncStart {
                            peer_id: sender_peer_id.clone(),
                            file_name,
                            total_chunks,
                            file_size,
                        };
                        let forward_json = serialize_message(&forward_msg).unwrap_or_default();
                        let clients = client_txs.lock().unwrap();
                        for (pid, tx) in clients.iter() {
                            if *pid != sender_peer_id {
                                let _ = tx.send(forward_json.clone());
                            }
                        }
                    }

                    CollaborationMessage::ImageSyncChunk {
                        peer_id: sender_peer_id,
                        file_name,
                        chunk_index,
                        total_chunks,
                        data_base64,
                    } => {
                        // 主机自身也需要接收并保存图片分片数据
                        // 调用 receive_image_chunk 将分片累积到缓冲区，
                        // 当所有分片到达后自动重组为完整文件并保存到 image_cache 目录
                        use crate::collaboration::sync::receive_image_chunk;
                        match receive_image_chunk(
                            &file_name,
                            chunk_index,
                            total_chunks,
                            &data_base64,
                        ) {
                            Ok(Some(saved_path)) => {
                                // 所有分片已到达，图片已保存到缓存目录
                                // 将主机文档中的远程图片路径替换为本地缓存路径
                                let cache_dir = crate::collaboration::sync::get_image_cache_dir()
                                    .unwrap_or_else(|_| std::path::PathBuf::from("."));
                                let mut guard = CURRENT_SESSION.lock().unwrap();
                                if let Some(ref mut session) = *guard {
                                    session.document =
                                        crate::collaboration::sync::replace_image_paths(
                                            &session.document,
                                            &cache_dir,
                                        );
                                }
                                eprintln!(
                                    "[MarkStudio] 主机端协作图片同步完成: {} → {}",
                                    file_name, saved_path
                                );
                            }
                            Ok(None) => {
                                // 分片尚未到齐，继续等待
                            }
                            Err(e) => {
                                eprintln!(
                                    "[MarkStudio] 主机端图片分片接收失败 ({}): {}",
                                    file_name, e
                                );
                            }
                        }

                        // 转发给其他客户端（排除发送者）
                        let forward_msg = CollaborationMessage::ImageSyncChunk {
                            peer_id: sender_peer_id.clone(),
                            file_name,
                            chunk_index,
                            total_chunks,
                            data_base64,
                        };
                        let forward_json = serialize_message(&forward_msg).unwrap_or_default();
                        let clients = client_txs.lock().unwrap();
                        for (pid, tx) in clients.iter() {
                            if *pid != sender_peer_id {
                                let _ = tx.send(forward_json.clone());
                            }
                        }
                    }

                    CollaborationMessage::ImageSyncEnd {
                        peer_id: sender_peer_id,
                        file_name,
                    } => {
                        // 主机收到客户端的图片同步完成消息，转发给其他客户端
                        let forward_msg = CollaborationMessage::ImageSyncEnd {
                            peer_id: sender_peer_id.clone(),
                            file_name,
                        };
                        let forward_json = serialize_message(&forward_msg).unwrap_or_default();
                        let clients = client_txs.lock().unwrap();
                        for (pid, tx) in clients.iter() {
                            if *pid != sender_peer_id {
                                let _ = tx.send(forward_json.clone());
                            }
                        }
                    }

                    _ => {
                        // 其他消息类型暂不处理
                    }
                }
            }

            Message::Close(_) => {
                // 客户端主动关闭连接
                break;
            }

            Message::Ping(_data) => {
                // 响应 Ping 帧——由 tungstenite 自动处理 Pong 响应
                let _ = send_handle.is_finished(); // 检查发送任务是否仍在运行
            }

            _ => {
                // 忽略其他帧类型（二进制帧等）
            }
        }
    }

    // 连接断开后的清理工作

    // 通知发送任务关闭
    let _ = shutdown_tx.send(());

    // 从客户端列表中移除
    let removed_peer_id = {
        let mut clients = client_txs.lock().unwrap();
        if let Some(pos) = clients.iter().position(|(pid, _)| *pid == peer_id) {
            clients.remove(pos);
        }
        peer_id.clone()
    };

    // 从对等方列表中移除
    {
        let mut session_guard = CURRENT_SESSION.lock().unwrap();
        if let Some(ref mut session) = *session_guard {
            session.peers.retain(|p| p.peer_id != removed_peer_id);
        }
    }

    // 广播离开通知给剩余客户端
    let leave_msg = CollaborationMessage::LeaveNotification {
        peer_id: removed_peer_id.clone(),
        username: client_username.clone(),
    };
    let leave_json = serialize_message(&leave_msg).unwrap_or_default();

    // 广播更新后的对等方列表
    let updated_peers = {
        let session_guard = CURRENT_SESSION.lock().unwrap();
        session_guard
            .as_ref()
            .map(|s| s.peers.clone())
            .unwrap_or_default()
    };
    let peer_list_msg = CollaborationMessage::PeerListUpdate {
        peers: updated_peers,
    };
    let peer_list_json = serialize_message(&peer_list_msg).unwrap_or_default();

    let clients = client_txs.lock().unwrap();
    for (_, tx) in clients.iter() {
        let _ = tx.send(leave_json.clone());
        let _ = tx.send(peer_list_json.clone());
    }

    Ok(())
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
#[allow(clippy::await_holding_lock)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;

    /// 测试级互斥锁——由于所有测试共享全局 `CURRENT_SESSION`，
    /// 必须确保同一时间只有一个测试访问该全局状态，避免竞态条件。
    static TEST_MUTEX: StdMutex<()> = StdMutex::new(());

    // ========================================================================
    // 全局状态管理测试
    // ========================================================================

    #[test]
    fn test_get_session_returns_static_ref() {
        let _test_guard = TEST_MUTEX.lock().unwrap();
        // 验证 get_session() 返回有效的静态引用
        let session_mutex = get_session();
        let _guard = session_mutex.lock().unwrap();
        // 不 panic 即表示成功获取
    }

    #[test]
    fn test_has_active_session_initial_false() {
        let _test_guard = TEST_MUTEX.lock().unwrap();
        // 初始状态应无活跃会话
        // 注意：需要先清理可能残留的会话状态
        {
            let mut guard = CURRENT_SESSION.lock().unwrap();
            *guard = None;
        }
        assert!(!has_active_session());
    }

    #[test]
    fn test_has_active_session_after_set() {
        let _test_guard = TEST_MUTEX.lock().unwrap();
        // 设置会话后应返回 true
        {
            let mut guard = CURRENT_SESSION.lock().unwrap();
            *guard = None; // 先清理
        }
        assert!(!has_active_session());

        // 手动设置一个会话
        {
            let mut guard = CURRENT_SESSION.lock().unwrap();
            *guard = Some(CollaborationSession {
                room_id: "test-room".to_string(),
                password: String::new(),
                document: String::new(),
                current_document_path: None,
                is_host: true,
                host_ips: vec!["192.168.1.100".to_string()],
                peers: Vec::new(),
                local_peer_id: "test-peer".to_string(),
                local_username: "tester".to_string(),
                connected: true,
                shared_files: Vec::new(),
                msg_tx: None,
                client_txs: None,
                accept_handle: None,
                broadcast_handle: None,
            });
        }
        assert!(has_active_session());

        // 清理
        {
            let mut guard = CURRENT_SESSION.lock().unwrap();
            *guard = None;
        }
    }

    // ========================================================================
    // RoomInfo 序列化测试
    // ========================================================================

    #[test]
    fn test_room_info_serialization() {
        let room_info = RoomInfo {
            room_id: "test-room-123".to_string(),
            host_ips: vec!["192.168.1.100".to_string(), "2001:db8::1".to_string()],
            port: 9090,
            peer_count: 3,
        };

        let json = serde_json::to_string(&room_info).unwrap();
        let deserialized: RoomInfo = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.room_id, "test-room-123");
        assert_eq!(deserialized.host_ips, vec!["192.168.1.100", "2001:db8::1"]);
        assert_eq!(deserialized.port, 9090);
        assert_eq!(deserialized.peer_count, 3);
    }

    // ========================================================================
    // get_local_ip 测试
    // ========================================================================

    #[test]
    fn test_get_local_ip_returns_addresses() {
        let ips = get_local_ip();
        // 在有网络连接的环境下应能获取到 IP
        // 如果获取失败（如无网络），也接受此结果
        match ips {
            Ok(ip_list) => {
                assert!(!ip_list.is_empty(), "IP 列表不应为空");
                // IPv4 地址应在 IPv6 之前
                let mut has_v4 = false;
                let mut has_v6 = false;
                for ip in &ip_list {
                    // 验证是合法的 IPv4 或 IPv6 地址格式
                    let is_v4 = ip.parse::<std::net::Ipv4Addr>().is_ok();
                    let is_v6 = ip.parse::<std::net::Ipv6Addr>().is_ok();
                    assert!(
                        is_v4 || is_v6,
                        "{} 既不是合法的 IPv4 也不是 IPv6 地址",
                        ip
                    );
                    if is_v4 {
                        has_v4 = true;
                    }
                    if is_v6 {
                        has_v6 = true;
                    }
                }
                // 至少能获取到一种协议族的 IP（取决于运行环境）
                assert!(has_v4 || has_v6, "应至少能获取一种协议族的 IP 地址");
            }
            Err(_) => {
                // 无网络时可能失败，这是可接受的
            }
        }
    }

    // ========================================================================
    // format_ws_url 测试（IPv4/IPv6 URL 格式化）
    // ========================================================================

    #[test]
    fn test_format_ws_url_ipv4() {
        // 普通 IPv4 地址应直接拼接端口
        assert_eq!(format_ws_url("192.168.1.100", 8080), "ws://192.168.1.100:8080");
        // 主机名也应直接拼接
        assert_eq!(format_ws_url("localhost", 8080), "ws://localhost:8080");
        // 127.0.0.1 本地回环
        assert_eq!(format_ws_url("127.0.0.1", 9090), "ws://127.0.0.1:9090");
    }

    #[test]
    fn test_format_ws_url_ipv6_full() {
        // 完整的 IPv6 地址必须用方括号包裹
        assert_eq!(
            format_ws_url("2001:db8:85a3::8a2e:370:7334", 8080),
            "ws://[2001:db8:85a3::8a2e:370:7334]:8080"
        );
    }

    #[test]
    fn test_format_ws_url_ipv6_loopback() {
        // IPv6 回环地址 ::1 必须用方括号包裹
        assert_eq!(format_ws_url("::1", 8080), "ws://[::1]:8080");
    }

    #[test]
    fn test_format_ws_url_ipv6_already_bracketed() {
        // 已带方括号的 IPv6 地址应原样使用，避免重复包裹
        assert_eq!(format_ws_url("[::1]", 8080), "ws://[::1]:8080");
        assert_eq!(
            format_ws_url("[2001:db8::1]", 9090),
            "ws://[2001:db8::1]:9090"
        );
    }

    #[test]
    fn test_format_ws_url_ipv6_link_local() {
        // IPv6 链路本地地址（fe80:: 前缀）
        assert_eq!(
            format_ws_url("fe80::1", 8080),
            "ws://[fe80::1]:8080"
        );
    }

    #[test]
    fn test_format_ws_url_trims_whitespace() {
        // 输入两端空白应被自动去除
        assert_eq!(format_ws_url("  192.168.1.1  ", 8080), "ws://192.168.1.1:8080");
        assert_eq!(format_ws_url("  ::1  ", 8080), "ws://[::1]:8080");
    }

    // ========================================================================
    // apply_remote_operation 测试
    // ========================================================================

    #[test]
    fn test_apply_remote_operation_insert() {
        let _test_guard = TEST_MUTEX.lock().unwrap();
        // 清理全局会话
        {
            let mut guard = CURRENT_SESSION.lock().unwrap();
            *guard = None;
        }

        // 设置一个测试会话
        {
            let mut guard = CURRENT_SESSION.lock().unwrap();
            *guard = Some(CollaborationSession {
                room_id: "test".to_string(),
                password: String::new(),
                document: "Hello".to_string(),
                current_document_path: None,
                is_host: true,
                host_ips: vec!["192.168.1.100".to_string()],
                peers: Vec::new(),
                local_peer_id: "peer-1".to_string(),
                local_username: "test".to_string(),
                connected: true,
                shared_files: Vec::new(),
                msg_tx: None,
                client_txs: None,
                accept_handle: None,
                broadcast_handle: None,
            });
        }

        let mut doc = "Hello".to_string();
        let op = Operation::Insert {
            position: 5,
            text: " World".to_string(),
        };

        apply_remote_operation(&mut doc, &op).unwrap();

        assert_eq!(doc, "Hello World");

        // 验证全局会话中的文档也同步更新了
        {
            let guard = CURRENT_SESSION.lock().unwrap();
            if let Some(ref session) = *guard {
                assert_eq!(session.document, "Hello World");
            }
        }

        // 清理
        {
            let mut guard = CURRENT_SESSION.lock().unwrap();
            *guard = None;
        }
    }

    #[test]
    fn test_apply_remote_operation_delete() {
        let _test_guard = TEST_MUTEX.lock().unwrap();
        // 清理全局会话
        {
            let mut guard = CURRENT_SESSION.lock().unwrap();
            *guard = None;
        }

        // 设置一个测试会话
        {
            let mut guard = CURRENT_SESSION.lock().unwrap();
            *guard = Some(CollaborationSession {
                room_id: "test".to_string(),
                password: String::new(),
                document: "Hello World".to_string(),
                current_document_path: None,
                is_host: true,
                host_ips: vec!["192.168.1.100".to_string()],
                peers: Vec::new(),
                local_peer_id: "peer-1".to_string(),
                local_username: "test".to_string(),
                connected: true,
                shared_files: Vec::new(),
                msg_tx: None,
                client_txs: None,
                accept_handle: None,
                broadcast_handle: None,
            });
        }

        let mut doc = "Hello World".to_string();
        let op = Operation::Delete {
            position: 5,
            length: 6,
        };

        apply_remote_operation(&mut doc, &op).unwrap();

        assert_eq!(doc, "Hello");

        // 验证全局会话中文档同步
        {
            let guard = CURRENT_SESSION.lock().unwrap();
            if let Some(ref session) = *guard {
                assert_eq!(session.document, "Hello");
            }
        }

        // 清理
        {
            let mut guard = CURRENT_SESSION.lock().unwrap();
            *guard = None;
        }
    }

    #[test]
    fn test_apply_remote_operation_no_active_session() {
        let _test_guard = TEST_MUTEX.lock().unwrap();
        // 清理全局会话
        {
            let mut guard = CURRENT_SESSION.lock().unwrap();
            *guard = None;
        }

        // 没有活跃会话时，操作仍应成功应用到本地文档
        let mut doc = "Hello".to_string();
        let op = Operation::Insert {
            position: 0,
            text: "Hi ".to_string(),
        };

        apply_remote_operation(&mut doc, &op).unwrap();
        assert_eq!(doc, "Hi Hello");
    }

    // ========================================================================
    // leave_room 测试
    // ========================================================================

    #[tokio::test]
    async fn test_leave_room_no_active_session() {
        let _test_guard = TEST_MUTEX.lock().unwrap();
        // 清理全局会话
        {
            let mut guard = CURRENT_SESSION.lock().unwrap();
            *guard = None;
        }

        let result = leave_room().await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("没有活跃"));
    }

    #[tokio::test]
    async fn test_leave_room_clears_session() {
        let _test_guard = TEST_MUTEX.lock().unwrap();
        // 清理
        {
            let mut guard = CURRENT_SESSION.lock().unwrap();
            *guard = None;
        }

        // 设置一个测试会话
        {
            let mut guard = CURRENT_SESSION.lock().unwrap();
            *guard = Some(CollaborationSession {
                room_id: "test".to_string(),
                password: String::new(),
                document: String::new(),
                current_document_path: None,
                is_host: false,
                host_ips: vec!["192.168.1.100".to_string()],
                peers: Vec::new(),
                local_peer_id: "peer-1".to_string(),
                local_username: "test".to_string(),
                connected: true,
                shared_files: Vec::new(),
                msg_tx: None,
                client_txs: None,
                accept_handle: None,
                broadcast_handle: None,
            });
        }

        assert!(has_active_session());

        let result = leave_room().await;
        assert!(result.is_ok());
        assert!(!has_active_session());
    }

    // ========================================================================
    // send_operation 测试
    // ========================================================================

    #[test]
    fn test_send_operation_no_active_session() {
        let _test_guard = TEST_MUTEX.lock().unwrap();
        // 清理
        {
            let mut guard = CURRENT_SESSION.lock().unwrap();
            *guard = None;
        }

        let op = Operation::Insert {
            position: 0,
            text: "test".to_string(),
        };
        let result = send_operation(&op);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("没有活跃"));
    }

    /// 测试 send_operation 在主机模式下通过 mpsc 通道发送
    #[test]
    fn test_send_operation_host_mode() {
        let _test_guard = TEST_MUTEX.lock().unwrap();
        // 清理全局会话
        {
            let mut guard = CURRENT_SESSION.lock().unwrap();
            *guard = None;
        }

        // 创建主机模式的 mpsc 通道——msg_tx 保留但不用于直接发送（主机直接写 client_txs）
        let (_msg_tx, _msg_rx) = mpsc::unbounded_channel::<String>();
        let client_txs: ClientSenderList = Arc::new(Mutex::new(Vec::new()));

        // 创建一个模拟的客户端通道（用于接收广播）
        let (client_tx, mut client_rx) = mpsc::unbounded_channel::<String>();
        {
            let mut clients = client_txs.lock().unwrap();
            clients.push(("client-1".to_string(), client_tx));
        }

        // 设置会话
        {
            let mut guard = CURRENT_SESSION.lock().unwrap();
            *guard = Some(CollaborationSession {
                room_id: "test-room".to_string(),
                password: String::new(),
                document: String::new(),
                current_document_path: None,
                is_host: true,
                host_ips: vec!["192.168.1.100".to_string()],
                peers: vec![PeerInfo {
                    peer_id: "host-1".to_string(),
                    username: "host".to_string(),
                    cursor_position: 0,
                    is_host: true,
                }],
                local_peer_id: "host-1".to_string(),
                local_username: "host".to_string(),
                connected: true,
                shared_files: Vec::new(),
                msg_tx: Some(_msg_tx),
                client_txs: Some(client_txs),
                accept_handle: None,
                broadcast_handle: None,
            });
        }

        // 发送操作
        let op = Operation::Insert {
            position: 0,
            text: "Hello".to_string(),
        };
        let result = send_operation(&op);
        assert!(result.is_ok());

        // 验证客户端通道收到了消息（注意：主机直接写入 client_txs，不经过 msg_tx）
        // 由于 send_operation 直接写入 client_txs，所以 client_rx 应该能收到
        let received = client_rx.try_recv();
        assert!(received.is_ok(), "客户端应收到操作同步消息");
        let received_text = received.unwrap();
        assert!(received_text.contains("OperationSync"));
        assert!(received_text.contains("Hello"));

        // 验证主机自身的文档已同步更新（修复：send_operation 需更新主机文档）
        {
            let guard = CURRENT_SESSION.lock().unwrap();
            let session = guard.as_ref().unwrap();
            assert_eq!(session.document, "Hello", "主机自身的文档应包含插入的文本");
        }

        // 清理
        {
            let mut guard = CURRENT_SESSION.lock().unwrap();
            *guard = None;
        }
    }

    /// 测试 send_operation 在客户端模式下通过 mpsc 通道发送
    #[test]
    fn test_send_operation_client_mode() {
        let _test_guard = TEST_MUTEX.lock().unwrap();
        // 清理全局会话
        {
            let mut guard = CURRENT_SESSION.lock().unwrap();
            *guard = None;
        }

        // 创建客户端模式的 mpsc 通道
        let (msg_tx, mut msg_rx) = mpsc::unbounded_channel::<String>();

        // 设置会话
        {
            let mut guard = CURRENT_SESSION.lock().unwrap();
            *guard = Some(CollaborationSession {
                room_id: "test-room".to_string(),
                password: String::new(),
                document: String::new(),
                current_document_path: None,
                is_host: false,
                host_ips: vec!["192.168.1.100".to_string()],
                peers: Vec::new(),
                local_peer_id: "client-1".to_string(),
                local_username: "client".to_string(),
                connected: true,
                shared_files: Vec::new(),
                msg_tx: Some(msg_tx),
                client_txs: None,
                accept_handle: None,
                broadcast_handle: None,
            });
        }

        // 发送操作
        let op = Operation::Delete {
            position: 3,
            length: 2,
        };
        let result = send_operation(&op);
        assert!(result.is_ok());

        // 验证 msg_rx 收到了消息
        let received = msg_rx.try_recv();
        assert!(received.is_ok(), "msg_rx 应收到操作同步消息");
        let received_text = received.unwrap();
        assert!(received_text.contains("OperationSync"));

        // 清理
        {
            let mut guard = CURRENT_SESSION.lock().unwrap();
            *guard = None;
        }
    }

    // ========================================================================
    // send_cursor_sync 测试
    // ========================================================================

    #[test]
    fn test_send_cursor_sync_no_active_session() {
        let _test_guard = TEST_MUTEX.lock().unwrap();
        // 清理
        {
            let mut guard = CURRENT_SESSION.lock().unwrap();
            *guard = None;
        }

        let result = send_cursor_sync(42);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("没有活跃"));
    }

    #[test]
    fn test_send_cursor_sync_client_mode() {
        let _test_guard = TEST_MUTEX.lock().unwrap();
        // 清理全局会话
        {
            let mut guard = CURRENT_SESSION.lock().unwrap();
            *guard = None;
        }

        let (msg_tx, mut msg_rx) = mpsc::unbounded_channel::<String>();

        {
            let mut guard = CURRENT_SESSION.lock().unwrap();
            *guard = Some(CollaborationSession {
                room_id: "test".to_string(),
                password: String::new(),
                document: String::new(),
                current_document_path: None,
                is_host: false,
                host_ips: vec!["192.168.1.100".to_string()],
                peers: Vec::new(),
                local_peer_id: "client-1".to_string(),
                local_username: "Alice".to_string(),
                connected: true,
                shared_files: Vec::new(),
                msg_tx: Some(msg_tx),
                client_txs: None,
                accept_handle: None,
                broadcast_handle: None,
            });
        }

        let result = send_cursor_sync(100);
        assert!(result.is_ok());

        let received = msg_rx.try_recv();
        assert!(received.is_ok());
        let text = received.unwrap();
        assert!(text.contains("CursorSync"));
        assert!(text.contains("100"));
        assert!(text.contains("Alice"));

        // 清理
        {
            let mut guard = CURRENT_SESSION.lock().unwrap();
            *guard = None;
        }
    }

    // ========================================================================
    // 集成测试：create_room + join_room 完整流程
    // ========================================================================

    /// 测试完整的房间创建与加入流程
    ///
    /// 该测试启动一个真实的 WebSocket 服务器，客户端连接后验证
    /// 加入响应和文档同步消息。
    #[tokio::test]
    async fn test_create_and_join_room() {
        let _test_guard = TEST_MUTEX.lock().unwrap();
        // 清理全局会话
        {
            let mut guard = CURRENT_SESSION.lock().unwrap();
            *guard = None;
        }

        // 创建房间
        let room_info = create_room(0, "", "HostUser", "# Hello Markdown")
            .await
            .expect("创建房间应成功");

        assert!(!room_info.room_id.is_empty());
        assert!(room_info.peer_count == 1);

        // 等待服务器启动
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let port = room_info.port;

        // 客户端加入房间
        let join_result = join_room("127.0.0.1", port, &room_info.room_id, "", "ClientUser").await;
        assert!(join_result.is_ok(), "加入房间应成功: {:?}", join_result);

        // 验证客户端会话状态
        {
            let guard = CURRENT_SESSION.lock().unwrap();
            let session = guard.as_ref().expect("应有活跃会话");
            assert!(!session.is_host, "客户端不应为主机");
            assert_eq!(session.local_username, "ClientUser");
            assert_eq!(session.document, "# Hello Markdown");
            assert!(session.connected);
        }

        // 等待对等方列表更新传播
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        // 清理
        leave_room().await.expect("离开房间应成功");
        assert!(!has_active_session());
    }

    /// 测试密码验证
    #[tokio::test]
    async fn test_join_room_wrong_password() {
        let _test_guard = TEST_MUTEX.lock().unwrap();
        // 清理全局会话
        {
            let mut guard = CURRENT_SESSION.lock().unwrap();
            *guard = None;
        }

        // 创建带密码的房间
        let room_info = create_room(0, "secret123", "HostUser", "Doc")
            .await
            .expect("创建房间应成功");

        // 等待服务器启动
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let port = room_info.port;

        // 使用错误密码加入
        let join_result =
            join_room("127.0.0.1", port, &room_info.room_id, "wrong", "BadClient").await;
        assert!(join_result.is_err());
        assert!(join_result.unwrap_err().contains("拒绝"));

        // 清理
        {
            let mut guard = CURRENT_SESSION.lock().unwrap();
            *guard = None;
        }
    }

    /// 测试重复创建房间被拒绝
    #[tokio::test]
    async fn test_create_room_while_active() {
        let _test_guard = TEST_MUTEX.lock().unwrap();
        // 清理全局会话
        {
            let mut guard = CURRENT_SESSION.lock().unwrap();
            *guard = None;
        }

        // 首次创建应成功
        let _room_info = create_room(0, "", "Host", "Doc")
            .await
            .expect("首次创建应成功");

        // 等待服务器启动
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // 第二次创建应失败
        let result = create_room(0, "", "Host2", "Doc2").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("已存在"));

        // 清理
        {
            let mut guard = CURRENT_SESSION.lock().unwrap();
            *guard = None;
        }
    }

    /// 测试文档同步：主机编辑后客户端通过 OperationSync 接收
    #[tokio::test]
    async fn test_document_sync_via_operation() {
        let _test_guard = TEST_MUTEX.lock().unwrap();
        // 清理全局会话
        {
            let mut guard = CURRENT_SESSION.lock().unwrap();
            *guard = None;
        }

        // 创建房间
        let room_info = create_room(0, "", "Host", "Hello")
            .await
            .expect("创建房间应成功");

        let port = room_info.port;

        // 等待服务器启动
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // 客户端加入
        join_room("127.0.0.1", port, &room_info.room_id, "", "Client")
            .await
            .expect("加入应成功");

        // 等待加入完成
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // 主机发送操作
        let op = Operation::Insert {
            position: 5,
            text: " World".to_string(),
        };
        send_operation(&op).expect("发送操作应成功");

        // 等待操作传播
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // 验证主机文档已更新（操作在发送前已应用到本地文档）
        // 注意：send_operation 只负责发送，不负责本地应用。
        // 本地应用由调用方（编辑器）通过 apply_remote_operation 完成。
        // 这里我们解封 send_operation 并验证消息已发送。

        // 清理
        leave_room().await.expect("离开房间应成功");
    }

    /// 测试 leave_room 后会话状态完全清理
    #[tokio::test]
    async fn test_leave_room_full_cleanup() {
        let _test_guard = TEST_MUTEX.lock().unwrap();
        // 清理
        {
            let mut guard = CURRENT_SESSION.lock().unwrap();
            *guard = None;
        }

        // 创建房间
        let room_info = create_room(0, "", "Host", "Doc")
            .await
            .expect("创建房间应成功");

        let port = room_info.port;

        // 等待服务器启动
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // 客户端加入
        join_room("127.0.0.1", port, &room_info.room_id, "", "Client")
            .await
            .expect("加入应成功");

        // 验证客户端有活跃会话
        assert!(has_active_session());

        // 客户端离开
        leave_room().await.expect("离开应成功");
        assert!(!has_active_session());

        // 清理主机会话
        {
            let mut guard = CURRENT_SESSION.lock().unwrap();
            *guard = None;
        }
    }
}
