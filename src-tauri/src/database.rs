//! SQLite 数据库模块
//!
//! 本模块负责 SQLite 数据库的初始化、连接管理与数据持久化。
//! 使用 rusqlite 库操作 SQLite 数据库，通过 std::sync::OnceLock 实现单例连接，
//! 使用 Mutex 保证线程安全。
//!
//! 数据库文件存储在项目根目录下的 data/markstudio.db 中，
//! 包含以下两张表：
//! - settings：存储用户配置（键值对形式）
//! - recent_files：存储最近打开的文件列表

use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::Mutex;

/// 全局数据库连接单例
///
/// 使用 OnceLock 确保数据库连接只初始化一次，
/// 内部使用 Mutex 包装 Connection 以保证多线程安全访问。
static DB_CONNECTION: std::sync::OnceLock<Mutex<Connection>> = std::sync::OnceLock::new();

/// 获取数据库文件路径
///
/// 数据库文件位于项目根目录下的 data/markstudio.db。
/// 开发模式下 current_dir 为 src-tauri/，需要回退到项目根目录，
/// 避免数据库文件被 Tauri 开发服务器的文件监控检测到而导致反复重启。
/// 如果 data/ 目录不存在，会自动创建。
///
/// # 返回
/// 数据库文件的完整路径
fn get_db_path() -> PathBuf {
    // 获取当前工作目录
    let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    // 开发模式下（cargo run），当前工作目录为 src-tauri/
    // 数据库文件需要放在项目根目录的 data/ 下，避免被 Tauri 的 watcher 监控到
    // 判断当前目录是否以 "src-tauri" 结尾，若是则回退到项目根目录
    let project_root = if current_dir
        .file_name()
        .map(|n| n == "src-tauri")
        .unwrap_or(false)
    {
        // 回退到上级目录（项目根目录）
        current_dir
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| current_dir.clone())
    } else {
        current_dir
    };

    // 在项目根目录下构建 data/ 子目录路径
    let data_dir = project_root.join("data");

    // 确保 data/ 目录存在，若不存在则创建
    if !data_dir.exists() {
        std::fs::create_dir_all(&data_dir)
            .expect("无法创建 data/ 目录，请检查文件系统权限");
    }

    // 返回数据库文件路径
    data_dir.join("markstudio.db")
}

/// 获取全局数据库连接（单例模式）
///
/// 首次调用时初始化数据库连接并创建必要的表结构。
/// 后续调用直接返回已存在的连接实例。
/// 使用 Mutex 包装 Connection，确保多线程环境下的安全访问。
///
/// # 返回
/// 数据库连接的静态引用（Mutex 包装）
pub fn get_connection() -> &'static Mutex<Connection> {
    DB_CONNECTION.get_or_init(|| {
        // 获取数据库文件路径
        let db_path = get_db_path();
        println!("[MarkStudio] 数据库路径: {:?}", db_path);

        // 打开数据库连接（如果文件不存在则自动创建）
        let conn = Connection::open(&db_path)
            .expect("无法打开 SQLite 数据库，请检查路径和权限");

        // 初始化数据库表结构
        init_database(&conn);

        // 将连接包装在 Mutex 中返回
        Mutex::new(conn)
    })
}

/// 初始化数据库表结构
///
/// 创建应用所需的基础表：
/// - settings：用户设置表，以 key-value 形式存储
/// - recent_files：最近打开文件列表，按打开时间排序
///
/// 使用 IF NOT EXISTS 确保重复调用时不会报错。
///
/// # 参数
/// - `conn`: 数据库连接引用
fn init_database(conn: &Connection) {
    // 创建用户设置表，key 为主键，value 存储设置值
    conn.execute(
        "CREATE TABLE IF NOT EXISTS settings (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        )",
        [], // 无参数
    )
    .expect("无法创建 settings 表");

    // 创建最近文件表，id 自增主键，path 唯一约束
    conn.execute(
        "CREATE TABLE IF NOT EXISTS recent_files (
            id        INTEGER PRIMARY KEY AUTOINCREMENT,
            path      TEXT NOT NULL UNIQUE,
            opened_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )",
        [], // 无参数
    )
    .expect("无法创建 recent_files 表");

    println!("[MarkStudio] 数据库表初始化完成");
}

// ==================== 设置表操作 ====================

