<script setup lang="ts">
/**
 * CloseConfirmDialog 组件 - 关闭确认对话框
 *
 * 当用户关闭窗口但文档有未保存的更改时，弹出此对话框。
 * 提供三个选项：
 * - 保存并关闭：保存文档后关闭窗口
 * - 不保存：放弃更改，直接关闭窗口（红色警告样式）
 * - 取消：取消关闭操作，返回编辑
 *
 * 使用 Promise 模式与父组件交互：父组件通过 show() 方法显示对话框，
 * 该方法返回 Promise，resolve 为用户的选项。
 */

import { ref } from "vue";

// ==================== 响应式状态 ====================

/** 对话框是否可见 */
const visible = ref(false);

/** 当前等待中的 Promise resolve 函数 */
let resolvePromise: ((value: "save" | "discard" | "cancel") => void) | null =
  null;

// ==================== 公开方法 ====================

/**
 * 显示关闭确认对话框
 *
 * @returns Promise，resolve 为用户选择的选项：
 *   - "save"：保存并关闭
 *   - "discard"：不保存直接关闭
 *   - "cancel"：取消关闭
 */
function show(): Promise<"save" | "discard" | "cancel"> {
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

/** 用户点击"保存并关闭" */
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
      class="close-confirm-overlay"
      @click.self="onCancel"
    >
      <!-- 对话框主体 -->
      <div class="close-confirm-dialog" role="dialog" aria-modal="true">
        <!-- 警告图标区域 -->
        <div class="close-confirm-icon">
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
        <h3 class="close-confirm-title">关闭 MarkStudio</h3>

        <!-- 提示信息 -->
        <p class="close-confirm-message">
          当前文档尚未保存，关闭后将丢失所有更改。
        </p>
        <p class="close-confirm-submessage">
          请选择是否保存后再关闭。
        </p>

        <!-- 按钮区域 -->
        <div class="close-confirm-buttons">
          <!-- 不保存按钮（红色警告样式） -->
          <button
            class="close-confirm-btn close-confirm-btn--discard"
            @click="onDiscard"
          >
            不保存
          </button>

          <!-- 取消按钮 -->
          <button
            class="close-confirm-btn close-confirm-btn--cancel"
            @click="onCancel"
          >
            取消
          </button>

          <!-- 保存并关闭按钮（主操作） -->
          <button
            class="close-confirm-btn close-confirm-btn--save"
            @click="onSave"
          >
            保存并关闭
          </button>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
/* ==================== 遮罩层 ==================== */

.close-confirm-overlay {
  position: fixed;
  inset: 0;
  z-index: 9999;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(0, 0, 0, 0.5);
  animation: overlay-fade-in 0.15s ease-out;
}

@keyframes overlay-fade-in {
  from {
    opacity: 0;
  }
  to {
    opacity: 1;
  }
}

/* ==================== 对话框主体 ==================== */

.close-confirm-dialog {
  width: 420px;
  max-width: 90vw;
  padding: 28px 32px;
  border-radius: 12px;
  background: var(--color-bg-primary, #ffffff);
  box-shadow: 0 20px 60px rgba(0, 0, 0, 0.3);
  text-align: center;
  animation: dialog-scale-in 0.15s ease-out;
}

@keyframes dialog-scale-in {
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

.close-confirm-icon {
  display: flex;
  justify-content: center;
  margin-bottom: 16px;
}

.close-confirm-icon svg {
  width: 48px;
  height: 48px;
  color: #e6a23c;
}

/* ==================== 标题与文本 ==================== */

.close-confirm-title {
  margin: 0 0 12px;
  font-size: 17px;
  font-weight: 600;
  color: var(--color-text-primary, #1a1a1a);
}

.close-confirm-message {
  margin: 0 0 4px;
  font-size: 14px;
  line-height: 1.6;
  color: var(--color-text-secondary, #666666);
}

.close-confirm-submessage {
  margin: 0 0 24px;
  font-size: 13px;
  color: var(--color-text-tertiary, #999999);
}

/* ==================== 按钮区域 ==================== */

.close-confirm-buttons {
  display: flex;
  justify-content: center;
  gap: 10px;
}

/* ==================== 按钮基础样式 ==================== */

.close-confirm-btn {
  padding: 8px 20px;
  border: 1px solid transparent;
  border-radius: 6px;
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.15s ease;
  outline: none;
}

.close-confirm-btn:focus-visible {
  box-shadow: 0 0 0 2px var(--color-focus-ring, rgba(64, 150, 255, 0.4));
}

/* ==================== "不保存"按钮 —— 红色警告样式 ==================== */

.close-confirm-btn--discard {
  background: #e74c3c;
  color: #ffffff;
  border-color: #e74c3c;
}

.close-confirm-btn--discard:hover {
  background: #c0392b;
  border-color: #c0392b;
}

.close-confirm-btn--discard:active {
  background: #a93226;
  border-color: #a93226;
}

/* ==================== "取消"按钮 ==================== */

.close-confirm-btn--cancel {
  background: var(--color-bg-tertiary, #f0f0f0);
  color: var(--color-text-secondary, #666666);
  border-color: var(--color-border, #d9d9d9);
}

.close-confirm-btn--cancel:hover {
  background: var(--color-bg-hover, #e0e0e0);
}

/* ==================== "保存并关闭"按钮（主操作） ==================== */

.close-confirm-btn--save {
  background: #409eff;
  color: #ffffff;
  border-color: #409eff;
}

.close-confirm-btn--save:hover {
  background: #337ecc;
  border-color: #337ecc;
}

.close-confirm-btn--save:active {
  background: #2a6cb3;
  border-color: #2a6cb3;
}
</style>