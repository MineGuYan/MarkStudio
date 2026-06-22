<script setup lang="ts">
/**
 * TabSaveConfirmDialog 组件 - 标签页保存确认对话框
 *
 * 当用户关闭已修改但未保存的标签页时，弹出此对话框。
 * 提供三个选项：
 * - 保存：保存当前文档的更改
 * - 不保存：放弃更改，直接关闭标签页（红色警告样式）
 * - 取消：取消关闭操作，返回编辑
 *
 * 使用 Promise 模式与父组件交互：父组件通过 show(fileName) 方法显示对话框，
 * 该方法返回 Promise，resolve 为用户的选项。
 * 样式与 CloseConfirmDialog 保持一致。
 */

import { ref } from "vue";

// ==================== 响应式状态 ====================

/** 对话框是否可见 */
const visible = ref(false);

/** 当前正在确认的文件名 */
const fileName = ref("");

/** 当前等待中的 Promise resolve 函数 */
let resolvePromise: ((value: "save" | "discard" | "cancel") => void) | null =
  null;

// ==================== 公开方法 ====================

/**
 * 显示标签页保存确认对话框
 *
 * @param name - 需要确认保存的文件名
 * @returns Promise，resolve 为用户选择的选项：
 *   - "save"：保存更改
 *   - "discard"：不保存，放弃更改
 *   - "cancel"：取消关闭
 */
function show(name: string): Promise<"save" | "discard" | "cancel"> {
  fileName.value = name;
  visible.value = true;
  return new Promise<"save" | "discard" | "cancel">((resolve) => {
    resolvePromise = resolve;
  });
}

/** 隐藏对话框 */
function hide(): void {
  visible.value = false;
  resolvePromise = null;
}

// ==================== 按钮事件处理 ====================

/** 用户点击"保存" */
function onSave(): void {
  if (resolvePromise) {
    resolvePromise("save");
  }
  hide();
}

/** 用户点击"不保存"（红色警告按钮） */
function onDiscard(): void {
  if (resolvePromise) {
    resolvePromise("discard");
  }
  hide();
}

/** 用户点击"取消"或点击遮罩层 */
function onCancel(): void {
  if (resolvePromise) {
    resolvePromise("cancel");
  }
  hide();
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
      class="tab-save-confirm-overlay"
      @click.self="onCancel"
    >
      <!-- 对话框主体 -->
      <div class="tab-save-confirm-dialog" role="dialog" aria-modal="true">
        <!-- 警告图标区域 -->
        <div class="tab-save-confirm-icon">
          <!-- 三角形警告图标（SVG） -->
          <svg
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <path d="M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z" />
            <line x1="12" y1="9" x2="12" y2="13" />
            <line x1="12" y1="17" x2="12.01" y2="17" />
          </svg>
        </div>

        <!-- 标题 -->
        <h3 class="tab-save-confirm-title">保存更改？</h3>

        <!-- 提示信息：动态显示文件名 -->
        <p class="tab-save-confirm-message">
          是否保存对 <strong>{{ fileName }}</strong> 的更改？
        </p>
        <p class="tab-save-confirm-submessage">
          如果不保存，将丢失所有更改。
        </p>

        <!-- 按钮区域 -->
        <div class="tab-save-confirm-buttons">
          <!-- 不保存按钮（红色警告样式） -->
          <button
            class="tab-save-confirm-btn tab-save-confirm-btn--discard"
            @click="onDiscard"
          >
            不保存
          </button>

          <!-- 取消按钮 -->
          <button
            class="tab-save-confirm-btn tab-save-confirm-btn--cancel"
            @click="onCancel"
          >
            取消
          </button>

          <!-- 保存按钮（主操作） -->
          <button
            class="tab-save-confirm-btn tab-save-confirm-btn--save"
            @click="onSave"
          >
            保存
          </button>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
/* ==================== 遮罩层 ==================== */

.tab-save-confirm-overlay {
  position: fixed;
  inset: 0;
  z-index: 9999;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(0, 0, 0, 0.5);
  animation: tab-save-overlay-fade-in 0.15s ease-out;
}

@keyframes tab-save-overlay-fade-in {
  from {
    opacity: 0;
  }
  to {
    opacity: 1;
  }
}

/* ==================== 对话框主体 ==================== */

.tab-save-confirm-dialog {
  width: 420px;
  max-width: 90vw;
  padding: 28px 32px;
  border-radius: 12px;
  background: var(--color-bg-primary, #ffffff);
  box-shadow: 0 20px 60px rgba(0, 0, 0, 0.3);
  text-align: center;
  animation: tab-save-dialog-scale-in 0.15s ease-out;
}

@keyframes tab-save-dialog-scale-in {
  from {
    transform: scale(0.95);
    opacity: 0;
  }
  to {
    transform: scale(1);
    opacity: 1;
  }
}

/* ==================== 警告图标 ==================== */

.tab-save-confirm-icon {
  display: flex;
  justify-content: center;
  margin-bottom: 16px;
}

.tab-save-confirm-icon svg {
  width: 48px;
  height: 48px;
  color: #e6a23c;
}

/* ==================== 标题与文本 ==================== */

.tab-save-confirm-title {
  margin: 0 0 12px;
  font-size: 17px;
  font-weight: 600;
  color: var(--color-text-primary, #1a1a1a);
}

.tab-save-confirm-message {
  margin: 0 0 4px;
  font-size: 14px;
  line-height: 1.6;
  color: var(--color-text-secondary, #666666);
}

.tab-save-confirm-message strong {
  color: var(--color-text-primary, #1a1a1a);
  font-weight: 600;
}

.tab-save-confirm-submessage {
  margin: 0 0 24px;
  font-size: 13px;
  color: var(--color-text-tertiary, #999999);
}

/* ==================== 按钮区域 ==================== */

.tab-save-confirm-buttons {
  display: flex;
  justify-content: center;
  gap: 10px;
}

/* ==================== 按钮基础样式 ==================== */

.tab-save-confirm-btn {
  padding: 8px 20px;
  border: 1px solid transparent;
  border-radius: 6px;
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.15s ease;
  outline: none;
}

.tab-save-confirm-btn:focus-visible {
  box-shadow: 0 0 0 2px var(--color-focus-ring, rgba(64, 150, 255, 0.4));
}

/* ==================== "不保存"按钮 —— 红色警告样式 ==================== */

.tab-save-confirm-btn--discard {
  background: #e74c3c;
  color: #ffffff;
  border-color: #e74c3c;
}

.tab-save-confirm-btn--discard:hover {
  background: #c0392b;
  border-color: #c0392b;
}

.tab-save-confirm-btn--discard:active {
  background: #a93226;
  border-color: #a93226;
}

/* ==================== "取消"按钮 ==================== */

.tab-save-confirm-btn--cancel {
  background: var(--color-bg-tertiary, #f0f0f0);
  color: var(--color-text-secondary, #666666);
  border-color: var(--color-border, #d9d9d9);
}

.tab-save-confirm-btn--cancel:hover {
  background: var(--color-bg-hover, #e0e0e0);
}

/* ==================== "保存"按钮（主操作） ==================== */

.tab-save-confirm-btn--save {
  background: #409eff;
  color: #ffffff;
  border-color: #409eff;
}

.tab-save-confirm-btn--save:hover {
  background: #337ecc;
  border-color: #337ecc;
}

.tab-save-confirm-btn--save:active {
  background: #2a6cb3;
  border-color: #2a6cb3;
}
</style>