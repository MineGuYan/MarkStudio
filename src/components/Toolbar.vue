<script setup lang="ts">
/**
 * Toolbar 组件 - 顶部工具栏
 * 
 * 功能：
 * - 提供编辑模式切换：源代码（source）/ 预览（preview）/ 双屏（split）
 * - 提供主题切换按钮：浅色（light）/ 深色（dark）
 * - 通过 emit 向父组件发出模式切换和主题切换事件
 */

// ==================== Props 定义 ====================

/** 当前编辑模式和主题 */
const props = defineProps<{
  /** 当前模式：source 为源代码编辑，preview 为预览模式，split 为双屏模式 */
  mode: "source" | "preview" | "split";
  /** 当前主题：light 为浅色主题，dark 为深色主题 */
  theme: "light" | "dark";
  /** 协作连接状态 */
  collabConnected?: boolean;
}>();

// ==================== Emits 定义 ====================

/** 发出事件通知父组件进行状态更新 */
const emit = defineEmits<{
  /** 切换编辑模式 */
  "update:mode": [mode: "source" | "preview" | "split"];
  /** 切换主题 */
  "update:theme": [theme: "light" | "dark"];
  /** 打开文件 */
  "open-file": [];
  /** 保存文件 */
  "save-file": [];
  /** 切换协作面板显示/隐藏 */
  "toggle-collab": [];
}>();

// ==================== 方法定义 ====================

/**
 * 设置编辑模式
 * 直接发出目标模式，由父组件响应更新
 * 
 * @param mode - 目标编辑模式
 */
function setMode(mode: "source" | "preview" | "split"): void {
  emit("update:mode", mode);
}

/** 切换主题：在浅色与深色之间切换 */
function toggleTheme(): void {
  emit("update:theme", props.theme === "light" ? "dark" : "light");
}
</script>

<template>
  <header class="toolbar">
    <!-- 左侧：应用标题 -->
    <div class="toolbar-left">
      <span class="toolbar-title">📝 MarkStudio</span>
    </div>

    <!-- 右侧：功能按钮组 -->
    <div class="toolbar-right">
      <!-- 编辑模式切换按钮组 -->
      <div class="toolbar-group">
        <!-- 源代码模式按钮 -->
        <button
          class="toolbar-btn"
          :class="{ active: mode === 'source' }"
          title="源代码编辑模式"
          @click="setMode('source')"
        >
          <!-- 源代码图标 -->
          <svg
            class="btn-icon"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
          >
            <polyline points="16 18 22 12 16 6" />
            <polyline points="8 6 2 12 8 18" />
          </svg>
          <span>源代码</span>
        </button>

        <!-- 双屏模式按钮 -->
        <button
          class="toolbar-btn"
          :class="{ active: mode === 'split' }"
          title="双屏模式（左编辑右预览）"
          @click="setMode('split')"
        >
          <!-- 双屏图标（左右分栏） -->
          <svg
            class="btn-icon"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
          >
            <rect x="3" y="3" width="18" height="18" rx="2" />
            <line x1="12" y1="3" x2="12" y2="21" />
          </svg>
          <span>双屏</span>
        </button>

        <!-- 预览模式按钮 -->
        <button
          class="toolbar-btn"
          :class="{ active: mode === 'preview' }"
          title="预览模式"
          @click="setMode('preview')"
        >
          <!-- 预览图标（眼睛） -->
          <svg
            class="btn-icon"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
          >
            <path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z" />
            <circle cx="12" cy="12" r="3" />
          </svg>
          <span>预览</span>
        </button>
      </div>

      <!-- 分隔线 -->
      <div class="toolbar-divider" />

      <!-- 文件操作按钮组 -->
      <div class="toolbar-group">
        <!-- 打开文件按钮 -->
        <button
          class="toolbar-btn"
          title="打开 Markdown 文件（.md/.markdown/.txt）"
          @click="emit('open-file')"
        >
          <!-- 打开文件夹图标 -->
          <svg
            class="btn-icon"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
          >
            <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" />
          </svg>
          <span>打开</span>
        </button>

        <!-- 保存文件按钮 -->
        <button
          class="toolbar-btn"
          title="保存文件（Ctrl+S）"
          @click="emit('save-file')"
        >
          <!-- 保存图标（软盘） -->
          <svg
            class="btn-icon"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
          >
            <path d="M19 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11l5 5v11a2 2 0 0 1-2 2z" />
            <polyline points="17 21 17 13 7 13 7 21" />
            <polyline points="7 3 7 8 15 8" />
          </svg>
          <span>保存</span>
        </button>
      </div>

      <!-- 分隔线 -->
      <div class="toolbar-divider" />

      <!-- 协作按钮 -->
      <button
        class="toolbar-btn"
        :class="{ active: collabConnected }"
        title="协作编辑"
        @click="emit('toggle-collab')"
      >
        <!-- 对话气泡图标 -->
        <svg
          class="btn-icon"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
        >
          <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" />
        </svg>
        <span>协作</span>
      </button>

      <!-- 分隔线 -->
      <div class="toolbar-divider" />

      <!-- 主题切换按钮 -->
      <button
        class="toolbar-btn theme-btn"
        title="切换主题"
        @click="toggleTheme"
      >
        <!-- 浅色主题图标（太阳） -->
        <svg
          v-if="theme === 'dark'"
          class="btn-icon"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
        >
          <circle cx="12" cy="12" r="5" />
          <line x1="12" y1="1" x2="12" y2="3" />
          <line x1="12" y1="21" x2="12" y2="23" />
          <line x1="4.22" y1="4.22" x2="5.64" y2="5.64" />
          <line x1="18.36" y1="18.36" x2="19.78" y2="19.78" />
          <line x1="1" y1="12" x2="3" y2="12" />
          <line x1="21" y1="12" x2="23" y2="12" />
          <line x1="4.22" y1="19.78" x2="5.64" y2="18.36" />
          <line x1="18.36" y1="5.64" x2="19.78" y2="4.22" />
        </svg>
        <!-- 深色主题图标（月亮） -->
        <svg
          v-else
          class="btn-icon"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
        >
          <path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z" />
        </svg>
      </button>
    </div>
  </header>
