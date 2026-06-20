//! 多人实时协作模块
//!
//! 本模块提供 Markdown 多人实时协作编辑功能，包含以下子模块：
//! - `ot`: Operational Transformation 算法，解决多人并发编辑冲突
//! - `network`: WebSocket 网络通信层，负责主机/客户端之间的消息传输
//! - `session`: 房间管理，处理房间的创建、加入、离开和文档同步
//! - `sync`: 图片等二进制文件的同步传输

pub mod network;
pub mod ot;
pub mod session;
pub mod sync;
