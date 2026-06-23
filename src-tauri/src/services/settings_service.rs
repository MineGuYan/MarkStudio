//! 设置管理服务
//!
//! 本模块负责管理应用设置项的读取、默认值回退和批量加载。
//! 所有设置项以键值对形式存储，优先从数据库读取，若数据库无记录则回退到默认值。

use std::collections::HashMap;

/// 默认主题：light（浅色主题）
pub const DEFAULT_THEME: &str = "light";

/// 默认图片缓存目录：相对于应用数据目录的路径
pub const DEFAULT_IMAGE_CACHE_DIR: &str = "data/image_cache/";

/// 获取默认图片缓存目录的绝对路径
///
/// 将相对路径 DEFAULT_IMAGE_CACHE_DIR 转换为绝对路径，
/// 确保设置面板中显示的是完整的绝对路径。
pub fn get_default_image_cache_dir_absolute() -> String {
    let current_dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));

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

    project_root
        .join("data")
        .join("image_cache")
        .to_string_lossy()
        .to_string()
}

/// 获取所有设置项的默认值
///
/// 返回一个包含所有已知设置项默认键值对的 HashMap。
/// 当数据库中没有对应记录时，将使用此处的默认值。
///
/// # 返回
/// 包含所有默认设置的 HashMap，key 为设置项名称，value 为默认值。
pub fn get_default_settings() -> HashMap<String, String> {
    let mut defaults = HashMap::new();
    defaults.insert("theme".to_string(), DEFAULT_THEME.to_string());
    defaults.insert(
        "image_cache_dir".to_string(),
        DEFAULT_IMAGE_CACHE_DIR.to_string(),
    );
    // 启动时恢复标签页：默认关闭（"false"）
    defaults.insert("restore_tabs_on_startup".to_string(), "false".to_string());
    defaults
}

/// 获取单个设置项的值，若数据库无记录则返回默认值
///
/// 查询顺序：
/// 1. 从数据库查询指定 key 的设置值
/// 2. 若数据库中存在，直接返回该值
/// 3. 若数据库中不存在，从 `get_default_settings()` 查找默认值
/// 4. 若默认值中也不存在，返回错误
///
/// # 参数
/// - `key`：设置项的键名
///
/// # 返回
/// - `Ok(value)`：设置项的值（数据库值或默认值）
/// - `Err(msg)`：key 既不在数据库中也不在默认值中
#[allow(dead_code)]
pub fn get_setting_with_default(key: &str) -> Result<String, String> {
    // 第一步：尝试从数据库获取
    let db_result = crate::database::get_setting(key)?;

    // 第二步：若数据库有值，直接返回
    if let Some(value) = db_result {
        return Ok(value);
    }

    // 第三步：数据库无值，回退到默认值
    let defaults = get_default_settings();
    defaults
        .get(key)
        .cloned()
        .ok_or_else(|| format!("未知的设置项: '{}'，且未提供默认值", key))
}

/// 加载所有设置项并合并数据库中的值
///
/// 先以默认值作为基准，再逐个从数据库读取实际值进行覆盖，
/// 最终返回合并后的完整设置表。
///
/// # 返回
/// - `Ok(map)`：合并后的完整设置键值对
/// - `Err(msg)`：数据库读取过程发生错误
pub fn load_all_settings() -> Result<HashMap<String, String>, String> {
    // 以默认值为基准
    let mut settings = get_default_settings();

    // 遍历每一个默认 key，尝试从数据库加载实际值
    for key in settings.clone().keys() {
        match crate::database::get_setting(key) {
            Ok(Some(value)) => {
                // 数据库中存在该 key，用数据库值覆盖默认值
                settings.insert(key.clone(), value);
            }
            Ok(None) => {
                // 数据库中不存在，保留默认值，无需操作
            }
            Err(e) => {
                // 数据库读取失败，返回错误
                return Err(format!("加载设置项 '{}' 失败: {}", key, e));
            }
        }
    }

    // 将图片缓存目录路径转换为绝对路径（如果是相对路径）
    if let Some(cache_dir) = settings.get("image_cache_dir") {
        let path = std::path::Path::new(cache_dir);
        if !path.is_absolute() {
            let abs_path = std::env::current_dir()
                .map(|c| c.join(path))
                .unwrap_or_else(|_| path.to_path_buf());
            settings.insert(
                "image_cache_dir".to_string(),
                abs_path.to_string_lossy().to_string(),
            );
        }
    }

    Ok(settings)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 验证 `get_default_settings` 返回的 HashMap 包含预期的键和值
    #[test]
    fn test_get_default_settings() {
        let defaults = get_default_settings();

        // 验证包含预期的键
        assert!(defaults.contains_key("theme"), "缺少 'theme' 键");
        assert!(
            defaults.contains_key("image_cache_dir"),
            "缺少 'image_cache_dir' 键"
        );

        // 验证默认值内容
        assert_eq!(defaults.get("theme").unwrap(), "light");
        assert_eq!(
            defaults.get("image_cache_dir").unwrap(),
            "data/image_cache/"
        );
    }

    /// 验证 `get_setting_with_default` 对已知 key 能返回默认值
    #[test]
    fn test_get_setting_with_default_known_key() {
        // 由于测试环境中数据库可能为空，该方法应返回默认值
        let result = get_setting_with_default("theme");
        // 无论数据库是否有值，该方法不应返回错误
        assert!(result.is_ok(), "获取已知设置项应成功");
        let value = result.unwrap();
        // 值应为非空字符串
        assert!(!value.is_empty(), "设置值不应为空");
    }

    /// 验证 `load_all_settings` 返回的 HashMap 包含所有预期键
    #[test]
    fn test_load_all_settings() {
        let result = load_all_settings();
        assert!(result.is_ok(), "加载所有设置应成功");

        let settings = result.unwrap();
        // 验证包含所有预期键
        assert!(settings.contains_key("theme"), "缺少 'theme' 键");
        assert!(
            settings.contains_key("image_cache_dir"),
            "缺少 'image_cache_dir' 键"
        );
        assert!(
            settings.contains_key("restore_tabs_on_startup"),
            "缺少 'restore_tabs_on_startup' 键"
        );

        // 验证所有值均为非空字符串
        for (key, value) in &settings {
            assert!(!value.is_empty(), "设置项 '{}' 的值不应为空", key);
        }
    }
}
