<script setup lang="ts">
/**
 * App.vue - 应用根组件
 *
 * 功能：
 * - 组合 Toolbar、Editor、Preview、SplitPane、Outline 子组件，构建完整应用布局
 * - 管理三种编辑模式：源代码（source）、预览（preview）、双屏（split）
 * - 管理主题（浅色 / 深色），通过 useTheme composable 实现
 * - 管理 Markdown 内容状态，并在预览/双屏模式下通过 Tauri IPC 解析为 HTML
 * - 调用 extract_outline IPC 获取大纲数据，显示在左侧大纲面板
 * - 通过 useShortcuts 注册快捷键：Ctrl+S 保存、Ctrl+F 查找
 * - 使用防抖优化 IPC 调用频率，避免频繁请求后端
 * - 集成协作编辑功能：管理协作面板、发送编辑操作和光标位置、轮询远程状态
 */

import { ref, watch, onMounted, onUnmounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";

// 导入子组件
import Toolbar from "./components/Toolbar.vue";
import Editor from "./components/Editor.vue";
import Preview from "./components/Preview.vue";
import SplitPane from "./components/SplitPane.vue";
import Outline from "./components/Outline.vue";
import CollaborationPanel from "./components/CollaborationPanel.vue";

// 导入组合式函数
import { useTheme } from "./composables/useTheme";
import { useShortcuts } from "./composables/useShortcuts";

// 导入类型
import type { OutlineItem } from "./components/Outline.vue";
import type { Operation } from "./components/Editor.vue";
import type { PeerInfo } from "./components/CursorOverlay.vue";

// ==================== 主题管理 ====================

/** 使用 useTheme composable 管理当前主题状态 */
const { theme } = useTheme();

/**
 * 监听主题变化，自动将主题偏好持久化到 SQLite 数据库
 * 使用 watch 的 immediate: false 避免启动时重复保存
 */
watch(theme, async (newTheme) => {
  try {
    await invoke("save_setting", { key: "theme", value: newTheme });
  } catch (e) {
    console.error("保存主题设置失败:", e);
  }
});

// ==================== 快捷键管理 ====================

const { registerShortcut } = useShortcuts();

/**
 * 注册 Ctrl+S 快捷键：保存文件
 * 如果已有文件路径则直接保存，否则弹出"另存为"对话框
 */
registerShortcut("s", () => {
  saveFile();
});

// ==================== 编辑模式管理 ====================

/** 当前编辑模式：source 为源代码编辑，preview 为预览，split 为双屏模式 */
const mode = ref<"source" | "preview" | "split">("source");

// ==================== Markdown 内容管理 ====================

/** Markdown 源代码内容 */
const content = ref<string>("");

/** 当前打开的文件路径，为空表示尚未保存过 */
const currentFilePath = ref<string>("");

/** Markdown 解析后的 HTML 字符串，用于预览渲染 */
const parsedHtml = ref<string>("");

/** 大纲条目列表，由 extract_outline IPC 获取 */
const outline = ref<OutlineItem[]>([]);

// ==================== 大纲面板折叠状态 ====================

/** 大纲面板是否折叠 */
const outlineCollapsed = ref<boolean>(false);

// ==================== 协作状态 ====================

/** 协作面板是否可见 */
const collabPanelVisible = ref<boolean>(false);

/** 是否已连接到协作房间 */
const collabConnected = ref<boolean>(false);

/** 协作者列表（包含光标位置） */
const collabPeers = ref<PeerInfo[]>([]);

/** 本地对等方 ID，用于过滤掉自己的光标 */
const localPeerId = ref<string>("");

/** 编辑器组件引用（source 模式） */
const sourceEditorRef = ref<InstanceType<typeof Editor> | null>(null);

/** 编辑器组件引用（split 模式 - 左侧） */
const splitEditorRef = ref<InstanceType<typeof Editor> | null>(null);

/** 协作状态轮询定时器 ID */
let collabPollTimer: ReturnType<typeof setInterval> | null = null;

/** 上一次轮询获取的文档内容，用于对比变更 */
let lastPollDocument: string = "";

// ==================== 防抖 IPC 调用 ====================

/** Markdown 解析防抖定时器 ID */
let parseDebounceTimer: ReturnType<typeof setTimeout> | null = null;

/** 大纲提取防抖定时器 ID */
let outlineDebounceTimer: ReturnType<typeof setTimeout> | null = null;

/** 防抖延迟时间（毫秒） */
const DEBOUNCE_DELAY = 300;

/**
 * 带防抖的 Markdown 解析函数
 * 当内容变化时，延迟 DEBOUNCE_DELAY 毫秒后才调用 Tauri 后端解析
 * 避免用户快速输入时频繁发起 IPC 请求
 *
 * @param markdown - 待解析的 Markdown 源代码
 */
function debouncedParseMarkdown(markdown: string): void {
  // 清除之前的定时器，重置防抖计时
  if (parseDebounceTimer !== null) {
    clearTimeout(parseDebounceTimer);
  }

  // 设置新的定时器
  parseDebounceTimer = setTimeout(async () => {
    try {
      // 调用 Tauri 后端的 parse_markdown 命令，将 Markdown 解析为 HTML
      parsedHtml.value = await invoke<string>("parse_markdown", {
        markdown,
      });
    } catch (error) {
      console.error("Markdown 解析失败:", error);
      // 解析失败时显示错误提示
      parsedHtml.value = `<p style="color: red;">Markdown 解析失败: ${error}</p>`;
    }
  }, DEBOUNCE_DELAY);
}

/**
 * 带防抖的大纲提取函数
 * 当内容变化时，延迟 DEBOUNCE_DELAY 毫秒后才调用 Tauri 后端提取大纲
 * 避免用户快速输入时频繁发起 IPC 请求
 *
 * @param markdown - 待提取大纲的 Markdown 源代码
 */
function debouncedExtractOutline(markdown: string): void {
  // 清除之前的定时器，重置防抖计时
  if (outlineDebounceTimer !== null) {
    clearTimeout(outlineDebounceTimer);
  }

  // 设置新的定时器
  outlineDebounceTimer = setTimeout(async () => {
    try {
      // 调用 Tauri 后端的 extract_outline 命令，提取 Markdown 文档大纲
      outline.value = await invoke<OutlineItem[]>("extract_outline", {
        markdown,
      });
    } catch (error) {
      console.error("大纲提取失败:", error);
      // 提取失败时清空大纲列表
      outline.value = [];
    }
  }, DEBOUNCE_DELAY);
}

// ==================== 文件操作 ====================

/**
 * 打开 Markdown 文件
 *
 * 使用 Tauri 原生文件打开对话框，支持 .md、.markdown、.txt 格式。
 * 用户选择文件后，通过 IPC 调用后端 read_file 命令读取文件内容，
 * 并将内容设置到编辑器中，同时记录当前文件路径。
 */
async function openFile(): Promise<void> {
  try {
    // 打开原生文件选择对话框，筛选 Markdown 相关文件
    const selected = await open({
      filters: [{ name: "Markdown", extensions: ["md", "markdown", "txt"] }],
      multiple: false,
    });

    if (selected) {
      // Tauri v2 的 open 对话框返回文件路径字符串
      const path = selected;
      currentFilePath.value = path;

      // 调用后端 read_file 命令读取文件内容
      content.value = await invoke<string>("read_file", { path });

      // 将文件路径记录到最近文件列表（SQLite 持久化）
      try {
        await invoke("add_recent_file", { path });
      } catch (e) {
        console.error("记录最近文件失败:", e);
      }
    }
  } catch (error) {
    console.error("打开文件失败:", error);
    alert(`打开文件失败: ${error}`);
  }
}

/**
 * 保存文件
 *
 * 如果当前已有文件路径（即之前打开过或保存过），则直接写入该文件。
 * 否则调用 saveFileAs() 弹出"另存为"对话框。
 */
async function saveFile(): Promise<void> {
  try {
    if (currentFilePath.value) {
      // 已有路径，直接保存
      await invoke("write_file", {
        path: currentFilePath.value,
        content: content.value,
      });
    } else {
      // 尚无路径，弹出"另存为"对话框
      await saveFileAs();
    }
  } catch (error) {
    console.error("保存文件失败:", error);
    alert(`保存文件失败: ${error}`);
  }
}

/**
 * 文件另存为
 *
 * 使用 Tauri 原生文件保存对话框，让用户选择保存位置和文件名。
 * 保存成功后更新当前文件路径。
 */
async function saveFileAs(): Promise<void> {
  try {
    // 打开原生文件保存对话框，默认扩展名为 .md
    const selected = await save({
      filters: [{ name: "Markdown", extensions: ["md"] }],
    });

    if (selected) {
      currentFilePath.value = selected;

      // 调用后端 write_file 命令写入文件内容
      await invoke("write_file", {
        path: selected,
        content: content.value,
      });
    }
  } catch (error) {
    console.error("另存为文件失败:", error);
    alert(`另存为文件失败: ${error}`);
  }
}

// ==================== 协作事件处理 ====================

/**
 * 处理协作面板"连接状态变更"事件
 *
 * @param connected - 是否已连接
 */
function onCollabConnectionChange(connected: boolean): void {
  collabConnected.value = connected;

  if (connected) {
    // 连接成功后开始轮询协作状态
    startCollabPolling();
  } else {
    // 断开后停止轮询
    stopCollabPolling();
    collabPeers.value = [];
  }
}

/**
 * 处理协作面板"协作者列表更新"事件
 *
 * @param peers - 协作者列表
 */
function onCollabPeersUpdate(peers: PeerInfo[]): void {
  collabPeers.value = peers;
}

/**
 * 处理协作面板"文档更新"事件
 * 当远程文档内容变更时，更新本地编辑器内容
 *
 * @param document - 远程文档内容
 */
function onCollabDocumentUpdate(document: string): void {
  // 仅在文档内容确实变化时更新
  if (document !== content.value) {
    content.value = document;
  }
}

/**
 * 处理编辑器发出的本地编辑操作
 * 将 OT 操作序列化为 JSON 并通过 IPC 发送给后端
 *
 * @param op - OT 操作对象
 */
async function onEditorOperation(op: Operation): Promise<void> {
  if (!collabConnected.value) return;

  try {
    const opJson = JSON.stringify(op);
    await invoke("send_collab_operation", { opJson });
  } catch (error) {
    console.error("发送协作操作失败:", error);
  }
}

/**
 * 处理编辑器发出的光标位置变化
 * 将光标位置通过 IPC 发送给后端
 *
 * @param position - 光标字符偏移量
 */
async function onEditorCursor(position: number): Promise<void> {
  if (!collabConnected.value) return;

  try {
    await invoke("send_collab_cursor", { position });
  } catch (error) {
    console.error("发送光标位置失败:", error);
  }
}

// ==================== 协作状态轮询 ====================

/**
 * 开始定期轮询协作状态
 * 每 500ms 调用一次 get_collab_status IPC 获取最新状态
 * 包括远程文档变更和协作者状态
 */
function startCollabPolling(): void {
  stopCollabPolling();

  collabPollTimer = setInterval(async () => {
    try {
      const statusJson = await invoke<string>("get_collab_status");
      const status = JSON.parse(statusJson);

      // 检测连接是否已断开（如主机关闭了房间）
      if (!status.connected && collabConnected.value) {
        console.warn("[协作] 检测到房间已断开，自动退出");
        collabConnected.value = false;
        stopCollabPolling();
        collabPeers.value = [];
        return;
      }

      // 更新协作者列表
      if (status.peers) {
        collabPeers.value = status.peers;
      }

      // 更新本地对等方 ID
      if (status.local_peer_id) {
        localPeerId.value = status.local_peer_id;
      }

      // 检查远程文档是否有变更
      if (
        status.document !== undefined &&
        status.document !== lastPollDocument
      ) {
        lastPollDocument = status.document;
        // 如果远程文档内容与本地不同，更新本地内容
        if (status.document !== content.value) {
          content.value = status.document;
        }
      }
    } catch (error) {
      console.error("协作状态轮询失败:", error);
    }
  }, 500);
}

/**
 * 停止协作状态轮询
 */
function stopCollabPolling(): void {
  if (collabPollTimer !== null) {
    clearInterval(collabPollTimer);
    collabPollTimer = null;
  }
  lastPollDocument = "";
  localPeerId.value = "";
}

// ==================== 监听器 ====================

/**
 * 监听 Markdown 内容变化和模式切换
 * 当处于预览模式或双屏模式时，触发防抖 Markdown 解析
 */
watch(
  () => ({ content: content.value, mode: mode.value }),
  ({ content: newContent, mode: newMode }) => {
    // 预览模式和双屏模式都需要解析 Markdown 为 HTML
    if (newMode === "preview" || newMode === "split") {
      debouncedParseMarkdown(newContent);
    }
  },
  { immediate: true }
);

/**
 * 监听 Markdown 内容变化，自动提取大纲
 */
watch(
  () => content.value,
  (newContent) => {
    debouncedExtractOutline(newContent);
  },
  { immediate: true }
);

// ==================== Ctrl+F 查找快捷键 ====================

/**
 * Ctrl+F 查找快捷键处理函数
 * 聚焦编辑器文本域，让浏览器原生查找功能在编辑器内进行
 * 不阻止默认行为，确保浏览器原生 Ctrl+F 查找对话框能正常弹出
 *
 * @param e - 键盘事件对象
 */
function handleCtrlF(e: KeyboardEvent): void {
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "f") {
    // 聚焦编辑器文本域，让浏览器查找在编辑器内进行
    const textarea = document.querySelector<HTMLTextAreaElement>(".editor-textarea");
    if (textarea) {
      textarea.focus();
    }
    // 不阻止默认行为，让浏览器原生 Ctrl+F 查找对话框弹出
  }
}

