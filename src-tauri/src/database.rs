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
        std::fs::create_dir_all(&data_dir).expect("无法创建 data/ 目录，请检查文件系统权限");
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
        let conn = Connection::open(&db_path).expect("无法打开 SQLite 数据库，请检查路径和权限");

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

    // 创建标签页恢复表
    init_open_tabs_table(conn);

    println!("[MarkStudio] 数据库表初始化完成");
}

/// 初始化数据库表结构（公开函数，供 lib.rs 调用）
///
/// 创建应用所需的基础表。
pub fn initialize_database() {
    get_connection();
    // 触发收藏夹数据库初始化
    get_favorites_connection();
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
/// 最多返回 50 条记录。
///
/// # 返回
/// 最近打开的文件路径列表（字符串向量），按最近打开在前排列
pub fn get_recent_files() -> Result<Vec<String>, String> {
    // 获取数据库连接锁
    let conn = get_connection()
        .lock()
        .map_err(|e| format!("获取数据库连接锁失败: {}", e))?;

    // 准备查询语句，按打开时间降序，限制 50 条
    let mut stmt = conn
        .prepare("SELECT path FROM recent_files ORDER BY opened_at DESC LIMIT 50")
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

/// 从最近文件列表中移除指定路径的记录
///
/// # 参数
/// - `path`: 要移除的文件路径
///
/// # 返回
/// 成功返回 Ok(())，失败返回错误描述字符串
pub fn remove_recent_file(path: &str) -> Result<(), String> {
    let conn = get_connection()
        .lock()
        .map_err(|e| format!("获取数据库连接锁失败: {}", e))?;
    conn.execute(
        "DELETE FROM recent_files WHERE path = ?1",
        rusqlite::params![path],
    )
    .map_err(|e| format!("删除最近文件记录失败: {}", e))?;
    Ok(())
}

/// 清空所有最近文件记录
///
/// # 返回
/// 成功返回 Ok(())，失败返回错误描述字符串
#[allow(dead_code)]
pub fn clear_recent_files() -> Result<(), String> {
    let conn = get_connection()
        .lock()
        .map_err(|e| format!("获取数据库连接锁失败: {}", e))?;
    conn.execute("DELETE FROM recent_files", [])
        .map_err(|e| format!("清空最近文件记录失败: {}", e))?;
    Ok(())
}

// ==================== 收藏夹数据库操作 ====================

/// 收藏夹数据库连接单例
///
/// 使用独立的数据库文件 `data/favorites.db` 存储收藏夹数据。
static FAVORITES_DB: std::sync::OnceLock<Mutex<Connection>> = std::sync::OnceLock::new();

/// 获取收藏夹数据库文件路径
///
/// 数据库文件位于项目根目录下的 data/favorites.db。
fn get_favorites_db_path() -> PathBuf {
    let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
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
    let data_dir = project_root.join("data");
    if !data_dir.exists() {
        std::fs::create_dir_all(&data_dir).expect("无法创建 data/ 目录");
    }
    data_dir.join("favorites.db")
}

/// 获取收藏夹数据库连接（单例模式）
///
/// 首次调用时初始化收藏夹数据库连接并创建必要的表结构。
pub fn get_favorites_connection() -> &'static Mutex<Connection> {
    FAVORITES_DB.get_or_init(|| {
        let db_path = get_favorites_db_path();
        println!("[MarkStudio] 收藏夹数据库路径: {:?}", db_path);
        let conn = Connection::open(&db_path).expect("无法打开收藏夹数据库，请检查路径和权限");
        init_favorites_db(&conn);
        Mutex::new(conn)
    })
}

/// 初始化收藏夹数据库表结构
///
/// 创建收藏夹所需的表：
/// - favorite_dirs：收藏夹目录表（支持无限层级嵌套）
/// - favorite_files：收藏文件表（存储文件路径，关联到目录）
fn init_favorites_db(conn: &Connection) {
    conn.execute("PRAGMA foreign_keys = ON", [])
        .expect("无法启用外键约束");

    conn.execute(
        "CREATE TABLE IF NOT EXISTS favorite_dirs (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            name       TEXT NOT NULL,
            parent_id  INTEGER,
            sort_order INTEGER NOT NULL DEFAULT 0,
            FOREIGN KEY (parent_id) REFERENCES favorite_dirs(id) ON DELETE CASCADE
        )",
        [],
    )
    .expect("无法创建 favorite_dirs 表");

    conn.execute(
        "CREATE TABLE IF NOT EXISTS favorite_files (
            id       INTEGER PRIMARY KEY AUTOINCREMENT,
            path     TEXT NOT NULL,
            dir_id   INTEGER NOT NULL,
            added_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (dir_id) REFERENCES favorite_dirs(id) ON DELETE CASCADE
        )",
        [],
    )
    .expect("无法创建 favorite_files 表");

    println!("[MarkStudio] 收藏夹数据库表初始化完成");
}

