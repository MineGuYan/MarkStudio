<script setup lang="ts">
/**
 * SettingsPanel 组件 - 设置面板
 *
 * 功能：
 * - 左侧分类栏：列出所有设置分类，支持切换
 * - 右侧设置区域：展示当前选中分类下的具体设置选项
 * - 设计为可扩展结构，未来新增设置只需在配置中增加条目即可
 *
 * 设置分类结构：
 * - 每个分类有一组设置项
 * - 每个设置项包含：标识键、显示名称、描述、类型（文本/路径/切换等）
 * - 设置变更通过 emit 事件通知父组件进行持久化
 */

import { ref, computed } from "vue";

// ==================== 类型定义 ====================

/** 设置项类型枚举 */
type SettingType = "text" | "path" | "toggle" | "select";

/** 单个设置项的定义 */
interface SettingItem {
  /** 设置项的唯一标识键（用于持久化存储） */
  key: string;
  /** 设置项的显示名称 */
  label: string;
  /** 设置项的描述说明 */
  description: string;
  /** 设置项的输入类型 */
  type: SettingType;
  /** 设置项的当前值 */
  value: string;
  /** 默认值（用于重置时恢复） */
  defaultValue: string;
  /** 可选值列表（仅 select 类型使用） */
  options?: { label: string; value: string }[];
}

/** 设置分类的定义 */
interface SettingCategory {
  /** 分类的唯一标识 */
  id: string;
  /** 分类的显示名称 */
  label: string;
  /** 分类下的设置项列表 */
  settings: SettingItem[];
}

// ==================== Props 定义 ====================

/** 设置分类与设置项数据，由父组件传入 */
const props = defineProps<{
  /** 所有设置分类及其包含的设置项 */
  categories: SettingCategory[];
}>();

// ==================== Emits 定义 ====================

const emit = defineEmits<{
  /** 设置项值变更事件 */
  "update-setting": [key: string, value: string];
}>();

// ==================== 状态管理 ====================

/** 当前选中的分类 ID，默认选中第一个分类 */
const activeCategoryId = ref<string>(
  props.categories.length > 0 ? props.categories[0].id : ""
);

/** 当前选中分类的计算属性 */
const activeCategory = computed<SettingCategory | undefined>(() => {
  return props.categories.find((cat) => cat.id === activeCategoryId.value);
});

// ==================== 方法 ====================

/**
 * 切换选中的设置分类
 * @param categoryId - 目标分类 ID
 */
function selectCategory(categoryId: string): void {
  activeCategoryId.value = categoryId;
}

/**
 * 处理设置项值变更
 * @param key - 设置项的标识键
 * @param value - 新的值
 */
function onSettingChange(key: string, value: string): void {
  emit("update-setting", key, value);
}

/**
 * 触发路径选择对话框（使用浏览器的原生目录选择）
 * 由于 Tauri 的 dialog 插件不支持目录选择，这里使用简单的 input 方式，
 * 实际项目中使用 Tauri 的 dialog open 来选择目录
 * @param key - 设置项的标识键
 */
async function browsePath(key: string): Promise<void> {
  // 尝试使用 Tauri 的 dialog 插件选择目录
  try {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const selected = await open({
      directory: true,
      multiple: false,
      title: "选择图片缓存目录",
    });
    if (selected) {
      onSettingChange(key, selected);
    }
  } catch (e) {
    console.error("打开目录选择对话框失败:", e);
  }
}

/**
 * 重置设置项为默认值
 * @param key - 设置项的标识键
 * @param defaultValue - 默认值
 */
function resetSetting(key: string, defaultValue: string): void {
  onSettingChange(key, defaultValue);
}
</script>