// 在组件挂载时绑定 Ctrl+F 键盘事件，并加载持久化的主题偏好
onMounted(async () => {
  document.addEventListener("keydown", handleCtrlF);

  // 从 SQLite 数据库加载持久化的主题偏好
  try {
    const savedTheme = await invoke<string | null>("get_setting", {
      key: "theme",
    });
    if (savedTheme && (savedTheme === "light" || savedTheme === "dark")) {
      theme.value = savedTheme;
    }
  } catch (e) {
    console.error("加载主题设置失败:", e);
  }
});

// 在组件卸载时移除 Ctrl+F 键盘事件，防止内存泄漏
onUnmounted(() => {
  document.removeEventListener("keydown", handleCtrlF);
  // 清理协作轮询定时器
  stopCollabPolling();
});

// ==================== 大纲导航处理 ====================

/**
 * 处理大纲面板的导航事件
 * 当用户点击大纲条目时，将编辑器光标跳转到对应行
 *
 * @param line - 目标行号（从 1 开始）
 */
function onOutlineNavigate(line: number): void {
  // 查找编辑器文本域
  const textarea = document.querySelector<HTMLTextAreaElement>(".editor-textarea");
  if (!textarea) return;

  // 计算目标行在文本中的字符起始位置
  const lines = content.value.split("\n");
  let charIndex = 0;
  for (let i = 0; i < Math.min(line - 1, lines.length); i++) {
    charIndex += lines[i].length + 1; // +1 为换行符
  }

  // 聚焦编辑器并将光标移动到目标行
  textarea.focus();
  textarea.setSelectionRange(charIndex, charIndex);

  // 滚动到目标行位置
  const lineHeight = parseFloat(getComputedStyle(textarea).lineHeight) || 24;
  textarea.scrollTop = (line - 1) * lineHeight;
}
</script>

