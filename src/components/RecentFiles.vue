<script setup lang="ts">
/**
 * RecentFiles 组件 - 最近文件面板
 *
 * 功能：
 * - 挂载时通过 invoke("get_recent_files") 获取最近文件路径列表
 * - 显示文件名（非完整路径），hover 时通过 title 属性显示完整路径
 * - 点击文件条目时，先检查文件是否存在，存在则 emit open-file 事件
 * - 若文件不存在，弹出确认对话框询问是否移除该记录
 * - 支持右键菜单，提供"收藏"选项，emit add-favorite 事件
 * - 无最近文件时显示空状态提示
 */

import { ref, onMounted, onUnmounted } from "vue";
import { invoke } from "@tauri-apps/api/core";

// ==================== Emits 定义 ====================

/** 发出打开文件和添加收藏事件 */
const emit = defineEmits<{
  /** 点击文件条目时，携带文件路径供父组件打开 */
  "open-file": [path: string];
  /** 右键菜单选择"收藏"时，携带文件路径供父组件处理 */
  "add-favorite": [path: string];
}>();

// ==================== 状态定义 ====================

/** 最近文件路径列表 */
const recentFiles = ref<string[]>([]);

/** 右键菜单是否可见 */
const contextMenuVisible = ref(false);

/** 右键菜单的水平位置（px） */
const contextMenuX = ref(0);

/** 右键菜单的垂直位置（px） */
const contextMenuY = ref(0);

/** 当前右键菜单作用的文件路径 */
const contextMenuPath = ref<string>("");

/** 确认对话框是否可见 */
const confirmDialogVisible = ref(false);

/** 待移除的文件路径（确认对话框使用） */
const pendingRemovePath = ref<string>("");

// ==================== 工具函数 ====================

/**
 * 从完整文件路径中提取文件名
 * 支持 Windows（\）和 Unix（/）路径分隔符
 *
 * @param path - 完整文件路径
 * @returns 文件名
 */
function getFileName(path: string): string {
  const parts = path.replace(/\\/g, "/").split("/");
  return parts[parts.length - 1] || "未知文件";
}

// ==================== 数据加载 ====================

/**
 * 加载最近文件列表
 * 调用 Tauri 后端获取最近打开的文件路径列表
 */
async function loadRecentFiles(): Promise<void> {
  try {
    recentFiles.value = await invoke<string[]>("get_recent_files");
  } catch (error) {
    console.error("加载最近文件列表失败:", error);
    recentFiles.value = [];
  }
}

// ==================== 文件操作 ====================

/**
 * 处理文件条目点击事件
 * 先检查文件是否存在：
 * - 存在：emit open-file 事件
 * - 不存在：弹出确认对话框，询问是否移除该记录
 *
 * @param path - 被点击的文件路径
 */
async function onFileClick(path: string): Promise<void> {
  try {
    const exists = await invoke<boolean>("check_file_exists", { path });
    if (exists) {
      emit("open-file", path);
    } else {
      // 文件不存在，弹出确认对话框
      pendingRemovePath.value = path;
      confirmDialogVisible.value = true;
    }
  } catch (error) {
    console.error("检查文件是否存在失败:", error);
    // 出错时仍尝试打开文件
    emit("open-file", path);
  }
}

/**
 * 确认移除不存在的文件记录
 * 调用 Tauri 后端移除该记录，并刷新列表
 */
async function confirmRemove(): Promise<void> {
  try {
    await invoke("remove_recent_file", { path: pendingRemovePath.value });
    // 从本地列表中移除
    recentFiles.value = recentFiles.value.filter(
      (f) => f !== pendingRemovePath.value
    );
  } catch (error) {
    console.error("移除最近文件记录失败:", error);
  } finally {
    confirmDialogVisible.value = false;
    pendingRemovePath.value = "";
  }
}

/**
 * 取消移除操作
 * 关闭确认对话框
 */
function cancelRemove(): void {
  confirmDialogVisible.value = false;
  pendingRemovePath.value = "";
}

// ==================== 右键菜单 ====================

/**
 * 处理文件条目上的右键点击事件
 * 显示上下文菜单，记录点击位置和关联的文件路径
 *
 * @param event - 鼠标事件
 * @param path - 被右键点击的文件路径
 */
function handleContextMenu(event: MouseEvent, path: string): void {
  event.preventDefault();
  contextMenuPath.value = path;
  contextMenuX.value = event.clientX;
  contextMenuY.value = event.clientY;
  contextMenuVisible.value = true;
}