/// 获取收藏夹完整目录树
///
/// 包含所有目录和其中的文件列表，递归构建树形结构。
///
/// # 返回
/// 目录树列表（根级别目录），每个目录包含子目录和文件列表
#[derive(Debug, Clone, serde::Serialize)]
pub struct FavoriteDirInfo {
    pub id: i64,
    pub name: String,
    pub parent_id: Option<i64>,
    pub sort_order: i32,
    pub children: Vec<FavoriteDirInfo>,
    pub files: Vec<FavoriteFileInfo>,
}

/// 收藏文件信息
#[derive(Debug, Clone, serde::Serialize)]
pub struct FavoriteFileInfo {
    pub id: i64,
    pub path: String,
    pub dir_id: i64,
    pub added_at: String,
}

/// 获取收藏夹完整目录树
pub fn get_favorite_tree() -> Result<Vec<FavoriteDirInfo>, String> {
    let conn = get_favorites_connection()
        .lock()
        .map_err(|e| format!("获取收藏夹数据库连接锁失败: {}", e))?;
    // 获取根级别目录（parent_id 为 NULL）
    let mut stmt = conn
        .prepare("SELECT id, name, parent_id, sort_order FROM favorite_dirs WHERE parent_id IS NULL ORDER BY sort_order, id")
        .map_err(|e| format!("查询根目录失败: {}", e))?;
    let roots = stmt
        .query_map([], |row| {
            Ok(FavoriteDirInfo {
                id: row.get(0)?,
                name: row.get(1)?,
                parent_id: row.get(2)?,
                sort_order: row.get(3)?,
                children: Vec::new(),
                files: Vec::new(),
            })
        })
        .map_err(|e| format!("查询根目录失败: {}", e))?;
    let mut result = Vec::new();
    for root in roots {
        match root {
            Ok(mut dir) => {
                // 递归加载子目录和文件
                load_favorite_dir_children(&conn, &mut dir)?;
                result.push(dir);
            }
            Err(e) => eprintln!("[MarkStudio] 读取收藏夹目录时出错: {}", e),
        }
    }
    Ok(result)
}

/// 递归加载目录的子目录和文件
fn load_favorite_dir_children(conn: &Connection, dir: &mut FavoriteDirInfo) -> Result<(), String> {
    // 加载子目录
    let mut stmt = conn
        .prepare("SELECT id, name, parent_id, sort_order FROM favorite_dirs WHERE parent_id = ?1 ORDER BY sort_order, id")
        .map_err(|e| format!("查询子目录失败: {}", e))?;
    let children = stmt
        .query_map(rusqlite::params![dir.id], |row| {
            Ok(FavoriteDirInfo {
                id: row.get(0)?,
                name: row.get(1)?,
                parent_id: row.get(2)?,
                sort_order: row.get(3)?,
                children: Vec::new(),
                files: Vec::new(),
            })
        })
        .map_err(|e| format!("查询子目录失败: {}", e))?;
    for child in children {
        match child {
            Ok(mut child_dir) => {
                load_favorite_dir_children(conn, &mut child_dir)?;
                dir.children.push(child_dir);
            }
            Err(e) => eprintln!("[MarkStudio] 读取子目录时出错: {}", e),
        }
    }
    // 加载文件列表
    let mut file_stmt = conn
        .prepare("SELECT id, path, dir_id, added_at FROM favorite_files WHERE dir_id = ?1 ORDER BY added_at DESC")
        .map_err(|e| format!("查询文件列表失败: {}", e))?;
    let files = file_stmt
        .query_map(rusqlite::params![dir.id], |row| {
            Ok(FavoriteFileInfo {
                id: row.get(0)?,
                path: row.get(1)?,
                dir_id: row.get(2)?,
                added_at: row.get::<_, String>(3).unwrap_or_default(),
            })
        })
        .map_err(|e| format!("查询文件列表失败: {}", e))?;
    for file in files {
        match file {
            Ok(f) => dir.files.push(f),
            Err(e) => eprintln!("[MarkStudio] 读取收藏文件时出错: {}", e),
        }
    }
    Ok(())
}

