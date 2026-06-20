import { ref, watchEffect } from "vue";

/**
 * useTheme 组合式函数 - 主题管理
 * 
 * 功能：
 * - 管理当前主题状态（light / dark）
 * - 通过设置 document.documentElement 的 data-theme 属性来切换全局主题
 * - 响应式地同步主题状态到 DOM 属性
 * - 提供 toggleTheme 方法用于切换主题
 * 
 * 使用方式：
 *   const { theme, toggleTheme } = useTheme();
 */

/** 主题类型：浅色或深色 */
export type Theme = "light" | "dark";

/**
 * 创建主题管理组合式函数
 * 
 * @returns 包含 theme 响应式引用和 toggleTheme 方法的对象
 */
export function useTheme() {
  // ==================== 状态定义 ====================

  /** 当前主题状态，默认为浅色主题 */
  const theme = ref<Theme>("light");

  // ==================== 副作用 ====================

  /**
   * 监听主题变化，自动同步到 DOM
   * 当 theme 值变化时，设置 document.documentElement 的 data-theme 属性
   */
  watchEffect(() => {
    document.documentElement.setAttribute("data-theme", theme.value);
  });

  // ==================== 方法 ====================

  /**
   * 切换主题
   * 在浅色（light）和深色（dark）之间切换
   */
  function toggleTheme(): void {
    theme.value = theme.value === "light" ? "dark" : "light";
  }

  return {
    /** 当前主题的响应式引用 */
    theme,
    /** 切换主题的方法 */
    toggleTheme,
  };
}