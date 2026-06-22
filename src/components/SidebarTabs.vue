<script setup lang="ts">
/**
 * SidebarTabs 组件 - 侧边栏标签页切换面板
 *
 * 功能：
 * - 提供三个标签页切换："大纲"、"最近访问"、"收藏夹"
 * - 默认显示"大纲"标签页
 * - 激活的标签页具有底部边框高亮效果
 * - 根据 activeTab prop 切换显示对应的子组件内容
 * - "大纲"标签页渲染已有的 Outline 组件
 * - "最近访问"和"收藏夹"标签页暂为占位内容
 * - 将 Outline 的 navigate 事件透传给父组件
 * - 预留 open-file、remove-recent、add-favorite 事件的发射接口
 */

import Outline from "./Outline.vue";
import type { OutlineItem } from "./Outline.vue";
import RecentFiles from "./RecentFiles.vue";
import Favorites from "./Favorites.vue";

// ==================== Props 定义 ====================

withDefaults(
  defineProps<{
    /** 当前激活的侧边栏标签页："outline" | "recent" | "favorites" */
    activeTab?: string;
    /** 大纲条目列表，透传给 Outline 子组件 */
    outline: OutlineItem[];
  }>(),
  {
    /** 默认激活"大纲"标签页 */
    activeTab: "outline",
  }
);

// ==================== Emits 定义 ====================

const emit = defineEmits<{
  /** 切换侧边栏标签页 */
  "update:activeTab": [tab: string];
  /** 大纲条目点击导航事件（透传自 Outline 子组件） */
  navigate: [line: number];
  /** 打开指定路径的文件 */
  "open-file": [path: string];
  /** 从最近访问列表中移除指定文件 */
  "remove-recent": [path: string];
  /** 将指定文件添加到收藏夹 */
  "add-favorite": [path: string];
}>();

// ==================== 标签页数据定义 ====================

/** 标签页配置列表：每个标签页包含唯一的标识键和中文显示名称 */
const tabs = [
  { key: "outline", label: "大纲" },
  { key: "recent", label: "最近访问" },
  { key: "favorites", label: "收藏夹" },
] as const;

// ==================== 方法定义 ====================

/**
 * 切换侧边栏标签页
 * 发出 update:activeTab 事件，由父组件响应更新 activeTab prop
 *
 * @param key - 目标标签页的标识键
 */
function switchTab(key: string): void {
  emit("update:activeTab", key);
}

/**
 * 处理大纲条目点击事件
 * 透传 Outline 子组件的 navigate 事件给父组件
 *
 * @param line - 点击的标题所在行号
 */
function onOutlineNavigate(line: number): void {
  emit("navigate", line);
}
</script>

<template>
  <div class="sidebar-tabs">
    <!-- ==================== 标签页切换栏 ==================== -->
    <div class="sidebar-tabs__header">
      <button
        v-for="tab in tabs"
        :key="tab.key"
        class="sidebar-tabs__tab"
        :class="{ 'sidebar-tabs__tab--active': activeTab === tab.key }"
        @click="switchTab(tab.key)"
      >
        {{ tab.label }}
      </button>
    </div>

    <!-- ==================== 标签页内容区域 ==================== -->
    <div class="sidebar-tabs__content">
      <!-- 大纲标签页：渲染已有的 Outline 组件 -->
      <Outline
        v-if="activeTab === 'outline'"
        :outline="outline"
        @navigate="onOutlineNavigate"
      />

      <!-- 最近访问标签页：渲染 RecentFiles 组件 -->
      <RecentFiles
        v-else-if="activeTab === 'recent'"
        @open-file="(path) => emit('open-file', path)"
        @add-favorite="(path) => emit('add-favorite', path)"
      />

      <!-- 收藏夹标签页：渲染 Favorites 组件 -->
      <Favorites
        v-else-if="activeTab === 'favorites'"
        @open-file="(path) => emit('open-file', path)"
      />
    </div>
  </div>
</template>

<style scoped>
/* ==================== 面板整体样式 ==================== */

.sidebar-tabs {
  /* 纵向 flex 布局 */
  display: flex;
  flex-direction: column;

  /* 撑满父容器高度 */
  height: 100%;

  /* 使用主题 CSS 变量 */
  background-color: var(--toolbar-bg-color);
  border-right: 1px solid var(--border-color);

  /* 防止被选中 */
  user-select: none;

  /* 过渡动画 */
  transition: background-color 0.3s ease, border-color 0.3s ease;
}

/* ==================== 标签页切换栏样式 ==================== */

.sidebar-tabs__header {
  /* 横向布局，等分标签页按钮 */
  display: flex;
  flex-direction: row;

  /* 底部边框 */
  border-bottom: 1px solid var(--border-color);

  /* 防止被压缩 */
  flex-shrink: 0;
}

/* ==================== 单个标签页按钮样式 ==================== */

.sidebar-tabs__tab {
  /* 均匀分布，每个标签页等宽 */
  flex: 1;

  /* 尺寸与样式 */
  height: 36px;
  padding: 0 8px;
  border: none;
  border-bottom: 2px solid transparent;
  background: transparent;
  cursor: pointer;

  /* 文字样式 */
  font-size: 13px;
  font-weight: 500;
  color: var(--text-color);
  white-space: nowrap;

  /* 过渡动画 */
  transition: color 0.15s ease, border-color 0.15s ease,
    background-color 0.15s ease;
}

/* 标签页按钮 hover 状态 */
.sidebar-tabs__tab:hover {
  background-color: var(--button-hover-bg);
}

/* 激活状态的标签页：底部边框高亮 */
.sidebar-tabs__tab--active {
  color: var(--button-active-text);
  border-bottom-color: var(--button-active-text);
}

/* ==================== 内容区域样式 ==================== */

.sidebar-tabs__content {
  /* 撑满剩余空间 */
  flex: 1;

  /* 支持内容溢出时滚动 */
  overflow-y: auto;
}

/* ==================== 占位内容样式 ==================== */

.sidebar-tabs__placeholder {
  /* 居中布局 */
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;

  /* 撑满容器 */
  height: 100%;
  padding: 24px 16px;
}

.sidebar-tabs__placeholder-text {
  font-size: 13px;
  color: var(--text-color);
  margin: 0 0 8px 0;
}

.sidebar-tabs__placeholder-hint {
  font-size: 12px;
  color: var(--editor-placeholder-color);
  margin: 0;
}
</style>