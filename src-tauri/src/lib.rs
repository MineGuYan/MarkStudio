// 声明模块
pub mod collaboration;
mod commands;
mod database;
mod error;
mod parser;
pub mod services;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 应用启动时初始化数据库连接，确保数据目录和表结构就绪
    // initialize_database 同时初始化主数据库和收藏夹数据库
    database::initialize_database();

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
            commands::save_image_cache,
            commands::compute_operation_cmd,
            commands::apply_operation_cmd,
            commands::paste_image_cmd,
            commands::check_dirty_cmd,
            commands::compute_line_position_cmd,
            commands::load_all_settings_cmd,
            commands::get_favorite_tree,
            commands::create_favorite_dir,
            commands::delete_favorite_dir,
            commands::rename_favorite_dir,
            commands::add_favorite_file,
            commands::remove_favorite_file,
            commands::save_open_tabs,
            commands::get_open_tabs,
            commands::remove_recent_file,
            commands::check_file_exists,
            commands::delete_local_file,
            commands::add_shared_file,
            commands::remove_shared_file,
            commands::get_shared_files,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