<template>
  <!-- 应用根容器，使用主题 CSS 变量控制背景色 -->
  <div class="app-container">
    <!-- 顶部工具栏：模式切换 & 主题切换 & 文件操作 & 协作 -->
    <Toolbar
      :mode="mode"
      :theme="theme"
      :collab-connected="collabConnected"
      @update:mode="(val: 'source' | 'preview' | 'split') => mode = val"
      @update:theme="(val: 'light' | 'dark') => theme = val"
      @open-file="openFile"
      @save-file="saveFile"
      @toggle-collab="collabPanelVisible = !collabPanelVisible"
    />

    <!-- 下方主体区域：横向 flex 布局 -->
    <div class="main-layout">
      <!-- 左侧：大纲面板（可折叠） -->
      <div
        class="outline-wrapper"
        :class="{ 'outline-wrapper--collapsed': outlineCollapsed }"
      >
        <Outline
          v-if="!outlineCollapsed"
          :outline="outline"
          @navigate="onOutlineNavigate"
        />
      </div>

      <!-- 大纲面板折叠/展开切换按钮 -->
      <div class="outline-toggle" @click="outlineCollapsed = !outlineCollapsed">
        <!-- 折叠/展开箭头图标 -->
        <svg
          class="outline-toggle__icon"
          :class="{ 'outline-toggle__icon--collapsed': outlineCollapsed }"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
        >
          <polyline points="15 18 9 12 15 6" />
        </svg>
      </div>

      <!-- 右侧：主内容区域 -->
      <main class="main-content">
        <!-- 源代码编辑模式：仅显示 Markdown 编辑器 -->
        <Editor
          v-if="mode === 'source'"
          ref="sourceEditorRef"
          v-model="content"
          placeholder="请输入 Markdown 内容..."
          :collab-enabled="collabConnected"
          :collab-peers="collabPeers"
          :local-peer-id="localPeerId"
          @collab-operation="onEditorOperation"
          @collab-cursor="onEditorCursor"
        />

        <!-- 预览模式：仅显示 Markdown 解析后的 HTML -->
        <Preview
          v-else-if="mode === 'preview'"
          :html="parsedHtml"
        />

        <!-- 双屏模式：左侧编辑器 + 右侧预览 -->
        <SplitPane v-else>
          <template #left>
            <Editor
              ref="splitEditorRef"
              v-model="content"
              placeholder="请输入 Markdown 内容..."
              :collab-enabled="collabConnected"
              :collab-peers="collabPeers"
              :local-peer-id="localPeerId"
              @collab-operation="onEditorOperation"
              @collab-cursor="onEditorCursor"
            />
          </template>
          <template #right>
            <Preview :html="parsedHtml" />
          </template>
        </SplitPane>
      </main>

      <!-- 右侧：协作面板（侧边栏形式） -->
      <CollaborationPanel
        v-show="collabPanelVisible"
        @connection-change="onCollabConnectionChange"
        @peers-update="onCollabPeersUpdate"
        @document-update="onCollabDocumentUpdate"
      />
    </div>
  </div>
