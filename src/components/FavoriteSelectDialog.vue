<script setup lang="ts">
/**
 * FavoriteSelectDialog 组件 - 收藏夹目录选择对话框
 *
 * 当用户将文件添加到收藏夹时，弹出此对话框供用户选择目标目录。
 * 功能包括：
 * - 展示收藏夹目录树结构
 * - 点击选中目标目录（高亮显示）
 * - 在选中层级下新建子目录
 * - 确认或取消选择
 *
 * 使用 Promise 模式与父组件交互：父组件通过 show() 方法显示对话框，
 * 该方法返回 Promise<number | null>，resolve 为选中目录的 ID（取消时为 null）。
 * 样式与 CloseConfirmDialog 保持一致。
 */

import { ref } from "vue";
import { invoke } from "@tauri-apps/api/core";

// ==================== 类型定义 ====================

/** 收藏夹目录树节点 */
interface FavoriteTreeNode {
  /** 节点唯一标识 */
  id: number;
  /** 节点名称 */
  name: string;
  /** 子节点列表 */
  children: FavoriteTreeNode[];
}

/** 扁平化后的树节点（用于渲染） */
interface FlatTreeNode {
  /** 节点唯一标识 */
  id: number;
  /** 节点名称 */
  name: string;
  /** 嵌套深度（0 为根级别） */
  depth: number;
  /** 是否有子节点 */
  hasChildren: boolean;
  /** 是否展开 */
  expanded: boolean;
}

// ==================== 响应式状态 ====================

/** 对话框是否可见 */
const visible = ref(false);

/** 收藏夹原始目录树数据 */
const treeData = ref<FavoriteTreeNode[]>([]);

/** 扁平化后的目录列表 */
const flatList = ref<FlatTreeNode[]>([]);

/** 当前选中的目录节点 ID（null 表示未选中） */
const selectedId = ref<number | null>(null);

/** 当前等待中的 Promise resolve 函数 */
let resolvePromise: ((value: number | null) => void) | null = null;

/** 新建目录输入框是否显示 */
const isCreating = ref(false);

/** 新目录名称 */
const newDirName = ref("");

/** 新建目录的错误提示信息 */
const createError = ref("");

// ==================== 公开方法 ====================

/**
 * 显示收藏夹目录选择对话框
 *
 * 加载收藏夹目录树并展示对话框，等待用户选择目标目录。
 *
 * @returns Promise，resolve 为选中目录的 ID：
 *   - number：用户选中了某个目录并点击"确认"
 *   - null：用户点击"取消"或关闭对话框
 */
function show(): Promise<number | null> {
  // 重置状态
  selectedId.value = null;
  isCreating.value = false;
  newDirName.value = "";
  createError.value = "";

  // 加载目录树
  loadTree();

  visible.value = true;
  return new Promise<number | null>((resolve) => {
    resolvePromise = resolve;
  });
}

/** 隐藏对话框 */
function hide(): void {
  visible.value = false;
  resolvePromise = null;
}

// ==================== 数据加载 ====================

/** 加载收藏夹目录树数据并构建扁平列表 */
async function loadTree(): Promise<void> {
  try {
    const json = await invoke<string>("get_favorite_tree");
    const data = JSON.parse(json) as FavoriteTreeNode[];
    treeData.value = data;
    buildFlatList(data);
  } catch (err) {
    console.error("加载收藏夹目录树失败:", err);
    treeData.value = [];
    flatList.value = [];
  }
}

/**
 * 将目录树递归展开为扁平列表
 *
 * @param nodes - 当前层级的节点列表
 * @param depth - 当前嵌套深度
 */
function buildFlatList(nodes: FavoriteTreeNode[], depth: number = 0): void {
  if (depth === 0) {
    flatList.value = [];
  }
  for (const node of nodes) {
    if (!node.name || node.name.trim() === "") {
      continue;
    }
    flatList.value.push({
      id: node.id,
      name: node.name,
      depth: depth,
      hasChildren: node.children && node.children.length > 0,
      expanded: true,
    });
    if (node.children && node.children.length > 0) {
      buildFlatList(node.children, depth + 1);
    }
  }
}

// ==================== 按钮事件处理 ====================

/** 用户点击"确认" */
function onConfirm(): void {
  if (resolvePromise) {
    resolvePromise(selectedId.value);
  }
  hide();
}

/** 用户点击"取消"或点击遮罩层 */
function onCancel(): void {
  if (resolvePromise) {
    resolvePromise(null);
  }
  hide();
}