<template>
  <!-- 设置面板：左侧分类栏 + 右侧设置区域 -->
  <div class="settings-panel">
    <!-- 左侧分类栏 -->
    <aside class="settings-sidebar">
      <div class="settings-sidebar__title">设置</div>
      <nav class="settings-sidebar__nav">
        <button
          v-for="category in categories"
          :key="category.id"
          class="settings-sidebar__item"
          :class="{
            'settings-sidebar__item--active': category.id === activeCategoryId,
          }"
          @click="selectCategory(category.id)"
        >
          <!-- 分类图标占位 -->
          <span class="settings-sidebar__icon">
            <svg
              class="icon"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
            >
              <circle v-if="category.id === 'general'" cx="12" cy="12" r="3" />
              <path
                v-if="category.id === 'general'"
                d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"
              />
              <path
                v-if="category.id === 'editor'"
                d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"
              />
              <path
                v-if="category.id === 'editor'"
                d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z"
              />
            </svg>
          </span>
          <span class="settings-sidebar__label">{{ category.label }}</span>
        </button>
      </nav>
    </aside>

    <!-- 右侧设置区域 -->
    <section class="settings-content">
      <!-- 当前分类标题 -->
      <h2 class="settings-content__title">
        {{ activeCategory?.label ?? "设置" }}
      </h2>

      <!-- 设置项列表 -->
      <div class="settings-content__items">
        <template v-if="activeCategory">
          <div
            v-for="setting in activeCategory.settings"
            :key="setting.key"
            class="setting-item"
          >
            <!-- 设置项标签与描述 -->
            <div class="setting-item__info">
              <label class="setting-item__label">{{ setting.label }}</label>
              <p class="setting-item__desc">{{ setting.description }}</p>
            </div>

            <!-- 设置项控件 -->
            <div class="setting-item__control">
              <!-- 文本输入框 -->
              <input
                v-if="setting.type === 'text'"
                type="text"
                class="setting-input"
                :value="setting.value"
                @input="
                  onSettingChange(
                    setting.key,
                    ($event.target as HTMLInputElement).value
                  )
                "
              />

              <!-- 路径选择器（带浏览按钮） -->
              <div v-if="setting.type === 'path'" class="setting-path">
                <input
                  type="text"
                  class="setting-input"
                  :value="setting.value"
                  @input="
                    onSettingChange(
                      setting.key,
                      ($event.target as HTMLInputElement).value
                    )
                  "
                />
                <button class="setting-btn" @click="browsePath(setting.key)">
                  浏览...
                </button>
                <button
                  class="setting-btn setting-btn--reset"
                  @click="resetSetting(setting.key, setting.defaultValue)"
                >
                  重置
                </button>
              </div>

              <!-- 开关切换 -->
              <label v-if="setting.type === 'toggle'" class="setting-toggle">
                <input
                  type="checkbox"
                  :checked="setting.value === 'true'"
                  @change="
                    onSettingChange(
                      setting.key,
                      ($event.target as HTMLInputElement).checked
                        ? 'true'
                        : 'false'
                    )
                  "
                />
                <span class="setting-toggle__slider"></span>
              </label>

              <!-- 下拉选择 -->
              <select
                v-if="setting.type === 'select'"
                class="setting-select"
                :value="setting.value"
                @change="
                  onSettingChange(
                    setting.key,
                    ($event.target as HTMLSelectElement).value
                  )
                "
              >
                <option
                  v-for="opt in setting.options"
                  :key="opt.value"
                  :value="opt.value"
                >
                  {{ opt.label }}
                </option>
              </select>
            </div>
          </div>
        </template>

        <!-- 空状态提示 -->
        <div v-if="!activeCategory?.settings.length" class="settings-empty">
          <p>此分类暂无设置项</p>
        </div>
      </div>
    </section>
  </div>
</template>

<style scoped>
/* ==================== 设置面板整体布局 ==================== */

.settings-panel {
  display: flex;
  flex-direction: row;
  height: 100%;
  width: 100%;
  background-color: var(--bg-color);
  overflow: hidden;
}

/* ==================== 左侧分类栏 ==================== */

.settings-sidebar {
  width: 180px;
  min-width: 180px;
  height: 100%;
  display: flex;
  flex-direction: column;
  background-color: var(--toolbar-bg-color);
  border-right: 1px solid var(--border-color);
  padding: 16px 0;
  overflow-y: auto;
}

.settings-sidebar__title {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-color);
  opacity: 0.5;
  text-transform: uppercase;
  letter-spacing: 0.5px;
  padding: 0 16px 12px;
}

.settings-sidebar__nav {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 0 8px;
}

.settings-sidebar__item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 12px;
  border: none;
  border-radius: 6px;
  background: transparent;
  cursor: pointer;
  font-size: 13px;
  color: var(--text-color);
  transition: all 0.15s ease;
  text-align: left;
  width: 100%;
}

.settings-sidebar__item:hover {
  background-color: var(--button-hover-bg);
}

.settings-sidebar__item--active {
  background-color: var(--button-active-bg);
  color: var(--button-active-text);
}

.settings-sidebar__icon {
  display: flex;
  align-items: center;
  flex-shrink: 0;
}

.settings-sidebar__icon .icon {
  width: 16px;
  height: 16px;
}

.settings-sidebar__label {
  white-space: nowrap;
}

/* ==================== 右侧设置区域 ==================== */

.settings-content {
  flex: 1;
  height: 100%;
  overflow-y: auto;
  padding: 24px 32px;
}

