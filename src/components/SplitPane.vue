<script setup lang="ts">
/**
 * SplitPane 组件 - 双屏分屏布局
 *
 * 功能：
 * - 支持左右分屏布局，左侧显示源码编辑区，右侧显示预览区
 * - 中间有可拖拽的分隔条（divider），用户拖动时可改变左右区域宽度比例
 * - 使用 Vue 3 Composition API
 * - 通过 slot 让父组件传入左侧和右侧内容
 */

import { ref, onUnmounted } from "vue";

// ==================== 状态定义 ====================

/** 左侧面板宽度比例（百分比），默认各占 50% */
const splitRatio = ref<number>(50);

/** 是否正在拖拽分隔条 */
const isDragging = ref<boolean>(false);

/** 容器 DOM 引用，用于计算拖拽位置 */
const containerRef = ref<HTMLElement | null>(null);

// ==================== 拖拽逻辑 ====================

/**
 * 处理分隔条 mousedown 事件：开始拖拽
 * 记录拖拽开始状态，并注册全局 mousemove 和 mouseup 事件
 */
function onDividerMouseDown(e: MouseEvent): void {
  // 阻止默认行为和文本选择
  e.preventDefault();
  isDragging.value = true;

  // 在 document 上注册全局事件，确保鼠标移出分隔条后仍能继续拖拽
  document.addEventListener("mousemove", onMouseMove);
  document.addEventListener("mouseup", onMouseUp);

  // 设置拖拽时的全局样式，防止文本被选中
  document.body.style.userSelect = "none";
  document.body.style.cursor = "col-resize";
}

/**
 * 处理 mousemove 事件：更新左右区域宽度比例
 * 根据鼠标在容器内的相对位置计算新的比例
 */
function onMouseMove(e: MouseEvent): void {
  if (!isDragging.value || !containerRef.value) return;

  // 获取容器的位置和尺寸
  const containerRect = containerRef.value.getBoundingClientRect();

  // 计算鼠标相对于容器左侧的偏移量，转换为百分比
  let newRatio = ((e.clientX - containerRect.left) / containerRect.width) * 100;

  // 限制比例范围在 20% ~ 80% 之间，防止一侧被完全压缩
  newRatio = Math.max(20, Math.min(80, newRatio));

  splitRatio.value = newRatio;
}

/**
 * 处理 mouseup 事件：结束拖拽
 * 清理全局事件监听器和拖拽状态
 */
function onMouseUp(): void {
  if (!isDragging.value) return;

  isDragging.value = false;

  // 移除全局事件监听器
  document.removeEventListener("mousemove", onMouseMove);
  document.removeEventListener("mouseup", onMouseUp);

  // 恢复默认样式
  document.body.style.userSelect = "";
  document.body.style.cursor = "";
}

// ==================== 生命周期 ====================

/**
 * 组件卸载时清理可能残留的全局事件监听器
 * 防止在拖拽过程中组件被销毁导致事件泄漏
 */
onUnmounted(() => {
  document.removeEventListener("mousemove", onMouseMove);
  document.removeEventListener("mouseup", onMouseUp);
  document.body.style.userSelect = "";
  document.body.style.cursor = "";
});
</script>

<template>
  <!-- 分屏容器，使用 ref 获取 DOM 引用 -->
  <div ref="containerRef" class="split-pane">
    <!-- 左侧面板 -->
    <div class="split-pane__left" :style="{ width: splitRatio + '%' }">
      <slot name="left" />
    </div>

    <!-- 可拖拽分隔条（divider） -->
    <div
      class="split-pane__divider"
      :class="{ 'split-pane__divider--dragging': isDragging }"
      @mousedown="onDividerMouseDown"
    />

    <!-- 右侧面板 -->
    <div class="split-pane__right" :style="{ width: (100 - splitRatio) + '%' }">
      <slot name="right" />
    </div>
  </div>
</template>

<style scoped>
/* ==================== 分屏容器样式 ==================== */

.split-pane {
  /* 横向 flex 布局 */
  display: flex;
  flex-direction: row;

  /* 撑满父容器 */
  width: 100%;
  height: 100%;

  /* 防止内容溢出 */
  overflow: hidden;
}

/* ==================== 左右面板样式 ==================== */

.split-pane__left,
.split-pane__right {
  /* 撑满高度 */
  height: 100%;

  /* 防止内容溢出 */
  overflow: hidden;

  /* 使用 flex-shrink: 0 防止被压缩 */
  flex-shrink: 0;
}

/* ==================== 分隔条样式 ==================== */

.split-pane__divider {
  /* 分隔条宽度 */
  width: 4px;
  min-width: 4px;

  /* 撑满高度 */
  height: 100%;

  /* 使用主题 CSS 变量 */
  background-color: var(--border-color);

  /* 鼠标样式 */
  cursor: col-resize;

  /* 过渡动画 */
  transition: background-color 0.2s ease;

  /* 防止被 flex 压缩 */
  flex-shrink: 0;
}

/* 分隔条 hover 状态 */
.split-pane__divider:hover {
  background-color: var(--link-color);
}

/* 分隔条拖拽状态 */
.split-pane__divider--dragging {
  background-color: var(--link-color);
}
</style>