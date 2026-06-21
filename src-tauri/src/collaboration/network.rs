//! WebSocket 网络通信模块
//!
//! 本模块负责多人协作中的网络通信，包括：
//! - 消息类型定义与序列化/反序列化
//! - WebSocket 服务器（主机端）启动与管理
//! - WebSocket 客户端（对等端）连接管理
//! - 心跳保活机制

use serde::{Deserialize, Serialize};

// ============================================================================
// 消息类型定义
// ============================================================================

/// 协作消息枚举，覆盖多人实时协作中的所有通信场景。
///
/// 所有变体均派生 `Debug`, `Clone`, `Serialize`, `Deserialize`，
/// 以便通过 JSON 在 WebSocket 连接上传输。
///
/// 注意：`OperationSync` 中的 `operation` 字段使用 `serde_json::Value` 类型，
/// 而非直接引用 `crate::collaboration::ot::Operation`，
/// 这样可以在不引入 OT 模块依赖的情况下完成消息的序列化/反序列化，
/// 具体的 Operation 类型转换由 session 层负责。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum CollaborationMessage {
    /// 加入房间请求
    /// - `room_id`: 目标房间标识
    /// - `password`: 房间密码（用于验证）
    /// - `username`: 请求加入的用户名
    JoinRequest {
        room_id: String,
        password: String,
        username: String,
    },

    /// 加入房间响应（由主机发送给请求者）
    /// - `accepted`: 是否接受加入请求
    /// - `message`: 附加说明信息
    /// - `document`: 当前文档内容（仅在 accepted 为 true 时有效）
    /// - `peer_id`: 主机分配给该对等方的唯一标识
    JoinResponse {
        accepted: bool,
        message: String,
        document: String,
        peer_id: String,
    },

    /// 离开通知（广播给房间内所有成员）
    /// - `peer_id`: 离开者的对等方标识
    /// - `username`: 离开者的用户名
    LeaveNotification { peer_id: String, username: String },

    /// 操作同步（将对等方的编辑操作广播给其他成员）
    /// - `peer_id`: 操作发起方的对等方标识
    /// - `operation`: 编辑操作，以 `serde_json::Value` 存储，
    ///   由 session 层负责与 OT 模块的 Operation 类型进行互转
    OperationSync {
        peer_id: String,
        operation: serde_json::Value,
    },

    /// 光标同步（广播对等方的光标位置）
    /// - `peer_id`: 对等方标识
    /// - `username`: 用户名
    /// - `position`: 光标在文档中的偏移位置（字节索引）
    CursorSync {
        peer_id: String,
        username: String,
        position: usize,
    },

    /// 心跳消息（用于维持连接活跃性）
    /// - `peer_id`: 发送心跳的对等方标识
    Heartbeat { peer_id: String },

    /// 主机断开连接通知（广播给所有客户端）
    HostDisconnected,

    /// 图片同步开始（通知接收方准备接收图片分片）
    /// - `peer_id`: 发送方对等方标识
    /// - `file_name`: 图片文件名
    /// - `total_chunks`: 总的分片数量
    /// - `file_size`: 图片文件总大小（字节）
    ImageSyncStart {
        peer_id: String,
        file_name: String,
        total_chunks: u32,
        file_size: u64,
    },

    /// 图片同步分片（传输单个分片数据）
    /// - `peer_id`: 发送方对等方标识
    /// - `file_name`: 图片文件名
    /// - `chunk_index`: 当前分片索引（从 0 开始）
    /// - `total_chunks`: 总分片数量（用于接收方判断是否所有分片已到达）
    /// - `data_base64`: 分片数据，使用 Base64 编码
    ImageSyncChunk {
        peer_id: String,
        file_name: String,
        chunk_index: u32,
        total_chunks: u32,
        data_base64: String,
    },

    /// 图片同步完成（通知接收方所有分片已发送完毕）
    /// - `peer_id`: 发送方对等方标识
    /// - `file_name`: 图片文件名
    ImageSyncEnd { peer_id: String, file_name: String },

    /// 完整文档同步（将整个文档内容发送给对等方）
    /// - `document`: 完整文档内容字符串
    DocumentSync { document: String },

    /// 对等方列表更新（广播当前房间内所有在线成员）
    /// - `peers`: 当前在线的对等方信息列表
    PeerListUpdate { peers: Vec<PeerInfo> },

    /// 错误消息（用于传递错误信息）
    /// - `message`: 错误描述字符串
    Error { message: String },
}

// ============================================================================
// 对等方信息结构体
// ============================================================================

