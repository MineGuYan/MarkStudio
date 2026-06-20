import { onMounted, onUnmounted } from "vue";

/**
 * useShortcuts 组合式函数 - 快捷键支持
 *
 * 功能：
 * - 提供 registerShortcut(key: string, handler: () => void) 方法注册快捷键
 * - 提供 unregisterShortcut(key: string) 方法取消注册
 * - 在组件挂载时绑定 keydown 事件，卸载时移除
 * - 支持 Ctrl+Key 组合键
 *
 * 使用方式：
 *   const { registerShortcut, unregisterShortcut } = useShortcuts();
 *   registerShortcut("s", handleSave);    // Ctrl+S
 *   registerShortcut("f", handleFind);    // Ctrl+F
 */

// ==================== 类型定义 ====================

/** 快捷键映射表：key -> 处理函数 */
type ShortcutMap = Map<string, () => void>;

// ==================== 组合式函数 ====================

/**
 * 创建快捷键管理组合式函数
 *
 * @returns 包含 registerShortcut 和 unregisterShortcut 方法的对象
 */
export function useShortcuts() {
  // ==================== 状态定义 ====================

  /** 存储已注册的快捷键及其处理函数 */
  const shortcuts: ShortcutMap = new Map();

  // ==================== 事件处理 ====================

  /**
   * 全局 keydown 事件处理函数
   * 检测 Ctrl 组合键并执行对应的处理函数
   *
   * @param e - 键盘事件对象
   */
  function handleKeyDown(e: KeyboardEvent): void {
    // 仅处理 Ctrl 组合键（Windows/Linux 使用 Ctrl，macOS 使用 Cmd 也视为 Ctrl）
    if (!e.ctrlKey && !e.metaKey) return;

    // 获取按下的键（统一转为小写，方便匹配）
    const key = e.key.toLowerCase();

    // 查找是否已注册该快捷键
    const handler = shortcuts.get(key);
    if (handler) {
      // 阻止浏览器默认行为（如 Ctrl+S 保存网页）
      e.preventDefault();
      // 执行注册的处理函数
      handler();
    }
  }

  // ==================== 生命周期 ====================

  /**
   * 组件挂载时绑定键盘事件监听器
   */
  onMounted(() => {
    document.addEventListener("keydown", handleKeyDown);
  });

  /**
   * 组件卸载时移除键盘事件监听器，防止内存泄漏
   */
  onUnmounted(() => {
    document.removeEventListener("keydown", handleKeyDown);
    // 清空快捷键映射表
    shortcuts.clear();
  });

  // ==================== 公开方法 ====================

  /**
   * 注册一个快捷键
   *
   * @param key - 快捷键的键名（不区分大小写），如 "s"、"f" 等
   * @param handler - 按下快捷键时执行的回调函数
   */
  function registerShortcut(key: string, handler: () => void): void {
    shortcuts.set(key.toLowerCase(), handler);
  }

  /**
   * 取消注册一个快捷键
   *
   * @param key - 要取消的快捷键键名（不区分大小写）
   */
  function unregisterShortcut(key: string): void {
    shortcuts.delete(key.toLowerCase());
  }

  return {
    /** 注册快捷键方法 */
    registerShortcut,
    /** 取消注册快捷键方法 */
    unregisterShortcut,
  };
}