/// 保存用户设置
///
/// 使用 INSERT OR REPLACE 策略，如果 key 已存在则更新值，
/// 如果不存在则插入新记录。
///
/// # 参数
/// - `key`: 设置项名称
/// - `value`: 设置项的值
///
/// # 返回
/// 成功返回 Ok(())，失败返回错误描述字符串
pub fn set_setting(key: &str, value: &str) -> Result<(), String> {
    // 获取数据库连接锁
    let conn = get_connection()
        .lock()
        .map_err(|e| format!("获取数据库连接锁失败: {}", e))?;

    // 使用 INSERT OR REPLACE 实现存在则更新、不存在则插入
    conn.execute(
        "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
        rusqlite::params![key, value],
    )
    .map_err(|e| format!("保存设置失败: {}", e))?;

    Ok(())
}

/// 读取用户设置
///
/// 根据 key 查询 settings 表中对应的 value。
///
/// # 参数
/// - `key`: 要查询的设置项名称
///
/// # 返回
/// - Ok(Some(value))：找到对应的设置值
/// - Ok(None)：未找到该设置项
/// - Err(msg)：数据库操作失败
pub fn get_setting(key: &str) -> Result<Option<String>, String> {
    // 获取数据库连接锁
    let conn = get_connection()
        .lock()
        .map_err(|e| format!("获取数据库连接锁失败: {}", e))?;

    // 准备查询语句
    let mut stmt = conn
        .prepare("SELECT value FROM settings WHERE key = ?1")
        .map_err(|e| format!("准备查询语句失败: {}", e))?;

    // 查询指定 key 的值
    let result = stmt.query_row(rusqlite::params![key], |row| row.get::<_, String>(0));

    // 匹配结果：找到记录返回 Some，未找到返回 None，其它错误也返回 None
    match result {
        Ok(value) => Ok(Some(value)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(format!("查询设置失败: {}", e)),
    }
}

// ==================== 最近文件表操作 ====================

/// 添加最近打开文件记录
///
/// 将文件路径记录到 recent_files 表中。
/// 如果文件路径已存在，则更新其打开时间（通过 INSERT OR REPLACE 实现）。
/// 如果不存在，则插入新记录。
///
/// # 参数
/// - `path`: 文件的完整路径
///
/// # 返回
/// 成功返回 Ok(())，失败返回错误描述字符串
pub fn add_recent_file(path: &str) -> Result<(), String> {
    // 获取数据库连接锁
    let conn = get_connection()
        .lock()
        .map_err(|e| format!("获取数据库连接锁失败: {}", e))?;

    // 先删除旧记录（如果存在），再插入新记录，以更新 opened_at 时间戳
    conn.execute(
        "DELETE FROM recent_files WHERE path = ?1",
        rusqlite::params![path],
    )
    .map_err(|e| format!("删除旧记录失败: {}", e))?;

    // 插入新记录，opened_at 使用当前时间
    conn.execute(
        "INSERT INTO recent_files (path, opened_at) VALUES (?1, CURRENT_TIMESTAMP)",
        rusqlite::params![path],
    )
    .map_err(|e| format!("插入最近文件记录失败: {}", e))?;

    Ok(())
}

/// 获取最近打开文件列表
///
/// 查询 recent_files 表，按打开时间（opened_at）降序排列，
/// 最多返回 10 条记录。
///
/// # 返回
/// 最近打开的文件路径列表（字符串向量），按最近打开在前排列
pub fn get_recent_files() -> Result<Vec<String>, String> {
    // 获取数据库连接锁
    let conn = get_connection()
        .lock()
        .map_err(|e| format!("获取数据库连接锁失败: {}", e))?;

    // 准备查询语句，按打开时间降序，限制 10 条
    let mut stmt = conn
        .prepare(
            "SELECT path FROM recent_files ORDER BY opened_at DESC LIMIT 10",
        )
        .map_err(|e| format!("准备查询语句失败: {}", e))?;

    // 执行查询，收集所有路径
    let paths = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|e| format!("查询最近文件失败: {}", e))?;

    // 收集结果，忽略读取失败的行
    let mut result = Vec::new();
    for path_result in paths {
        match path_result {
            Ok(path) => result.push(path),
            Err(e) => eprintln!("[MarkStudio] 读取最近文件记录时出错: {}", e),
        }
    }

    Ok(result)
}