</template>

<style>
/* ==================== 全局重置样式 ==================== */

/* 重置默认边距，确保应用填满整个窗口 */
* {
  margin: 0;
  padding: 0;
  box-sizing: border-box;
}

/* 页面根元素样式 */
html,
body,
#app {
  height: 100%;
  width: 100%;
  overflow: hidden;
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", "PingFang SC",
    "Hiragino Sans GB", "Microsoft YaHei", "Helvetica Neue", Helvetica, Arial,
    sans-serif;
  font-size: 14px;
  line-height: 1.6;
  color: var(--text-color);
  background-color: var(--bg-color);
  transition: background-color 0.3s ease, color 0.3s ease;
}
</style>

<style scoped>
/* ==================== 应用布局样式 ==================== */

/* 应用根容器：纵向 flex 布局，顶部工具栏 + 下方主内容区 */
.app-container {
  display: flex;
  flex-direction: column;
  height: 100vh;
  width: 100vw;
  overflow: hidden;
  background-color: var(--bg-color);
  transition: background-color 0.3s ease;
}

/* ==================== 主体布局 ==================== */

/* 主体区域：横向 flex 布局，左侧大纲面板 + 右侧主内容 */
.main-layout {
  flex: 1;
  display: flex;
  flex-direction: row;
  overflow: hidden;
}