/// 对等方（Peer）信息，描述房间中一个在线成员的基本状态。
///
/// 该结构体在 `PeerListUpdate` 消息中使用，
/// 用于向所有成员广播当前房间内的在线用户列表及其光标位置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    /// 对等方的唯一标识符（由主机分配）
    pub peer_id: String,
    /// 用户名
    pub username: String,
    /// 光标在文档中的当前偏移位置（字节索引）
    pub cursor_position: usize,
    /// 是否为房间主机（只有主机自身为 true，其余成员为 false）
    pub is_host: bool,
}

// ============================================================================
// 序列化 / 反序列化函数
// ============================================================================

/// 将 `CollaborationMessage` 序列化为 JSON 字符串。
///
/// # 参数
/// - `msg`: 要序列化的消息引用
///
/// # 返回
/// - `Ok(String)`: 序列化成功，返回 JSON 字符串
/// - `Err(String)`: 序列化失败，返回错误描述
///
/// # 示例
/// ```
/// use crate::collaboration::network::{CollaborationMessage, serialize_message};
///
/// let msg = CollaborationMessage::Heartbeat {
///     peer_id: "peer-1".to_string(),
/// };
/// let json = serialize_message(&msg).unwrap();
/// assert!(json.contains("Heartbeat"));
/// ```
pub fn serialize_message(msg: &CollaborationMessage) -> Result<String, String> {
    serde_json::to_string(msg).map_err(|e| format!("消息序列化失败: {}", e))
}

/// 将 JSON 字符串反序列化为 `CollaborationMessage`。
///
/// 使用 `#[serde(tag = "type", content = "payload")]` 的内部标记格式，
/// 通过 `type` 字段区分消息类型，`payload` 字段承载具体数据。
///
/// # 参数
/// - `json`: 待反序列化的 JSON 字符串
///
/// # 返回
/// - `Ok(CollaborationMessage)`: 反序列化成功
/// - `Err(String)`: 反序列化失败，返回错误描述
///
/// # 示例
/// ```
/// use crate::collaboration::network::{CollaborationMessage, deserialize_message};
///
/// let json = r#"{"type":"Heartbeat","payload":{"peer_id":"peer-1"}}"#;
/// let msg = deserialize_message(json).unwrap();
/// match msg {
///     CollaborationMessage::Heartbeat { peer_id } => assert_eq!(peer_id, "peer-1"),
///     _ => panic!("意外的消息类型"),
/// }
/// ```
pub fn deserialize_message(json: &str) -> Result<CollaborationMessage, String> {
    serde_json::from_str(json).map_err(|e| format!("消息反序列化失败: {}", e))
}

/// 将二进制数据直接返回（不做任何编码转换）。
///
/// 此函数用于处理需要以二进制形式传输的数据（如文件分片），
/// 直接包装为 `Vec<u8>` 以便通过 WebSocket 的二进制帧发送。
///
/// # 参数
/// - `data`: 原始二进制数据切片
///
/// # 返回
/// 包含原始数据副本的 `Vec<u8>`
pub fn serialize_binary(data: &[u8]) -> Vec<u8> {
    data.to_vec()
}

// ============================================================================
// 心跳检测
// ============================================================================