/// 创建收藏夹目录
pub fn create_favorite_dir(name: &str, parent_id: Option<i64>) -> Result<i64, String> {
    let conn = get_favorites_connection()
        .lock()
        .map_err(|e| format!("获取收藏夹数据库连接锁失败: {}", e))?;

    let count: i64 = if let Some(pid) = parent_id {
        conn.query_row(
            "SELECT COUNT(*) FROM favorite_dirs WHERE parent_id = ?1 AND name = ?2",
            rusqlite::params![pid, name],
            |row| row.get(0),
        )
        .map_err(|e| format!("检查目录名称失败: {}", e))?
    } else {
        conn.query_row(
            "SELECT COUNT(*) FROM favorite_dirs WHERE parent_id IS NULL AND name = ?1",
            rusqlite::params![name],
            |row| row.get(0),
        )
        .map_err(|e| format!("检查目录名称失败: {}", e))?
    };

    if count > 0 {
        return Err("同级目录下已存在同名目录".to_string());
    }

    conn.execute(
        "INSERT INTO favorite_dirs (name, parent_id) VALUES (?1, ?2)",
        rusqlite::params![name, parent_id],
    )
    .map_err(|e| format!("创建收藏夹目录失败: {}", e))?;
    Ok(conn.last_insert_rowid())
}

/// 删除收藏夹目录（级联删除子目录和文件）
pub fn delete_favorite_dir(id: i64) -> Result<(), String> {
    let conn = get_favorites_connection()
        .lock()
        .map_err(|e| format!("获取收藏夹数据库连接锁失败: {}", e))?;
    conn.execute(
        "DELETE FROM favorite_dirs WHERE id = ?1",
        rusqlite::params![id],
    )
    .map_err(|e| format!("删除收藏夹目录失败: {}", e))?;
    Ok(())
}

/// 重命名收藏夹目录
pub fn rename_favorite_dir(id: i64, name: &str) -> Result<(), String> {
    let conn = get_favorites_connection()
        .lock()
        .map_err(|e| format!("获取收藏夹数据库连接锁失败: {}", e))?;

    let parent_id: Option<i64> = conn
        .query_row(
            "SELECT parent_id FROM favorite_dirs WHERE id = ?1",
            rusqlite::params![id],
            |row| row.get(0),
        )
        .map_err(|e| format!("获取目录信息失败: {}", e))?;

    let count: i64 = if let Some(pid) = parent_id {
        conn.query_row(
            "SELECT COUNT(*) FROM favorite_dirs WHERE parent_id = ?1 AND name = ?2 AND id != ?3",
            rusqlite::params![pid, name, id],
            |row| row.get(0),
        )
        .map_err(|e| format!("检查目录名称失败: {}", e))?
    } else {
        conn.query_row(
            "SELECT COUNT(*) FROM favorite_dirs WHERE parent_id IS NULL AND name = ?1 AND id != ?2",
            rusqlite::params![name, id],
            |row| row.get(0),
        )
        .map_err(|e| format!("检查目录名称失败: {}", e))?
    };

    if count > 0 {
        return Err("同级目录下已存在同名目录".to_string());
    }

    conn.execute(
        "UPDATE favorite_dirs SET name = ?1 WHERE id = ?2",
        rusqlite::params![name, id],
    )
    .map_err(|e| format!("重命名收藏夹目录失败: {}", e))?;
    Ok(())
}

/// 添加文件到收藏夹目录
pub fn add_favorite_file(path: &str, dir_id: i64) -> Result<i64, String> {
    let conn = get_favorites_connection()
        .lock()
        .map_err(|e| format!("获取收藏夹数据库连接锁失败: {}", e))?;
    conn.execute(
        "INSERT INTO favorite_files (path, dir_id) VALUES (?1, ?2)",
        rusqlite::params![path, dir_id],
    )
    .map_err(|e| format!("添加收藏文件失败: {}", e))?;
    Ok(conn.last_insert_rowid())
}

