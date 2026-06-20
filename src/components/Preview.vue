<script setup lang="ts">
/**
 * Preview 组件 - Markdown 预览渲染器
 * 
 * 功能：
 * - 接收父组件传入的 HTML 字符串
 * - 使用 v-html 在安全沙箱中渲染解析后的 Markdown HTML
 * - 为预览内容提供美观的排版样式
 */

// ==================== Props 定义 ====================

/** 解析后的 HTML 字符串，由父组件通过 Tauri IPC 获取 */
defineProps<{
  /** Markdown 解析后的 HTML 内容 */
  html: string;
}>();
</script>

<template>
  <div class="preview-container">
    <!-- 预览内容区域，使用 v-html 渲染 HTML -->
    <div class="preview-content markdown-body" v-html="html" />
  </div>
</template>

<style scoped>
/* ==================== 预览容器样式 ==================== */

.preview-container {
  /* 撑满父容器 */
  display: flex;
  flex-direction: column;
  height: 100%;
  width: 100%;

  /* 支持内容溢出时滚动 */
  overflow-y: auto;

  /* 使用主题 CSS 变量 */
  background-color: var(--preview-bg-color);
  color: var(--text-color);

  /* 平滑过渡 */
  transition: background-color 0.3s ease, color 0.3s ease;
}

.preview-content {
  padding: 24px 32px;
  max-width: 860px;
  margin: 0 auto;
  width: 100%;
}

/* ==================== 滚动条样式 ==================== */

.preview-container::-webkit-scrollbar {
  width: 8px;
}

.preview-container::-webkit-scrollbar-track {
  background: var(--preview-bg-color);
}

.preview-container::-webkit-scrollbar-thumb {
  background-color: var(--border-color);
  border-radius: 4px;
}

/* ==================== Markdown 内容样式 ==================== */

/* 标题样式 */
.markdown-body :deep(h1) {
  font-size: 2em;
  font-weight: 700;
  margin: 0.67em 0 0.5em;
  padding-bottom: 0.3em;
  border-bottom: 1px solid var(--border-color);
  color: var(--heading-color);
}

.markdown-body :deep(h2) {
  font-size: 1.5em;
  font-weight: 600;
  margin: 0.83em 0 0.5em;
  padding-bottom: 0.25em;
  border-bottom: 1px solid var(--border-color);
  color: var(--heading-color);
}

.markdown-body :deep(h3) {
  font-size: 1.25em;
  font-weight: 600;
  margin: 1em 0 0.5em;
  color: var(--heading-color);
}

.markdown-body :deep(h4) {
  font-size: 1.1em;
  font-weight: 600;
  margin: 1em 0 0.5em;
  color: var(--heading-color);
}

.markdown-body :deep(h5),
.markdown-body :deep(h6) {
  font-size: 1em;
  font-weight: 600;
  margin: 1em 0 0.5em;
  color: var(--heading-color);
}

/* 段落样式 */
.markdown-body :deep(p) {
  margin: 0.8em 0;
  line-height: 1.75;
}

/* 链接样式 */
.markdown-body :deep(a) {
  color: var(--link-color);
  text-decoration: none;
  border-bottom: 1px solid transparent;
  transition: border-color 0.2s ease;
}

.markdown-body :deep(a:hover) {
  border-bottom-color: var(--link-color);
}

/* 代码块样式 */
.markdown-body :deep(pre) {
  background-color: var(--code-bg-color);
  border: 1px solid var(--border-color);
  border-radius: 8px;
  padding: 16px 20px;
  overflow-x: auto;
  margin: 1em 0;
  font-family: "Cascadia Code", "Fira Code", "JetBrains Mono", "Consolas",
    "Monaco", monospace;
  font-size: 13px;
  line-height: 1.6;
}

.markdown-body :deep(pre code) {
  background: none;
  padding: 0;
  border-radius: 0;
  font-size: inherit;
}

/* 行内代码样式 */
.markdown-body :deep(code) {
  background-color: var(--code-bg-color);
  padding: 2px 6px;
  border-radius: 4px;
  font-family: "Cascadia Code", "Fira Code", "JetBrains Mono", "Consolas",
    "Monaco", monospace;
  font-size: 0.9em;
  color: var(--code-text-color);
}

/* 引用块样式 */
.markdown-body :deep(blockquote) {
  margin: 1em 0;
  padding: 8px 16px;
  border-left: 4px solid var(--link-color);
  background-color: var(--blockquote-bg-color);
  border-radius: 0 6px 6px 0;
  color: var(--text-color);
}

.markdown-body :deep(blockquote p) {
  margin: 0.4em 0;
}

/* 列表样式 */
.markdown-body :deep(ul),
.markdown-body :deep(ol) {
  padding-left: 1.8em;
  margin: 0.8em 0;
}

.markdown-body :deep(li) {
  margin: 0.3em 0;
  line-height: 1.7;
}

/* 表格样式 */
.markdown-body :deep(table) {
  border-collapse: collapse;
  width: 100%;
  margin: 1em 0;
  border: 1px solid var(--border-color);
}

.markdown-body :deep(th),
.markdown-body :deep(td) {
  border: 1px solid var(--border-color);
  padding: 8px 12px;
  text-align: left;
}

.markdown-body :deep(th) {
  background-color: var(--code-bg-color);
  font-weight: 600;
}

.markdown-body :deep(tr:nth-child(even)) {
  background-color: var(--table-stripe-color);
}

/* 水平线样式 */
.markdown-body :deep(hr) {
  border: none;
  border-top: 1px solid var(--border-color);
  margin: 1.5em 0;
}

/* 图片样式 */
.markdown-body :deep(img) {
  max-width: 100%;
  border-radius: 6px;
  margin: 1em 0;
}

/* 强调样式 */
.markdown-body :deep(strong) {
  font-weight: 600;
  color: var(--heading-color);
}

.markdown-body :deep(em) {
  font-style: italic;
}
</style>