<script setup lang="ts">
/**
 * Editor 组件 - Markdown 源代码编辑器
 * 
 * 功能：
 * - 提供 textarea 文本编辑区域，用于编辑 Markdown 源代码
 * - 通过 v-model 双向绑定内容，支持父组件同步更新
 * - 提供 placeholder 占位提示文字，引导用户输入
 */

// ==================== Props 定义 ====================

/** 编辑器内容模型，通过 v-model 与父组件双向绑定 */
const modelValue = defineModel<string>("modelValue", { required: true });

/** 编辑器空白时的占位提示文字 */
withDefaults(
  defineProps<{
    placeholder?: string;
  }>(),
  {
    placeholder: "请输入 Markdown 内容...",
  }
);

// ==================== Emits 定义 ====================

/**
 * 当编辑器内容变化时发出的事件
 * 支持 v-model 的双向绑定机制
 */
defineEmits<{
  "update:modelValue": [value: string];
}>();
</script>

<template>
  <div class="editor-container">
    <!-- Markdown 源代码编辑区域 -->
    <textarea
      class="editor-textarea"
      :value="modelValue"
      :placeholder="placeholder"
      @input="
        $emit('update:modelValue', ($event.target as HTMLTextAreaElement).value)
      "
    />
  </div>
</template>

<style scoped>
/* ==================== 编辑器容器样式 ==================== */

.editor-container {
  /* 撑满父容器，确保编辑器占据全部可用空间 */
  display: flex;
  flex-direction: column;
  height: 100%;
  width: 100%;
}

/* ==================== 编辑器文本框样式 ==================== */

.editor-textarea {
  /* 撑满容器 */
  flex: 1;
  width: 100%;

  /* 移除默认样式 */
  border: none;
  outline: none;
  resize: none;

  /* 内边距与字体样式 */
  padding: 16px 20px;
  font-family: "Cascadia Code", "Fira Code", "JetBrains Mono", "Consolas",
    "Monaco", monospace;
  font-size: 14px;
  line-height: 1.7;

  /* 使用主题 CSS 变量控制颜色 */
  color: var(--editor-text-color);
  background-color: var(--editor-bg-color);

  /* 平滑过渡动画 */
  transition: background-color 0.3s ease, color 0.3s ease;
}

/* 占位文字样式 */
.editor-textarea::placeholder {
  color: var(--editor-placeholder-color);
  opacity: 0.6;
}

/* 滚动条样式 */
.editor-textarea::-webkit-scrollbar {
  width: 8px;
}

.editor-textarea::-webkit-scrollbar-track {
  background: var(--editor-bg-color);
}

.editor-textarea::-webkit-scrollbar-thumb {
  background-color: var(--border-color);
  border-radius: 4px;
}
</style>