/* ==================== 大纲面板容器 ==================== */

/* 大纲面板容器，支持折叠动画 */
.outline-wrapper {
  width: 200px;
  min-width: 200px;
  height: 100%;
  overflow: hidden;
  transition: width 0.2s ease, min-width 0.2s ease;
}

/* 大纲面板折叠状态：宽度为 0 */
.outline-wrapper--collapsed {
  width: 0;
  min-width: 0;
}

/* ==================== 大纲折叠按钮 ==================== */

/* 大纲面板折叠/展开切换按钮 */
.outline-toggle {
  /* 布局 */
  display: flex;
  align-items: center;
  justify-content: center;

  /* 尺寸 */
  width: 18px;
  height: 100%;

  /* 样式 */
  cursor: pointer;
  background-color: var(--toolbar-bg-color);
  border-right: 1px solid var(--border-color);

  /* 过渡动画 */
  transition: background-color 0.2s ease;

  /* 防止文本被选中 */
  user-select: none;

  /* 防止被 flex 压缩 */
  flex-shrink: 0;
}

.outline-toggle:hover {
  background-color: var(--button-hover-bg);
}

/* 折叠/展开箭头图标 */
.outline-toggle__icon {
  width: 12px;
  height: 12px;
  color: var(--text-color);
  transition: transform 0.2s ease;
}

/* 折叠状态下的箭头方向翻转 */
.outline-toggle__icon--collapsed {
  transform: rotate(180deg);
}

/* ==================== 主内容区域 ==================== */

/* 主内容区域：撑满大纲面板右侧剩余空间 */
.main-content {
  flex: 1;
  display: flex;
  overflow: hidden;
}
</style>