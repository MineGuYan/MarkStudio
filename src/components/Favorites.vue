﻿﻿﻿﻿﻿﻿<script setup lang="ts">
/**
 * Favorites 组件 - 收藏夹面板
 *
 * 功能：
 * - 从后端加载收藏夹目录树结构并展示
 * - 目录支持折叠/展开
 * - 点击文件发出 open-file 事件
 * - 文件不存在时提示用户是否移除
 * - 右键上下文菜单：新建/重命名/删除目录，移除文件
 * - 操作后自动刷新目录树
 * - 空状态提示
 */

import { ref, computed, onMounted, onUnmounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import InputDialog from "./InputDialog.vue";

// ==================== 类型定义 ====================

/** 收藏文件接口 */
export interface FavoriteFile {
  /** 文件唯一标识 */
  id: number;
  /** 文件路径 */
  path: string;
  /** 所属目录 ID，null 表示根目录 */
  dir_id: number | null;
  /** 添加时间 */
  added_at: string;
}

/** 收藏目录接口（支持嵌套） */
export interface FavoriteDir {
  /** 目录唯一标识 */
  id: number;
  /** 目录名称 */
  name: string;
  /** 父目录 ID，null 表示根目录 */
  parent_id: number | null;
  /** 排序序号 */
  sort_order: number;
  /** 子目录列表 */
  children: FavoriteDir[];
  /** 目录下的文件列表 */
  files: FavoriteFile[];
}

/** 扁平化后的树节点（用于列表渲染） */
interface FlatNode {
  /** 节点类型 */
  type: "dir" | "file";
  /** 目录数据（type 为 'dir' 时有效） */
  dir?: FavoriteDir;
  /** 文件数据（type 为 'file' 时有效） */
  file?: FavoriteFile;
  /** 嵌套深度 */
  depth: number;
  /** 唯一标识（用于 key） */
  key: string;
}

// ==================== Emits 定义 ====================

/** 发出 open-file 事件，携带文件路径 */
const emit = defineEmits<{
  "open-file": [path: string];
}>();

// ==================== 状态定义 ====================

/** 收藏夹目录树数据 */
const tree = ref<FavoriteDir[]>([]);

/** 已折叠目录的 ID 集合 */
const collapsedSet = ref<Set<number>>(new Set());

/** 右键菜单是否可见 */
const contextMenuVisible = ref(false);

/** 右键菜单 X 坐标 */
const contextMenuX = ref(0);

/** 右键菜单 Y 坐标 */
const contextMenuY = ref(0);

/** 右键菜单目标类型：'dir' | 'file' | 'empty' */
const contextMenuTargetType = ref<"dir" | "file" | "empty">("empty");

/** 右键菜单目标目录（当 targetType 为 'dir' 时） */
const contextMenuDir = ref<FavoriteDir | null>(null);

/** 右键菜单目标文件（当 targetType 为 'file' 时） */
const contextMenuFile = ref<FavoriteFile | null>(null);

/** 输入对话框组件引用 */
const inputDialog = ref<InstanceType<typeof InputDialog> | null>(null);

// ==================== 计算属性 ====================

/**
 * 将目录树扁平化为可见的线性列表
 * 根据 collapsedSet 中的折叠状态，过滤掉被折叠的目录下的所有子节点
 */
const flatNodes = computed<FlatNode[]>(() => {
  const result: FlatNode[] = [];
  flattenTree(tree.value, 0, result);
  return result;
});

/**
 * 递归扁平化目录树
 *
 * @param dirs - 当前层级的目录列表
 * @param depth - 当前嵌套深度
 * @param result - 累积结果数组
 */
function flattenTree(
  dirs: FavoriteDir[],
  depth: number,
  result: FlatNode[]
): void {
  for (const dir of dirs) {
    // 添加目录节点
    result.push({
      type: "dir",
      dir,
      depth,
      key: `dir-${dir.id}`,
    });

    // 如果目录未折叠，递归处理子目录和文件
    if (!collapsedSet.value.has(dir.id)) {
      // 先添加子目录
      if (dir.children && dir.children.length > 0) {
        flattenTree(dir.children, depth + 1, result);
      }

      // 再添加文件
      if (dir.files && dir.files.length > 0) {
        for (const file of dir.files) {
          result.push({
            type: "file",
            file,
            depth: depth + 1,
            key: `file-${file.id}`,
          });
        }
      }
    }
  }
}

// ==================== 工具函数 ====================

/**
 * 从文件路径中提取文件名
 *
 * @param path - 完整文件路径
 * @returns 文件名（含扩展名）
 */
function getFileName(path: string): string {
  // 处理 Windows 和 Unix 风格路径分隔符
  const parts = path.replace(/\\/g, "/").split("/");
  return parts[parts.length - 1] || path;
}

// ==================== 方法定义 ====================

/**
 * 加载收藏夹目录树
 * 调用后端接口获取 JSON 字符串，解析后赋值给 tree
 */
async function loadTree(): Promise<void> {
  try {
    const json = await invoke<string>("get_favorite_tree");
    tree.value = JSON.parse(json);
  } catch (e) {
    console.error("加载收藏夹目录树失败:", e);
    tree.value = [];
  }
}

/**
 * 切换目录的折叠/展开状态
 *
 * @param dirId - 目录 ID
 */
function toggleDir(dirId: number): void {
  const newSet = new Set(collapsedSet.value);
  if (newSet.has(dirId)) {
    newSet.delete(dirId);
  } else {
    newSet.add(dirId);
  }
  collapsedSet.value = newSet;
}

/**
 * 处理文件点击事件
 * 先检查文件是否存在，若不存在则提示用户是否移除
 *
 * @param file - 被点击的收藏文件
 */
async function handleFileClick(file: FavoriteFile): Promise<void> {
  try {
    // 检查文件是否存在
    const exists = await invoke<boolean>("check_file_exists", {
      path: file.path,
    });
    if (!exists) {
      // 文件不存在，弹出确认对话框
      const confirmed = confirm(
        `文件 "${file.path}" 不存在。\n是否从收藏夹中移除该文件？`
      );
      if (confirmed) {
        await invoke("remove_favorite_file", { id: file.id });
        await loadTree();
      }
      return;
    }
    // 文件存在，发出 open-file 事件
    emit("open-file", file.path);
  } catch (e) {
    // 如果 check_file_exists 接口不存在或调用失败，直接发出 open-file 事件
    console.error("检查文件存在性失败:", e);
    emit("open-file", file.path);
  }
}

// ==================== 右键菜单相关 ====================

/**
 * 处理目录右键点击事件
 *
 * @param event - 鼠标事件
 * @param dir - 被右键点击的目录
 */
function handleDirContextMenu(event: MouseEvent, dir: FavoriteDir): void {
  event.preventDefault();
  event.stopPropagation();
  contextMenuTargetType.value = "dir";
  contextMenuDir.value = dir;
  contextMenuFile.value = null;
  contextMenuX.value = event.clientX;
  contextMenuY.value = event.clientY;
  contextMenuVisible.value = true;
}

/**
 * 处理文件右键点击事件
 *
 * @param event - 鼠标事件
 * @param file - 被右键点击的文件
 */
function handleFileContextMenu(event: MouseEvent, file: FavoriteFile): void {
  event.preventDefault();
  event.stopPropagation();
  contextMenuTargetType.value = "file";
  contextMenuFile.value = file;
  contextMenuDir.value = null;
  contextMenuX.value = event.clientX;
  contextMenuY.value = event.clientY;
  contextMenuVisible.value = true;
}

/**
 * 处理面板空白区域右键点击事件
 *
 * @param event - 鼠标事件
 */
function handleEmptyContextMenu(event: MouseEvent): void {
  event.preventDefault();
  contextMenuTargetType.value = "empty";
  contextMenuDir.value = null;
  contextMenuFile.value = null;
  contextMenuX.value = event.clientX;
  contextMenuY.value = event.clientY;
  contextMenuVisible.value = true;
}

/** 关闭右键菜单 */
function closeContextMenu(): void {
  contextMenuVisible.value = false;
  contextMenuDir.value = null;
  contextMenuFile.value = null;
}

/** 新建子目录 */
async function handleCreateSubDir(): Promise<void> {
  const parentDir = contextMenuDir.value;
  closeContextMenu();
  const name = await inputDialog.value?.show({
    title: "新建目录",
    placeholder: "请输入目录名称",
  });
  if (!name) return;
  const parentId = parentDir?.id ?? null;
  try {
    await invoke("create_favorite_dir", { name, parentId });
    await loadTree();
  } catch (e) {
    alert("创建目录失败: " + (e as Error).message);
  }
}

/** 新建根目录 */
async function handleCreateRootDir(): Promise<void> {
  closeContextMenu();
  const name = await inputDialog.value?.show({
    title: "新建根目录",
    placeholder: "请输入目录名称",
  });
  if (!name) return;
  try {
    await invoke("create_favorite_dir", { name, parentId: null });
    await loadTree();
  } catch (e) {
    alert("创建目录失败: " + (e as Error).message);
  }
}

/** 重命名目录 */
async function handleRenameDir(): Promise<void> {
  const dir = contextMenuDir.value;
  if (!dir) return;
  closeContextMenu();
  const newName = await inputDialog.value?.show({
    title: "重命名目录",
    placeholder: "请输入新名称",
    defaultValue: dir.name,
  });
  if (!newName || newName === dir.name) return;
  try {
    await invoke("rename_favorite_dir", { id: dir.id, name: newName });
    await loadTree();
  } catch (e) {
    alert("重命名目录失败: " + (e as Error).message);
  }
}

/** 删除目录 */
async function handleDeleteDir(): Promise<void> {
  const dir = contextMenuDir.value;
  if (!dir) return;
  closeContextMenu();
  const confirmed = confirm(
    `确定要删除目录 "${dir.name}" 吗？\n该目录下的所有子目录和文件也将被删除。`
  );
  if (!confirmed) return;
  try {
    await invoke("delete_favorite_dir", { id: dir.id });
    await loadTree();
  } catch (e) {
    alert("删除目录失败: " + (e as Error).message);
  }
}

/** 移除文件 */
async function handleRemoveFile(): Promise<void> {
  const file = contextMenuFile.value;
  if (!file) return;
  closeContextMenu();
  const confirmed = confirm(`确定要从收藏夹中移除文件 "${getFileName(file.path)}" 吗？`);
  if (!confirmed) return;
  try {
    await invoke("remove_favorite_file", { id: file.id });
    await loadTree();
  } catch (e) {
    // Tauri invoke 返回的错误可能是字符串或 Error 对象
    const errMsg = typeof e === "string" ? e : (e as Error).message || String(e);
    alert("移除文件失败: " + errMsg);
  }
}

/** 删除文件 */
async function handleDeleteFile(): Promise<void> {
  const file = contextMenuFile.value;
  if (!file) return;
  closeContextMenu();
  const confirmed = confirm(
    `确定要删除文件 "${getFileName(file.path)}" 吗？\n此操作将删除本地文件，无法撤销！`
  );
  if (!confirmed) return;
  try {
    await invoke("remove_favorite_file", { id: file.id });
    await invoke("delete_local_file", { path: file.path });
    await loadTree();
  } catch (e) {
    // Tauri invoke 返回的错误可能是字符串或 Error 对象
    const errMsg = typeof e === "string" ? e : (e as Error).message || String(e);
    alert("删除文件失败: " + errMsg);
  }
}

// ==================== 生命周期 ====================

/** 组件挂载时加载目录树，并注册全局点击事件以关闭右键菜单 */
onMounted(() => {
  loadTree();
  document.addEventListener("click", onDocumentClick);
});

/** 组件卸载时移除全局事件监听 */
onUnmounted(() => {
  document.removeEventListener("click", onDocumentClick);
});

/** 暴露刷新方法给父组件 */
defineExpose({
  refresh: loadTree,
});

/**
 * 全局点击事件处理：点击菜单外部时关闭右键菜单
 */
function onDocumentClick(event: MouseEvent): void {
  if (!contextMenuVisible.value) return;
  const menu = document.querySelector(".favorites-context-menu");
  if (menu && !menu.contains(event.target as Node)) {
    closeContextMenu();
  }
}
</script>

<template>
  <div class="favorites-panel" @contextmenu="handleEmptyContextMenu">
    <!-- 面板标题 -->
    <div class="favorites-panel__header">收藏夹</div>

    <!-- 目录树列表 -->
    <div class="favorites-panel__list">
      <!-- 空状态提示 -->
      <div v-if="tree.length === 0" class="favorites-panel__empty">
        暂无收藏
      </div>

      <!-- 遍历扁平化后的可见节点 -->
      <div
        v-for="node in flatNodes"
        :key="node.key"
        class="favorites-panel__item"
        :class="{
          'favorites-panel__item--dir': node.type === 'dir',
          'favorites-panel__item--file': node.type === 'file',
        }"
        :style="{
          /** 根据嵌套深度计算缩进距离 */
          paddingLeft: node.depth * 1.2 + 0.5 + 'em',
        }"
        @contextmenu="
          node.type === 'dir'
            ? handleDirContextMenu($event, node.dir!)
            : handleFileContextMenu($event, node.file!)
        "
      >
        <!-- 目录节点 -->
        <template v-if="node.type === 'dir' && node.dir">
          <!-- 折叠/展开箭头图标 -->
          <span
            class="favorites-panel__toggle"
            :class="{
              'favorites-panel__toggle--collapsed': collapsedSet.has(
                node.dir.id
              ),
            }"
            @click.stop="toggleDir(node.dir.id)"
          >
            <!-- 右箭头 SVG 图标 -->
            <svg
              class="favorites-panel__toggle-icon"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
            >
              <polyline points="9 18 15 12 9 6" />
            </svg>
          </span>

          <!-- 目录图标 -->
          <span class="favorites-panel__icon favorites-panel__icon--dir">
            <svg
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
            >
              <path
                d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"
              />
            </svg>
          </span>

          <!-- 目录名称 -->
          <span class="favorites-panel__item-text">
            {{ node.dir.name }}
          </span>
        </template>

        <!-- 文件节点 -->
        <template v-if="node.type === 'file' && node.file">
          <!-- 占位对齐（无箭头图标） -->
          <span
            class="favorites-panel__toggle favorites-panel__toggle--placeholder"
          />

          <!-- 文件图标 -->
          <span class="favorites-panel__icon favorites-panel__icon--file">
            <svg
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
            >
              <path
                d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"
              />
              <polyline points="14 2 14 8 20 8" />
            </svg>
          </span>

          <!-- 文件名称（点击打开） -->
          <span
            class="favorites-panel__item-text"
            :title="node.file.path"
            @click="handleFileClick(node.file)"
          >
            {{ getFileName(node.file.path) }}
          </span>
        </template>
      </div>
    </div>

    <!-- 右键上下文菜单 -->
    <Teleport to="body">
      <div
        v-if="contextMenuVisible"
        class="favorites-context-menu"
        :style="{
          left: contextMenuX + 'px',
          top: contextMenuY + 'px',
        }"
      >
        <!-- 目录右键菜单项 -->
        <template v-if="contextMenuTargetType === 'dir'">
          <div class="context-menu-item" @click="handleCreateSubDir">
            新建目录
          </div>
          <div class="context-menu-item" @click="handleRenameDir">
            重命名
          </div>
          <div class="context-menu-item" @click="handleDeleteDir">
            删除
          </div>
        </template>

        <!-- 文件右键菜单项 -->
        <template v-if="contextMenuTargetType === 'file'">
          <div class="context-menu-item" @click="handleRemoveFile">
            移除
          </div>
          <div class="context-menu-item context-menu-item--danger" @click="handleDeleteFile">
            删除文件
          </div>
        </template>

        <!-- 空白区域右键菜单项 -->
        <template v-if="contextMenuTargetType === 'empty'">
          <div class="context-menu-item" @click="handleCreateRootDir">
            新建根目录
          </div>
        </template>
      </div>
    </Teleport>

    <InputDialog ref="inputDialog" />
  </div>