// ==================== 新建目录 ====================

/** 点击"新建目录"按钮，展开新建输入框 */
function onStartCreate(): void {
  isCreating.value = true;
  newDirName.value = "";
  createError.value = "";
}

/** 确认新建目录 */
async function onConfirmCreate(): Promise<void> {
  const name = newDirName.value.trim();
  if (!name) {
    createError.value = "目录名称不能为空";
    return;
  }

  // 确定父级目录 ID：若当前有选中目录，则在其下创建；否则在根目录下创建
  const parentId = selectedId.value;

  try {
    await invoke("create_favorite_dir", {
      parentId: parentId,
      name: name,
    });
    // 创建成功后重新加载目录树
    isCreating.value = false;
    newDirName.value = "";
    createError.value = "";
    await loadTree();
  } catch (err) {
    console.error("创建收藏夹目录失败:", err);
    createError.value = "创建目录失败，请重试";
  }
}

/** 取消新建目录 */
function onCancelCreate(): void {
  isCreating.value = false;
  newDirName.value = "";
  createError.value = "";
}

// ==================== 目录选择与展开/折叠 ====================

/** 点击目录节点进行选中 */
function onSelectNode(id: number): void {
  selectedId.value = id;
}

/**
 * 切换节点的展开/折叠状态
 * 折叠时隐藏其所有子节点，展开时恢复显示
 */
function toggleExpand(nodeId: number): void {
  // 找到该节点在扁平列表中的位置
  const index = flatList.value.findIndex((n) => n.id === nodeId);
  if (index === -1) return;

  const node = flatList.value[index];
  node.expanded = !node.expanded;

  // 重新构建整个扁平列表以反映展开/折叠状态
  rebuildFlatList();
}

/**
 * 根据当前各节点的展开/折叠状态重新构建扁平列表
 */
function rebuildFlatList(): void {
  const result: FlatTreeNode[] = [];

  function walk(nodes: FavoriteTreeNode[], depth: number): void {
    for (const node of nodes) {
      const flatNode = flatList.value.find((n) => n.id === node.id);
      const expanded = flatNode ? flatNode.expanded : true;
      const hasChildren = node.children && node.children.length > 0;

      result.push({
        id: node.id,
        name: node.name,
        depth: depth,
        hasChildren: hasChildren,
        expanded: expanded,
      });

      if (hasChildren && expanded) {
        walk(node.children, depth + 1);
      }
    }
  }

  walk(treeData.value, 0);
  flatList.value = result;
}

// 暴露 show 方法给父组件调用
defineExpose({ show });
</script>