</template>

<style scoped>
/* ==================== 工具栏整体样式 ==================== */

.toolbar {
  /* 横向布局，左右两端对齐 */
  display: flex;
  align-items: center;
  justify-content: space-between;

  /* 尺寸与间距 */
  height: 48px;
  padding: 0 16px;
  flex-shrink: 0;

  /* 使用主题 CSS 变量 */
  background-color: var(--toolbar-bg-color);
  border-bottom: 1px solid var(--border-color);

  /* 过渡动画 */
  transition: background-color 0.3s ease, border-color 0.3s ease;

  /* 防止文本被选中 */
  user-select: none;
}

/* ==================== 左侧区域 ==================== */

.toolbar-left {
  display: flex;
  align-items: center;
}

.toolbar-title {
  font-size: 15px;
  font-weight: 600;
  color: var(--heading-color);
  letter-spacing: 0.5px;
}

/* ==================== 右侧区域 ==================== */

.toolbar-right {
  display: flex;
  align-items: center;
  gap: 4px;
}

/* 按钮分组 */
.toolbar-group {
  display: flex;
  align-items: center;
  gap: 2px;
  background-color: var(--button-group-bg);
  border-radius: 6px;
  padding: 2px;
}

/* 分隔线 */
.toolbar-divider {
  width: 1px;
  height: 22px;
  background-color: var(--border-color);
  margin: 0 8px;
}

/* ==================== 按钮基础样式 ==================== */

.toolbar-btn {
  /* 布局 */
  display: flex;
  align-items: center;
  gap: 5px;

  /* 尺寸与样式 */
  height: 32px;
  padding: 0 10px;
  border: none;
  border-radius: 5px;
  background: transparent;
  cursor: pointer;

  /* 文字样式 */
  font-size: 13px;
  font-weight: 500;
  color: var(--text-color);
  white-space: nowrap;

  /* 过渡动画 */
  transition: all 0.2s ease;
}

.toolbar-btn:hover {
  background-color: var(--button-hover-bg);
}

.toolbar-btn:active {
  transform: scale(0.96);
}

/* 激活状态的按钮 */
.toolbar-btn.active {
  background-color: var(--button-active-bg);
  color: var(--button-active-text);
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.1);
}

/* 主题切换按钮 */
.theme-btn {
  width: 34px;
  height: 34px;
  padding: 0;
  justify-content: center;
  border-radius: 6px;
}

/* ==================== 图标样式 ==================== */

.btn-icon {
  width: 16px;
  height: 16px;
  flex-shrink: 0;
}
</style>