/** 关闭右键菜单 */
function closeContextMenu(): void {
  contextMenuVisible.value = false;
  contextMenuPath.value = "";
}

/**
 * 处理右键菜单项"收藏"
 * 发出 add-favorite 事件并关闭菜单
 */
function handleAddFavorite(): void {
  emit("add-favorite", contextMenuPath.value);
  closeContextMenu();
}

// ==================== 全局点击监听 ====================

/**
 * 全局点击事件处理
 * 当点击右键菜单或确认对话框外部区域时，关闭对应元素
 *
 * @param event - 鼠标事件
 */
function handleGlobalClick(event: MouseEvent): void {
  // 关闭右键菜单
  if (contextMenuVisible.value) {
    const contextMenu = document.querySelector(".recent-files-context-menu");
    if (contextMenu && !contextMenu.contains(event.target as Node)) {
      closeContextMenu();
    }
  }
}

// ==================== 生命周期 ====================

onMounted(() => {
  loadRecentFiles();
  document.addEventListener("click", handleGlobalClick);
});

onUnmounted(() => {
  document.removeEventListener("click", handleGlobalClick);
});

// ==================== 暴露方法 ====================

/**
 * 暴露 refresh 方法给父组件，用于在打开新文件后主动刷新最近文件列表
 */
defineExpose({
  refresh: loadRecentFiles,
});
</script>

<template>
  <div class="recent-files-panel">
    <!-- 面板标题 -->
    <div class="recent-files-panel__header">最近文件</div>

    <!-- 最近文件列表 -->
    <div class="recent-files-panel__list">
      <!-- 无最近文件时的空状态提示 -->
      <div v-if="recentFiles.length === 0" class="recent-files-panel__empty">
        暂无最近文件
      </div>

      <!-- 逐条渲染最近文件 -->
      <div
        v-for="path in recentFiles"
        :key="path"
        class="recent-files-panel__item"
        :title="path"
        @click="onFileClick(path)"
        @contextmenu="handleContextMenu($event, path)"
      >
        <!-- 文件图标 -->
        <span class="recent-files-panel__item-icon">
          <svg
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            width="14"
            height="14"
          >
            <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
            <polyline points="14 2 14 8 20 8" />
          </svg>
        </span>

        <!-- 文件名 -->
        <span class="recent-files-panel__item-text">
          {{ getFileName(path) }}
        </span>
      </div>
    </div>
  </div>

  <!-- 右键上下文菜单（通过 Teleport 挂载到 body，避免受 overflow 影响） -->
  <Teleport to="body">
    <div
      v-if="contextMenuVisible"
      class="recent-files-context-menu"
      :style="{
        left: contextMenuX + 'px',
        top: contextMenuY + 'px',
      }"
    >
      <div class="context-menu-item" @click="handleAddFavorite">
        收藏
      </div>
    </div>
  </Teleport>

  <!-- 文件不存在确认对话框 -->
  <Teleport to="body">
    <div v-if="confirmDialogVisible" class="recent-files-dialog-overlay">
      <div class="recent-files-dialog">
        <div class="recent-files-dialog__title">文件不存在</div>
        <div class="recent-files-dialog__message">
          文件"{{ getFileName(pendingRemovePath) }}"不存在，是否从最近文件列表中移除该记录？
        </div>
        <div class="recent-files-dialog__actions">
          <button
            class="recent-files-dialog__btn recent-files-dialog__btn--cancel"
            @click="cancelRemove"
          >
            取消
          </button>
          <button
            class="recent-files-dialog__btn recent-files-dialog__btn--confirm"
            @click="confirmRemove"
          >
            移除
          </button>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
/* ==================== 面板整体样式 ==================== */

.recent-files-panel {
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
}

/* ==================== 面板标题样式 ==================== */

.recent-files-panel__header {
  /* 标题样式 */
  padding: 12px 16px;
  font-size: 13px;
  font-weight: 600;
  color: var(--heading-color);
  border-bottom: 1px solid var(--border-color);

  /* 防止标题被压缩 */
  flex-shrink: 0;

  /* 过渡动画 */
  transition: color 0.3s ease;
}

/* ==================== 条目列表样式 ==================== */

.recent-files-panel__list {
  /* 撑满剩余空间 */
  flex: 1;

  /* 支持内容溢出时滚动 */
  overflow-y: auto;

  /* 上下内边距 */
  padding: 4px 0;
}

/* ==================== 滚动条样式 ==================== */

.recent-files-panel__list::-webkit-scrollbar {
  width: 6px;
}

.recent-files-panel__list::-webkit-scrollbar-track {
  background: transparent;
}