/// 从收藏夹移除文件
pub fn remove_favorite_file(id: i64) -> Result<(), String> {
    let conn = get_favorites_connection()
        .lock()
        .map_err(|e| format!("获取收藏夹数据库连接锁失败: {}", e))?;
    conn.execute(
        "DELETE FROM favorite_files WHERE id = ?1",
        rusqlite::params![id],
    )
    .map_err(|e| format!("移除收藏文件失败: {}", e))?;
    Ok(())
}

// ==================== 标签页恢复表操作 ====================

/// 初始化标签页恢复表
///
/// 在 `markstudio.db` 中创建 `open_tabs` 表，
/// 用于存储应用关闭时打开的标签页信息，以便下次启动时恢复。
pub fn init_open_tabs_table(conn: &Connection) {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS open_tabs (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            tab_index       INTEGER NOT NULL,
            path            TEXT NOT NULL DEFAULT '',
            title           TEXT NOT NULL DEFAULT '新建文档',
            content         TEXT NOT NULL DEFAULT '',
            is_dirty        INTEGER NOT NULL DEFAULT 0,
            mode            TEXT NOT NULL DEFAULT 'source',
            active_tab      INTEGER NOT NULL DEFAULT 0
        )",
        [],
    )
    .expect("无法创建 open_tabs 表");
}

/// 保存打开的标签页信息
pub fn save_open_tabs(tabs_json: &str, active_index: usize) -> Result<(), String> {
    let conn = get_connection()
        .lock()
        .map_err(|e| format!("获取数据库连接锁失败: {}", e))?;
    // 先清空旧记录
    conn.execute("DELETE FROM open_tabs", [])
        .map_err(|e| format!("清空标签页记录失败: {}", e))?;
    // 解析 JSON 数组
    let tabs: Vec<serde_json::Value> =
        serde_json::from_str(tabs_json).map_err(|e| format!("解析标签页 JSON 失败: {}", e))?;
    for (i, tab) in tabs.iter().enumerate() {
        let path = tab["path"].as_str().unwrap_or("");
        let title = tab["title"].as_str().unwrap_or("新建文档");
        let content = tab["content"].as_str().unwrap_or("");
        let is_dirty = if tab["isDirty"].as_bool().unwrap_or(false) {
            1
        } else {
            0
        };
        let mode = tab["mode"].as_str().unwrap_or("source");
        let active = if i == active_index { 1 } else { 0 };
        conn.execute(
            "INSERT INTO open_tabs (tab_index, path, title, content, is_dirty, mode, active_tab) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![i as i32, path, title, content, is_dirty, mode, active],
        )
        .map_err(|e| format!("保存标签页记录失败: {}", e))?;
    }
    Ok(())
}

/// 获取上次打开的标签页信息
pub fn get_open_tabs() -> Result<String, String> {
    let conn = get_connection()
        .lock()
        .map_err(|e| format!("获取数据库连接锁失败: {}", e))?;
    let mut stmt = conn
        .prepare("SELECT tab_index, path, title, content, is_dirty, mode, active_tab FROM open_tabs ORDER BY tab_index")
        .map_err(|e| format!("查询标签页记录失败: {}", e))?;
    let tabs: Vec<serde_json::Value> = stmt
        .query_map([], |row| {
            Ok(serde_json::json!({
                "path": row.get::<_, String>(1).unwrap_or_default(),
                "title": row.get::<_, String>(2).unwrap_or_default(),
                "content": row.get::<_, String>(3).unwrap_or_default(),
                "isDirty": row.get::<_, i32>(4).unwrap_or(0) != 0,
                "mode": row.get::<_, String>(5).unwrap_or_else(|_| "source".to_string()),
                "active": row.get::<_, i32>(6).unwrap_or(0) != 0,
            }))
        })
        .map_err(|e| format!("查询标签页记录失败: {}", e))?
        .filter_map(|r| r.ok())
        .collect();
    serde_json::to_string(&tabs).map_err(|e| format!("序列化标签页数据失败: {}", e))
}

/// 清空标签页记录
#[allow(dead_code)]
pub fn clear_open_tabs() -> Result<(), String> {
    let conn = get_connection()
        .lock()
        .map_err(|e| format!("获取数据库连接锁失败: {}", e))?;
    conn.execute("DELETE FROM open_tabs", [])
        .map_err(|e| format!("清空标签页记录失败: {}", e))?;
    Ok(())
}
