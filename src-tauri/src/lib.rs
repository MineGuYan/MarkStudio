// 声明模块
mod collaboration;
mod commands;
mod database;
mod parser;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 应用启动时初始化数据库连接，确保数据目录和表结构就绪
    database::get_connection();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            greet,
            commands::parse_markdown,
            commands::extract_outline,
            commands::read_file,
            commands::write_file,
            commands::save_setting,
            commands::get_setting,
            commands::add_recent_file,
            commands::get_recent_files,
            commands::create_collab_room,
            commands::join_collab_room,
            commands::leave_collab_room,
            commands::send_collab_operation,
            commands::send_collab_cursor,
            commands::get_local_ip,
            commands::get_collab_status,
            commands::send_collab_image,
            commands::get_collab_cache_dir,
            commands::save_temp_image,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