<template>
  <!-- 使用 Teleport 将对话框渲染到 body，确保层级最高 -->
  <Teleport to="body">
    <!-- 遮罩层：点击遮罩层等同于点击"取消" -->
    <div
      v-if="visible"
      class="favorite-select-overlay"
      @click.self="onCancel"
    >
      <!-- 对话框主体 -->
      <div class="favorite-select-dialog" role="dialog" aria-modal="true">
        <!-- 标题 -->
        <h3 class="favorite-select-title">选择收藏目录</h3>

        <!-- 提示信息 -->
        <p class="favorite-select-message">
          请选择要将文件添加到哪个收藏目录。
        </p>

        <!-- 目录树区域 -->
        <div class="favorite-select-tree">
          <!-- 空状态提示 -->
          <p v-if="flatList.length === 0" class="favorite-select-empty">
            暂无收藏目录，请先新建目录。
          </p>

          <!-- 扁平化目录列表 -->
          <div
            v-for="item in flatList"
            :key="item.id"
            class="favorite-tree-item-row"
            :class="{
              'favorite-tree-item-row--selected': selectedId === item.id,
            }"
            :style="{ paddingLeft: 8 + item.depth * 20 + 'px' }"
            @click="onSelectNode(item.id)"
          >
            <!-- 展开/折叠箭头（仅在有子节点时显示） -->
            <span
              v-if="item.hasChildren"
              class="favorite-tree-arrow"
              :class="{ 'favorite-tree-arrow--expanded': item.expanded }"
              @click.stop="toggleExpand(item.id)"
            >
              ▶
            </span>
            <!-- 无子节点时占位 -->
            <span
              v-else
              class="favorite-tree-arrow favorite-tree-arrow--placeholder"
            ></span>

            <!-- 目录图标 -->
            <span class="favorite-tree-icon">
              <svg
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                stroke-linecap="round"
                stroke-linejoin="round"
                width="16"
                height="16"
              >
                <path
                  d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"
                />
              </svg>
            </span>

            <!-- 节点名称 -->
            <span class="favorite-tree-name">{{ item.name }}</span>
          </div>
        </div>

        <!-- 新建目录区域 -->
        <div class="favorite-select-create">
          <!-- 新建目录输入框 -->
          <div v-if="isCreating" class="favorite-create-input-row">
            <input
              v-model="newDirName"
              type="text"
              class="favorite-create-input"
              placeholder="请输入目录名称"
              maxlength="50"
              @keyup.enter="onConfirmCreate"
              @keyup.escape="onCancelCreate"
            />
            <button
              class="favorite-create-btn favorite-create-btn--confirm"
              @click="onConfirmCreate"
            >
              确定
            </button>
            <button
              class="favorite-create-btn favorite-create-btn--cancel"
              @click="onCancelCreate"
            >
              取消
            </button>
          </div>
          <!-- 错误提示 -->
          <p v-if="createError" class="favorite-create-error">
            {{ createError }}
          </p>
          <!-- 新建目录按钮 -->
          <button
            v-if="!isCreating"
            class="favorite-select-new-btn"
            @click="onStartCreate"
          >
            + 新建目录
          </button>
        </div>

        <!-- 按钮区域 -->
        <div class="favorite-select-buttons">
          <!-- 取消按钮 -->
          <button
            class="favorite-select-btn favorite-select-btn--cancel"
            @click="onCancel"
          >
            取消
          </button>

          <!-- 确认按钮（主操作，未选中目录时禁用） -->
          <button
            class="favorite-select-btn favorite-select-btn--confirm"
            :disabled="selectedId === null"
            @click="onConfirm"
          >
            确认
          </button>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
/* ==================== 遮罩层 ==================== */

.favorite-select-overlay {
  position: fixed;
  inset: 0;
  z-index: 10000;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(0, 0, 0, 0.5);
  animation: favorite-overlay-fade-in 0.15s ease-out;
}

@keyframes favorite-overlay-fade-in {
  from {
    opacity: 0;
  }
  to {
    opacity: 1;
  }
}

/* ==================== 对话框主体 ==================== */