/// 检测心跳是否超时。
///
/// 通过比较上次心跳时间与当前时间，判断对等方是否已断开连接。
/// 若超过指定的超时秒数仍未收到心跳，则认为连接已丢失。
///
/// # 参数
/// - `last_heartbeat`: 上次收到心跳的 `Instant` 时间点
/// - `timeout_secs`: 心跳超时阈值（秒）
///
/// # 返回
/// - `true`: 心跳超时，对等方可能已断开
/// - `false`: 心跳未超时，连接仍有效
///
/// # 示例
/// ```
/// use std::time::{Duration, Instant};
/// use crate::collaboration::network::is_heartbeat_timeout;
///
/// let last = Instant::now() - Duration::from_secs(31);
/// assert!(is_heartbeat_timeout(last, 30));
/// ```
pub fn is_heartbeat_timeout(last_heartbeat: std::time::Instant, timeout_secs: u64) -> bool {
    last_heartbeat.elapsed().as_secs() >= timeout_secs
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    // --- 测试辅助函数 ---

    /// 辅助函数：验证消息经过序列化再反序列化后与原始消息一致
    fn assert_roundtrip(msg: &CollaborationMessage) {
        let json = serialize_message(msg).expect("序列化应成功");
        let deserialized = deserialize_message(&json).expect("反序列化应成功");

        // 重新序列化后比较 JSON 字符串（因为 Value 类型无法直接 PartialEq 比较）
        let json2 = serialize_message(&deserialized).expect("二次序列化应成功");
        assert_eq!(json, json2, "消息往返序列化后 JSON 应一致");
    }

    // ========================================================================
    // 序列化 / 反序列化测试
    // ========================================================================

    #[test]
    fn test_serialize_join_request() {
        let msg = CollaborationMessage::JoinRequest {
            room_id: "room-1".to_string(),
            password: "secret".to_string(),
            username: "Alice".to_string(),
        };
        let json = serialize_message(&msg).unwrap();
        assert!(json.contains("JoinRequest"));
        assert!(json.contains("room-1"));
        assert!(json.contains("Alice"));
    }

    #[test]
    fn test_deserialize_join_request() {
        let json = r#"{"type":"JoinRequest","payload":{"room_id":"room-1","password":"secret","username":"Alice"}}"#;
        let msg = deserialize_message(json).unwrap();
        match msg {
            CollaborationMessage::JoinRequest {
                room_id,
                password,
                username,
            } => {
                assert_eq!(room_id, "room-1");
                assert_eq!(password, "secret");
                assert_eq!(username, "Alice");
            }
            _ => panic!("应为 JoinRequest"),
        }
    }

    #[test]
    fn test_roundtrip_join_response() {
        let msg = CollaborationMessage::JoinResponse {
            accepted: true,
            message: "欢迎加入".to_string(),
            document: "# Hello\n\nWorld".to_string(),
            peer_id: "peer-abc".to_string(),
        };
        assert_roundtrip(&msg);
    }

    #[test]
    fn test_roundtrip_leave_notification() {
        let msg = CollaborationMessage::LeaveNotification {
            peer_id: "peer-1".to_string(),
            username: "Bob".to_string(),
        };
        assert_roundtrip(&msg);
    }

    #[test]
    fn test_roundtrip_operation_sync() {
        let msg = CollaborationMessage::OperationSync {
            peer_id: "peer-1".to_string(),
            operation: serde_json::json!({
                "op_type": "Insert",
                "position": 5,
                "text": "Hello"
            }),
        };
        assert_roundtrip(&msg);
    }

    #[test]
    fn test_deserialize_operation_sync() {
        let json = r#"{"type":"OperationSync","payload":{"peer_id":"peer-1","operation":{"op_type":"Insert","position":5,"text":"Hello"}}}"#;
        let msg = deserialize_message(json).unwrap();
        match msg {
            CollaborationMessage::OperationSync { peer_id, operation } => {
                assert_eq!(peer_id, "peer-1");
                assert_eq!(operation["op_type"], "Insert");
                assert_eq!(operation["position"], 5);
                assert_eq!(operation["text"], "Hello");
            }
            _ => panic!("应为 OperationSync"),
        }
    }

    #[test]
    fn test_roundtrip_cursor_sync() {
        let msg = CollaborationMessage::CursorSync {
            peer_id: "peer-1".to_string(),
            username: "Alice".to_string(),
            position: 42,
        };
        assert_roundtrip(&msg);
    }

    #[test]
    fn test_roundtrip_heartbeat() {
        let msg = CollaborationMessage::Heartbeat {
            peer_id: "peer-1".to_string(),
        };
        assert_roundtrip(&msg);
    }

    #[test]
    fn test_roundtrip_host_disconnected() {
        let msg = CollaborationMessage::HostDisconnected;
        assert_roundtrip(&msg);
    }

    #[test]
    fn test_deserialize_host_disconnected() {
        let json = r#"{"type":"HostDisconnected","payload":null}"#;
        let msg = deserialize_message(json).unwrap();
        match msg {
            CollaborationMessage::HostDisconnected => {}
            _ => panic!("应为 HostDisconnected"),
        }
    }

    #[test]
    fn test_roundtrip_image_sync_start() {
        let msg = CollaborationMessage::ImageSyncStart {
            peer_id: "peer-1".to_string(),
            file_name: "photo.png".to_string(),
            total_chunks: 10,
            file_size: 102400,
        };
        assert_roundtrip(&msg);
    }

    #[test]
    fn test_roundtrip_image_sync_chunk() {
        let msg = CollaborationMessage::ImageSyncChunk {
            peer_id: "peer-1".to_string(),
            file_name: "photo.png".to_string(),
            chunk_index: 3,
            total_chunks: 10,
            data_base64: "SGVsbG8gV29ybGQ=".to_string(),
        };
        assert_roundtrip(&msg);
    }

    #[test]
    fn test_roundtrip_image_sync_end() {
        let msg = CollaborationMessage::ImageSyncEnd {
            peer_id: "peer-1".to_string(),
            file_name: "photo.png".to_string(),
        };
        assert_roundtrip(&msg);
    }

    #[test]
    fn test_roundtrip_document_sync() {
        let msg = CollaborationMessage::DocumentSync {
            document: "# 文档标题\n\n正文内容".to_string(),
        };
        assert_roundtrip(&msg);
    }

    #[test]
    fn test_roundtrip_peer_list_update() {
        let peers = vec![
            PeerInfo {
                peer_id: "peer-1".to_string(),
                username: "Alice".to_string(),
                cursor_position: 10,
                is_host: true,
            },
            PeerInfo {
                peer_id: "peer-2".to_string(),
                username: "Bob".to_string(),
                cursor_position: 25,
                is_host: false,
            },
        ];
        let msg = CollaborationMessage::PeerListUpdate { peers };
        assert_roundtrip(&msg);
    }

    #[test]
    fn test_deserialize_peer_list_update() {
        let json = r#"{
            "type": "PeerListUpdate",
            "payload": {
                "peers": [
                    {"peer_id": "peer-1", "username": "Alice", "cursor_position": 10, "is_host": true},
                    {"peer_id": "peer-2", "username": "Bob", "cursor_position": 25, "is_host": false}
                ]
            }
        }"#;
        let msg = deserialize_message(json).unwrap();
        match msg {
            CollaborationMessage::PeerListUpdate { peers } => {
                assert_eq!(peers.len(), 2);
                assert_eq!(peers[0].peer_id, "peer-1");
                assert_eq!(peers[0].username, "Alice");
                assert_eq!(peers[0].cursor_position, 10);
                assert!(peers[0].is_host, "peer-1 应为房间主机");
                assert_eq!(peers[1].peer_id, "peer-2");
                assert_eq!(peers[1].username, "Bob");
                assert_eq!(peers[1].cursor_position, 25);
                assert!(!peers[1].is_host, "peer-2 不应为房间主机");
            }
            _ => panic!("应为 PeerListUpdate"),
        }
    }

    #[test]
    fn test_roundtrip_error() {
        let msg = CollaborationMessage::Error {
            message: "房间不存在".to_string(),
        };
        assert_roundtrip(&msg);
    }

    #[test]
    fn test_deserialize_error() {
        let json = r#"{"type":"Error","payload":{"message":"房间已满"}}"#;
        let msg = deserialize_message(json).unwrap();
        match msg {
            CollaborationMessage::Error { message } => {
                assert_eq!(message, "房间已满");
            }
            _ => panic!("应为 Error"),
        }
    }

    #[test]
    fn test_deserialize_invalid_json() {
        let result = deserialize_message("这不是合法的 JSON");
        assert!(result.is_err());
    }

    #[test]
    fn test_deserialize_unknown_type() {
        let json = r#"{"type":"UnknownType","payload":{"field":"value"}}"#;
        let result = deserialize_message(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_serialize_binary() {
        let data = b"Hello, binary world!";
        let result = serialize_binary(data);
        assert_eq!(result, data.to_vec());
    }

    #[test]
    fn test_serialize_binary_empty() {
        let data: &[u8] = &[];
        let result = serialize_binary(data);
        assert!(result.is_empty());
    }

    // ========================================================================
    // 心跳超时检测测试
    // ========================================================================

    #[test]
    fn test_heartbeat_not_timeout() {
        // 刚刚发送的心跳，不会超时
        let last_heartbeat = Instant::now();
        assert!(!is_heartbeat_timeout(last_heartbeat, 30));
    }

    #[test]
    fn test_heartbeat_timeout_exceeded() {
        // 模拟 31 秒前的心跳，超时阈值为 30 秒
        let last_heartbeat = Instant::now() - Duration::from_secs(31);
        assert!(is_heartbeat_timeout(last_heartbeat, 30));
    }

    #[test]
    fn test_heartbeat_exactly_at_threshold() {
        // 模拟恰好 30 秒前的心跳，应判定为超时（>=）
        let last_heartbeat = Instant::now() - Duration::from_secs(30);
        assert!(is_heartbeat_timeout(last_heartbeat, 30));
    }

    #[test]
    fn test_heartbeat_just_below_threshold() {
        // 模拟 29 秒前的心跳，不应超时
        let last_heartbeat = Instant::now() - Duration::from_secs(29);
        assert!(!is_heartbeat_timeout(last_heartbeat, 30));
    }

    #[test]
    fn test_heartbeat_zero_timeout() {
        // 超时阈值为 0 时，任何过去的时间都应判定为超时
        let last_heartbeat = Instant::now();
        assert!(is_heartbeat_timeout(last_heartbeat, 0));
    }
}