.settings-content__title {
  font-size: 20px;
  font-weight: 600;
  color: var(--heading-color);
  margin-bottom: 24px;
  padding-bottom: 12px;
  border-bottom: 1px solid var(--border-color);
}

.settings-content__items {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

/* ==================== 单个设置项 ==================== */

.setting-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 16px 20px;
  border-radius: 8px;
  transition: background-color 0.15s ease;
}

.setting-item:hover {
  background-color: var(--button-hover-bg);
}

.setting-item__info {
  flex: 1;
  min-width: 0;
  margin-right: 24px;
}

.setting-item__label {
  display: block;
  font-size: 14px;
  font-weight: 500;
  color: var(--text-color);
  margin-bottom: 4px;
}

.setting-item__desc {
  font-size: 12px;
  color: var(--text-color);
  opacity: 0.6;
  margin: 0;
  line-height: 1.5;
}

.setting-item__control {
  flex-shrink: 0;
  display: flex;
  align-items: center;
}

/* ==================== 输入控件样式 ==================== */

.setting-input {
  width: 280px;
  height: 34px;
  padding: 0 12px;
  border: 1px solid var(--border-color);
  border-radius: 6px;
  font-size: 13px;
  color: var(--text-color);
  background-color: var(--editor-bg-color);
  outline: none;
  transition: border-color 0.2s ease;
}

.setting-input:focus {
  border-color: var(--button-active-bg);
  box-shadow: 0 0 0 2px rgba(59, 130, 246, 0.15);
}

/* 路径选择器容器 */
.setting-path {
  display: flex;
  gap: 8px;
  align-items: center;
}

.setting-path .setting-input {
  width: 200px;
}

/* 通用按钮 */
.setting-btn {
  height: 34px;
  padding: 0 14px;
  border: 1px solid var(--border-color);
  border-radius: 6px;
  background: transparent;
  font-size: 13px;
  color: var(--text-color);
  cursor: pointer;
  transition: all 0.15s ease;
  white-space: nowrap;
}

.setting-btn:hover {
  background-color: var(--button-hover-bg);
}

.setting-btn--reset {
  color: var(--text-color);
  opacity: 0.6;
}

.setting-btn--reset:hover {
  opacity: 1;
  color: #ef4444;
}

/* 下拉选择 */
.setting-select {
  width: 200px;
  height: 34px;
  padding: 0 12px;
  border: 1px solid var(--border-color);
  border-radius: 6px;
  font-size: 13px;
  color: var(--text-color);
  background-color: var(--editor-bg-color);
  outline: none;
  cursor: pointer;
}

/* ==================== 开关切换控件 ==================== */

.setting-toggle {
  position: relative;
  display: inline-block;
  width: 48px;
  height: 28px;
  cursor: pointer;
}

.setting-toggle input {
  opacity: 0;
  width: 0;
  height: 0;
}

.setting-toggle__slider {
  position: absolute;
  inset: 0;
  background-color: var(--border-color);
  border-radius: 14px;
  transition: background-color 0.25s ease, box-shadow 0.25s ease;
  box-shadow: inset 0 2px 4px rgba(0, 0, 0, 0.1);
}

.setting-toggle__slider::before {
  content: "";
  position: absolute;
  top: 2px;
  left: 2px;
  width: 24px;
  height: 24px;
  background-color: white;
  border-radius: 50%;
  transition: transform 0.25s ease, box-shadow 0.25s ease;
  box-shadow: 0 2px 6px rgba(0, 0, 0, 0.2), 0 1px 2px rgba(0, 0, 0, 0.1);
}

.setting-toggle input:checked + .setting-toggle__slider {
  background-color: var(--button-active-bg);
  box-shadow: 0 0 0 2px rgba(59, 130, 246, 0.2), inset 0 1px 1px rgba(255, 255, 255, 0.2);
}

.setting-toggle input:checked + .setting-toggle__slider::before {
  transform: translateX(20px);
  box-shadow: 0 2px 8px rgba(59, 130, 246, 0.4), 0 1px 3px rgba(0, 0, 0, 0.15);
}

/* ==================== 空状态 ==================== */

.settings-empty {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 200px;
  color: var(--text-color);
  opacity: 0.4;
  font-size: 14px;
}

/* ==================== 滚动条样式 ==================== */

.settings-content::-webkit-scrollbar,
.settings-sidebar::-webkit-scrollbar {
  width: 6px;
}

.settings-content::-webkit-scrollbar-track,
.settings-sidebar::-webkit-scrollbar-track {
  background: transparent;
}

.settings-content::-webkit-scrollbar-thumb,
.settings-sidebar::-webkit-scrollbar-thumb {
  background-color: var(--border-color);
  border-radius: 3px;
}
</style>