</template>

<style scoped>
/* ==================== 面板整体样式 ==================== */

.favorites-panel {
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

.favorites-panel__header {
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

.favorites-panel__list {
  /* 撑满剩余空间 */
  flex: 1;

  /* 支持内容溢出时滚动 */
  overflow-y: auto;

  /* 上下内边距 */
  padding: 4px 0;
}

/* ==================== 滚动条样式 ==================== */

.favorites-panel__list::-webkit-scrollbar {
  width: 6px;
}

.favorites-panel__list::-webkit-scrollbar-track {
  background: transparent;
}

.favorites-panel__list::-webkit-scrollbar-thumb {
  background-color: var(--border-color);
  border-radius: 3px;
}

/* ==================== 空状态提示样式 ==================== */

.favorites-panel__empty {
  padding: 16px;
  text-align: center;
  font-size: 13px;
  color: var(--editor-placeholder-color);
}

/* ==================== 条目样式 ==================== */

.favorites-panel__item {
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
.favorites-panel__item:hover {
  background-color: var(--button-hover-bg);
}

/* ==================== 折叠/展开箭头样式 ==================== */

.favorites-panel__toggle {
  /* 箭头图标容器 */
  display: flex;
  align-items: center;
  justify-content: center;

  /* 固定尺寸，保证对齐 */
  width: 16px;
  height: 16px;

  /* 与文本之间的间距 */
  margin-right: 4px;

  /* 防止被 flex 压缩 */
  flex-shrink: 0;

  /* 颜色 */
  color: var(--text-color);
  opacity: 0.5;

  /* 过渡动画 */
  transition: transform 0.15s ease, opacity 0.15s ease;
}

/* 箭头图标 hover 时更明显 */
.favorites-panel__toggle:hover {
  opacity: 0.8;
}

/* 折叠状态下的箭头：指向右侧 */
.favorites-panel__toggle--collapsed {
  transform: rotate(0deg);
}

/* 展开状态下的箭头：旋转 90 度指向下方 */
.favorites-panel__toggle:not(.favorites-panel__toggle--collapsed) {
  transform: rotate(90deg);
}

/* 占位图标：文件节点无箭头，透明度为 0 以保持对齐 */
.favorites-panel__toggle--placeholder {
  opacity: 0;
  pointer-events: none;
}

/* 箭头 SVG 图标尺寸 */
.favorites-panel__toggle-icon {
  width: 12px;
  height: 12px;
}

/* ==================== 节点图标样式 ==================== */

.favorites-panel__icon {
  /* 图标容器 */
  display: flex;
  align-items: center;
  justify-content: center;

  /* 固定尺寸 */
  width: 14px;
  height: 14px;

  /* 与文本之间的间距 */
  margin-right: 6px;

  /* 防止被 flex 压缩 */
  flex-shrink: 0;

  /* 颜色 */
  color: var(--text-color);
  opacity: 0.6;
}

/* 目录图标颜色微调 */
.favorites-panel__icon--dir {
  color: var(--heading-color);
  opacity: 0.7;
}

/* ==================== 条目文本样式 ==================== */

.favorites-panel__item-text {
  /* 超长文本省略 */
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;

  /* 撑满剩余空间 */
  flex: 1;
}

/* ==================== 右键上下文菜单 ==================== */

.favorites-context-menu {
  /* 固定定位：跟随鼠标点击位置 */
  position: fixed;

  /* 尺寸 */
  min-width: 140px;

  /* 样式 */
  background-color: var(--toolbar-bg-color);
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

.context-menu-item--danger {
  color: #d32f2f;
}

.context-menu-item--danger:hover {
  background-color: rgba(211, 47, 47, 0.1);
}

.context-menu-divider {
  height: 1px;
  background-color: var(--border-color);
  margin: 4px 0;
}
</style>