.recent-files-panel__list::-webkit-scrollbar-thumb {
  background-color: var(--border-color);
  border-radius: 3px;
}

/* ==================== 空状态提示样式 ==================== */

.recent-files-panel__empty {
  padding: 16px;
  text-align: center;
  font-size: 13px;
  color: var(--editor-placeholder-color);
}

/* ==================== 条目样式 ==================== */

.recent-files-panel__item {
  /* 条目布局 */
  display: flex;
  align-items: center;
  padding: 4px 16px;
  cursor: pointer;

  /* 文字样式 */
  font-size: 13px;
  color: var(--text-color);
  line-height: 1.6;

  /* 过渡动画 */
  transition: background-color 0.15s ease, color 0.15s ease;
}

/* 条目 hover 状态 */
.recent-files-panel__item:hover {
  background-color: var(--button-hover-bg);
}

/* ==================== 条目图标样式 ==================== */

.recent-files-panel__item-icon {
  /* 图标容器 */
  display: flex;
  align-items: center;
  justify-content: center;

  /* 固定尺寸，保证对齐 */
  width: 16px;
  height: 16px;

  /* 与文本之间的间距 */
  margin-right: 6px;

  /* 防止被 flex 压缩 */
  flex-shrink: 0;

  /* 颜色 */
  color: var(--text-color);
  opacity: 0.5;
}

/* ==================== 条目文本样式 ==================== */

.recent-files-panel__item-text {
  /* 超长文本省略 */
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;

  /* 撑满剩余空间 */
  flex: 1;
}

/* ==================== 右键上下文菜单 ==================== */

.recent-files-context-menu {
  /* 固定定位：跟随鼠标点击位置 */
  position: fixed;

  /* 尺寸 */
  min-width: 120px;

  /* 样式 */
  background-color: var(--bg-color);
  border: 1px solid var(--border-color);
  border-radius: 6px;
  box-shadow: 0 4px 16px rgba(0, 0, 0, 0.15);

  /* 层级 */
  z-index: 300;

  /* 间距 */
  padding: 4px 0;
}

/* 菜单项 */
.context-menu-item {
  /* 布局 */
  display: flex;
  align-items: center;

  /* 尺寸 */
  padding: 8px 16px;

  /* 样式 */
  cursor: pointer;
  color: var(--text-color);
  font-size: 13px;
  white-space: nowrap;

  /* 过渡动画 */
  transition: background-color 0.1s ease;
}

.context-menu-item:hover {
  background-color: var(--button-hover-bg);
}

/* ==================== 确认对话框样式 ==================== */

/* 对话框遮罩层 */
.recent-files-dialog-overlay {
  /* 固定定位，覆盖全屏 */
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;

  /* 半透明背景 */
  background-color: rgba(0, 0, 0, 0.4);

  /* 居中布局 */
  display: flex;
  align-items: center;
  justify-content: center;

  /* 层级：高于右键菜单 */
  z-index: 400;
}

/* 对话框本体 */
.recent-files-dialog {
  /* 样式 */
  background-color: var(--bg-color);
  border: 1px solid var(--border-color);
  border-radius: 8px;
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.2);

  /* 尺寸 */
  min-width: 360px;
  max-width: 480px;

  /* 内边距 */
  padding: 24px;
}

/* 对话框标题 */
.recent-files-dialog__title {
  font-size: 16px;
  font-weight: 600;
  color: var(--heading-color);
  margin-bottom: 12px;
}

/* 对话框消息 */
.recent-files-dialog__message {
  font-size: 14px;
  color: var(--text-color);
  line-height: 1.6;
  margin-bottom: 20px;
}

/* 对话框按钮区域 */
.recent-files-dialog__actions {
  display: flex;
  justify-content: flex-end;
  gap: 12px;
}

/* 对话框按钮通用样式 */
.recent-files-dialog__btn {
  padding: 8px 20px;
  border: 1px solid var(--border-color);
  border-radius: 6px;
  cursor: pointer;
  font-size: 13px;
  font-weight: 500;
  transition: background-color 0.15s ease, color 0.15s ease;
}

/* 取消按钮 */
.recent-files-dialog__btn--cancel {
  background-color: transparent;
  color: var(--text-color);
}

.recent-files-dialog__btn--cancel:hover {
  background-color: var(--button-hover-bg);
}

/* 确认按钮 */
.recent-files-dialog__btn--confirm {
  background-color: #d32f2f;
  color: #fff;
  border-color: #d32f2f;
}

.recent-files-dialog__btn--confirm:hover {
  background-color: #b71c1c;
  border-color: #b71c1c;
}
</style>