.favorite-select-dialog {
  width: 420px;
  max-width: 90vw;
  max-height: 80vh;
  display: flex;
  flex-direction: column;
  padding: 28px 32px;
  border-radius: 12px;
  background: var(--color-bg-primary, #ffffff);
  box-shadow: 0 20px 60px rgba(0, 0, 0, 0.3);
  animation: favorite-dialog-scale-in 0.15s ease-out;
}

@keyframes favorite-dialog-scale-in {
  from {
    transform: scale(0.95);
    opacity: 0;
  }
  to {
    transform: scale(1);
    opacity: 1;
  }
}

/* ==================== 标题与文本 ==================== */

.favorite-select-title {
  margin: 0 0 8px;
  font-size: 17px;
  font-weight: 600;
  color: var(--color-text-primary, #1a1a1a);
  text-align: center;
}

.favorite-select-message {
  margin: 0 0 16px;
  font-size: 13px;
  color: var(--color-text-tertiary, #999999);
  text-align: center;
}

/* ==================== 目录树区域 ==================== */

.favorite-select-tree {
  flex: 1;
  overflow-y: auto;
  min-height: 60px;
  max-height: 320px;
  margin-bottom: 12px;
  padding: 8px;
  border: 1px solid var(--color-border, #d9d9d9);
  border-radius: 6px;
  background: var(--color-bg-secondary, #fafafa);
}

/* 空状态提示 */
.favorite-select-empty {
  margin: 0;
  padding: 20px 0;
  text-align: center;
  font-size: 13px;
  color: var(--color-text-tertiary, #999999);
}

/* ==================== 目录树节点行 ==================== */

.favorite-tree-item-row {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 6px 8px;
  border-radius: 4px;
  cursor: pointer;
  transition: background-color 0.12s ease;
  user-select: none;
}

.favorite-tree-item-row:hover {
  background: var(--color-bg-hover, #e8e8e8);
}

.favorite-tree-item-row--selected {
  background: rgba(64, 158, 255, 0.12);
  outline: 1px solid rgba(64, 158, 255, 0.4);
}

.favorite-tree-item-row--selected:hover {
  background: rgba(64, 158, 255, 0.18);
}

/* ==================== 展开/折叠箭头 ==================== */

.favorite-tree-arrow {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 16px;
  height: 16px;
  font-size: 10px;
  color: var(--color-text-tertiary, #999999);
  transition: transform 0.15s ease;
  cursor: pointer;
  flex-shrink: 0;
}

.favorite-tree-arrow--expanded {
  transform: rotate(90deg);
}

.favorite-tree-arrow--placeholder {
  visibility: hidden;
  pointer-events: none;
}

/* ==================== 目录图标 ==================== */

.favorite-tree-icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  color: #e6a23c;
}

/* ==================== 节点名称 ==================== */

.favorite-tree-name {
  font-size: 13px;
  color: var(--color-text-primary, #1a1a1a);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* ==================== 新建目录区域 ==================== */

.favorite-select-create {
  margin-bottom: 16px;
}

/* 新建目录按钮 */
.favorite-select-new-btn {
  display: block;
  width: 100%;
  padding: 6px 0;
  border: 1px dashed var(--color-border, #d9d9d9);
  border-radius: 4px;
  background: transparent;
  color: var(--color-text-secondary, #666666);
  font-size: 13px;
  cursor: pointer;
  transition: all 0.15s ease;
}

.favorite-select-new-btn:hover {
  border-color: #409eff;
  color: #409eff;
  background: rgba(64, 158, 255, 0.04);
}

/* 新建目录输入框行 */
.favorite-create-input-row {
  display: flex;
  gap: 6px;
  align-items: center;
}

.favorite-create-input {
  flex: 1;
  padding: 6px 10px;
  border: 1px solid var(--color-border, #d9d9d9);
  border-radius: 4px;
  font-size: 13px;
  color: var(--color-text-primary, #1a1a1a);
  background: var(--color-bg-primary, #ffffff);
  outline: none;
  transition: border-color 0.15s ease;
}

.favorite-create-input:focus {
  border-color: #409eff;
}

.favorite-create-input::placeholder {
  color: var(--color-text-tertiary, #999999);
}

/* 新建目录操作按钮 */
.favorite-create-btn {
  padding: 6px 12px;
  border: 1px solid transparent;
  border-radius: 4px;
  font-size: 12px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.15s ease;
  white-space: nowrap;
}

.favorite-create-btn--confirm {
  background: #409eff;
  color: #ffffff;
  border-color: #409eff;
}

.favorite-create-btn--confirm:hover {
  background: #337ecc;
}

.favorite-create-btn--cancel {
  background: var(--color-bg-tertiary, #f0f0f0);
  color: var(--color-text-secondary, #666666);
  border-color: var(--color-border, #d9d9d9);
}

.favorite-create-btn--cancel:hover {
  background: var(--color-bg-hover, #e0e0e0);
}

/* 新建目录错误提示 */
.favorite-create-error {
  margin: 6px 0 0;
  font-size: 12px;
  color: #e74c3c;
}

/* ==================== 按钮区域 ==================== */

.favorite-select-buttons {
  display: flex;
  justify-content: center;
  gap: 10px;
}

/* ==================== 按钮基础样式 ==================== */

.favorite-select-btn {
  padding: 8px 20px;
  border: 1px solid transparent;
  border-radius: 6px;
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.15s ease;
  outline: none;
}

.favorite-select-btn:focus-visible {
  box-shadow: 0 0 0 2px var(--color-focus-ring, rgba(64, 150, 255, 0.4));
}

/* ==================== "取消"按钮 ==================== */

.favorite-select-btn--cancel {
  background: var(--color-bg-tertiary, #f0f0f0);
  color: var(--color-text-secondary, #666666);
  border-color: var(--color-border, #d9d9d9);
}

.favorite-select-btn--cancel:hover {
  background: var(--color-bg-hover, #e0e0e0);
}

/* ==================== "确认"按钮（主操作） ==================== */

.favorite-select-btn--confirm {
  background: #409eff;
  color: #ffffff;
  border-color: #409eff;
}

.favorite-select-btn--confirm:hover {
  background: #337ecc;
  border-color: #337ecc;
}

.favorite-select-btn--confirm:active {
  background: #2a6cb3;
  border-color: #2a6cb3;
}

.favorite-select-btn--confirm:disabled {
  background: #a0cfff;
  border-color: #a0cfff;
  cursor: not-allowed;
  opacity: 0.7;
}
</style>