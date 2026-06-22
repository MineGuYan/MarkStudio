<script setup lang="ts">
/**
 * App.vue - 应用根组件
 *
 * 功能：
 * - 组合 Toolbar、TabBar、Editor、Preview、SplitPane、Outline 子组件，构建完整应用布局
 * - 支持多标签页管理：每个标签页拥有独立的文档内容、文件路径、脏状态、编辑模式、解析后的 HTML 和大纲
 * - 管理三种编辑模式：源代码（source）、预览（preview）、双屏（split）
 * - 管理主题（浅色 / 深色），通过 useTheme composable 实现
 * - 管理 Markdown 内容状态，并在预览/双屏模式下通过 Tauri IPC 解析为 HTML
 * - 调用 extract_outline IPC 获取大纲数据，显示在左侧大纲面板
 * - 通过 useShortcuts 注册快捷键：Ctrl+S 保存、Ctrl+F 查找
 * - 使用防抖优化 IPC 调用频率，避免频繁请求后端
 * - 集成协作编辑功能：管理协作面板、发送编辑操作和光标位置、轮询远程状态
 * - 标签页恢复：启动时根据设置项决定是否恢复上次打开的标签页
 * - 窗口关闭时保存所有打开的标签页信息
 */

import { ref, watch, computed, onMounted, onUnmounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import { getCurrentWindow } from "@tauri-apps/api/window";

// 导入子组件
import Toolbar from "./components/Toolbar.vue";
import TabBar from "./components/TabBar.vue";
import Editor from "./components/Editor.vue";
import Preview from "./components/Preview.vue";
import SplitPane from "./components/SplitPane.vue";
import SidebarTabs from "./components/SidebarTabs.vue";
import CollaborationPanel from "./components/CollaborationPanel.vue";
import SettingsPanel from "./components/SettingsPanel.vue";
import CloseConfirmDialog from "./components/CloseConfirmDialog.vue";
import TabSaveConfirmDialog from "./components/TabSaveConfirmDialog.vue";
import FavoriteSelectDialog from "./components/FavoriteSelectDialog.vue";

// 导入组合式函数
import { useTheme } from "./composables/useTheme";
import { useShortcuts } from "./composables/useShortcuts";

// 导入类型
import type { OutlineItem } from "./components/Outline.vue";
import type { Operation } from "./components/Editor.vue";
import type { PeerInfo } from "./components/CursorOverlay.vue";
import type { SharedFileInfo } from "./components/CollaborationPanel.vue";

// ==================== 类型定义 ====================

/**
 * 标签页信息接口
 * 每个标签页独立维护自己的文档内容、路径、脏状态、编辑模式和解析结果
 */
interface TabInfo {
  /** 标签页唯一标识 */
  id: number;
  /** 文件完整路径（新建文档为空字符串） */
  path: string;
  /** 标签页显示标题 */
  title: string;
  /** Markdown 源代码内容 */
  content: string;
  /** 最近一次保存时的文档内容，用于对比是否被修改 */
  lastSavedContent: string;
  /** 文档是否有未保存的修改 */
  isDirty: boolean;
  /** 当前编辑模式：source 为源代码编辑，preview 为预览，split 为双屏 */
  mode: "source" | "preview" | "split";
  /** Markdown 解析后的 HTML 字符串，用于预览渲染 */
  parsedHtml: string;
  /** 大纲条目列表 */
  outline: OutlineItem[];
}

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
 * 注册 Ctrl+S 快捷键：保存当前活跃标签页的文件
 * 如果已有文件路径则直接保存，否则弹出"另存为"对话框
 */
registerShortcut("s", () => {
  saveFile();
});

// ==================== 多标签页状态管理 ====================

/** 所有打开的标签页数组 */
const tabs = ref<TabInfo[]>([]);

/** 当前活跃标签页的 ID，0 表示没有打开的标签页 */
const activeTabId = ref<number>(0);

/** 自增 ID 计数器，用于生成唯一的标签页 ID */
let nextTabId = 1;

/**
 * 活跃标签页计算属性
 * 根据 activeTabId 从 tabs 数组中查找对应的标签页
 * 如果没有打开的标签页，返回 null
 */
const activeTab = computed<TabInfo | null>(
  () => tabs.value.find((t) => t.id === activeTabId.value) || null
);

/**
 * 活跃标签页的解析后 HTML（用于预览组件绑定）
 */
const activeParsedHtml = computed(() => activeTab.value?.parsedHtml ?? "");

/**
 * 活跃标签页的大纲数据（用于大纲面板绑定）
 */
const activeOutline = computed(() => activeTab.value?.outline ?? []);

/**
 * 活跃标签页的内容双向绑定计算属性
 * 用于 Editor 组件的 v-model 绑定
 */
const activeContent = computed({
  get: () => activeTab.value?.content ?? "",
  set: (val: string) => {
    if (activeTab.value) {
      activeTab.value.content = val;
    }
  },
});

/**
 * 活跃标签页的编辑模式（用于工具栏绑定）
 */
const activeMode = computed({
  get: () => activeTab.value?.mode ?? "source",
  set: (val: "source" | "preview" | "split") => {
    if (activeTab.value) {
      activeTab.value.mode = val;
    }
  },
});

// ==================== 工具函数 ====================

/**
 * 从完整文件路径中提取文件名
 * 支持 Windows（\）和 Unix（/）路径分隔符
 *
 * @param path - 完整文件路径
 * @returns 文件名
 */
function getFileName(path: string): string {
  if (!path) return "新建文档";
  const parts = path.replace(/\\/g, "/").split("/");
  return parts[parts.length - 1] || "新建文档";
}

// ==================== 窗口关闭相关 ====================

/** 窗口关闭事件监听器的取消函数 */
let unlistenCloseRequested: (() => void) | null = null;

/** 关闭确认对话框组件引用 */
const closeConfirmRef = ref<InstanceType<typeof CloseConfirmDialog> | null>(
  null
);

/** 标签页保存确认对话框组件引用 */
const tabSaveConfirmRef =
  ref<InstanceType<typeof TabSaveConfirmDialog> | null>(null);

/** 收藏选择弹窗组件引用 */
const favoriteSelectRef =
  ref<InstanceType<typeof FavoriteSelectDialog> | null>(null);

/** 侧边栏标签页组件引用，用于主动调用刷新方法 */
const sidebarTabsRef =
  ref<InstanceType<typeof SidebarTabs> | null>(null);

// ==================== 侧边栏状态 ====================

/** 大纲面板是否折叠 */
const outlineCollapsed = ref<boolean>(false);

/** 侧边栏当前激活的选项卡：outline | recent | favorites */
const sidebarActiveTab = ref<"outline" | "recent" | "favorites">("outline");

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

// ==================== 设置面板状态 ====================

/** 设置面板是否可见 */
const settingsPanelVisible = ref<boolean>(false);

/** 图片缓存目录路径（可在设置中更改） */
const imageCacheDir = ref<string>("");

/** 启动时是否恢复上次打开的标签页 */
const restoreTabsOnStartup = ref<boolean>(false);

/** 默认的图片缓存目录 */
const DEFAULT_IMAGE_CACHE_DIR = "data/image_cache/";

/**
 * 设置分类与设置项配置
 *
 * 要新增设置分类，只需在此数组中添加新的分类对象即可。
 * 每个分类包含 id、label 和一组 settings。
 * 每个 setting 包含 key、label、description、type、value 和 defaultValue。
 */
const settingsCategories = ref([
  {
    id: "general",
    label: "通用",
    settings: [
      {
        key: "image_cache_dir",
        label: "图片缓存目录",
        description:
          "粘贴图片时，图片文件将保存到此目录。默认为项目根目录下的 data/image_cache/。",
        type: "path" as const,
        value: imageCacheDir.value,
        defaultValue: DEFAULT_IMAGE_CACHE_DIR,
      },
      {
        key: "restore_tabs_on_startup",
        label: "启动时恢复标签页",
        description:
          "启动应用时自动恢复上次关闭前打开的所有标签页。",
        type: "toggle" as const,
        value: String(restoreTabsOnStartup.value),
        defaultValue: "false",
      },
    ],
  },
  {
    id: "editor",
    label: "编辑器",
    settings: [
      // 未来可在此添加编辑器相关设置
    ],
  },
]);

// ==================== 设置面板事件处理 ====================

/**
 * 处理设置面板"设置项变更"事件
 * 将设置值持久化到 SQLite 数据库，并同步更新本地状态
 *
 * @param key - 设置项标识键
 * @param value - 新的设置值
 */
async function onSettingUpdate(key: string, value: string): Promise<void> {
  try {
    // 持久化到 SQLite 数据库
    await invoke("save_setting", { key, value });

    // 同步更新本地状态
    if (key === "image_cache_dir") {
      imageCacheDir.value = value;
    }
    if (key === "restore_tabs_on_startup") {
      restoreTabsOnStartup.value = value === "true";
    }

    // 同步更新 settingsCategories 中对应设置项的 value
    for (const category of settingsCategories.value) {
      const setting = category.settings.find((s) => s.key === key);
      if (setting) {
        setting.value = value;
        break;
      }
    }
  } catch (e) {
    console.error("保存设置失败:", e);
  }
}

// ==================== 防抖 IPC 调用 ====================

/** Markdown 解析防抖定时器 ID */
let parseDebounceTimer: ReturnType<typeof setTimeout> | null = null;

/** 大纲提取防抖定时器 ID */
let outlineDebounceTimer: ReturnType<typeof setTimeout> | null = null;

/** 防抖延迟时间（毫秒） */
const DEBOUNCE_DELAY = 300;

/**
 * 带防抖的 Markdown 解析函数
 * 当活跃标签页内容变化时，延迟 DEBOUNCE_DELAY 毫秒后才调用 Tauri 后端解析
 * 通过捕获 tabId 防止切换标签页后的竞态条件
 *
 * @param markdown - 待解析的 Markdown 源代码
 * @param tabId - 触发解析的标签页 ID
 */
function debouncedParseMarkdown(markdown: string, tabId: number): void {
  if (parseDebounceTimer !== null) {
    clearTimeout(parseDebounceTimer);
  }

  parseDebounceTimer = setTimeout(async () => {
    if (activeTabId.value !== tabId) return;

    try {
      const html = await invoke<string>("parse_markdown", { markdown });
      const tab = tabs.value.find((t) => t.id === tabId);
      if (tab) {
        tab.parsedHtml = html;
      }
    } catch (error) {
      console.error("Markdown 解析失败:", error);
      const tab = tabs.value.find((t) => t.id === tabId);
      if (tab) {
        tab.parsedHtml = `<p style="color: red;">Markdown 解析失败: ${error}</p>`;
      }
    }
  }, DEBOUNCE_DELAY);
}

/**
 * 带防抖的大纲提取函数
 * 当活跃标签页内容变化时，延迟 DEBOUNCE_DELAY 毫秒后才调用 Tauri 后端提取大纲
 * 通过捕获 tabId 防止切换标签页后的竞态条件
 *
 * @param markdown - 待提取大纲的 Markdown 源代码
 * @param tabId - 触发提取的标签页 ID
 */
function debouncedExtractOutline(markdown: string, tabId: number): void {
  if (outlineDebounceTimer !== null) {
    clearTimeout(outlineDebounceTimer);
  }

  outlineDebounceTimer = setTimeout(async () => {
    if (activeTabId.value !== tabId) return;

    try {
      const items = await invoke<OutlineItem[]>("extract_outline", {
        markdown,
      });
      const tab = tabs.value.find((t) => t.id === tabId);
      if (tab) {
        tab.outline = items;
      }
    } catch (error) {
      console.error("大纲提取失败:", error);
      const tab = tabs.value.find((t) => t.id === tabId);
      if (tab) {
        tab.outline = [];
      }
    }
  }, DEBOUNCE_DELAY);
}

// ==================== 标签页管理 ====================

/**
 * 创建新的空白标签页
 * 标题为"新建文档"，内容为空，编辑模式为源代码
 */
function newTab(): void {
  const tab: TabInfo = {
    id: nextTabId++,
    path: "",
    title: "新建文档",
    content: "",
    lastSavedContent: "",
    isDirty: false,
    mode: "source",
    parsedHtml: "",
    outline: [],
  };
  tabs.value.push(tab);
  activeTabId.value = tab.id;
}

function selectTab(tabId: number): void {
  activeTabId.value = tabId;
}

function removeTab(tabId: number): void {
  const idx = tabs.value.findIndex((t) => t.id === tabId);
  if (idx === -1) return;

  tabs.value.splice(idx, 1);

  if (activeTabId.value === tabId) {
    if (tabs.value.length > 0) {
      const newIdx = Math.min(idx, tabs.value.length - 1);
      activeTabId.value = tabs.value[newIdx].id;
    } else {
      activeTabId.value = 0;
    }
  }
}

async function closeTab(tabId: number): Promise<void> {
  const tab = tabs.value.find((t) => t.id === tabId);
  if (!tab) return;

  if (tab.isDirty) {
    activeTabId.value = tabId;
    const choice = await tabSaveConfirmRef.value!.show(tab.title);

    if (choice === "save") {
      await saveFile();
      removeTab(tabId);
    } else if (choice === "discard") {
      removeTab(tabId);
    }
  } else {
    removeTab(tabId);
  }
}

async function closeAllTabs(): Promise<void> {
  const dirtyTabs = tabs.value.filter((t) => t.isDirty);

  for (const tab of dirtyTabs) {
    activeTabId.value = tab.id;
    const choice = await tabSaveConfirmRef.value!.show(tab.title);

    if (choice === "save") {
      await saveFile();
    } else if (choice === "cancel") {
      return;
    }
  }

  tabs.value = [];
  activeTabId.value = 0;
}

function closeUnmodifiedTabs(): void {
  const unmodifiedIds = tabs.value
    .filter((t) => !t.isDirty)
    .map((t) => t.id);

  for (const id of unmodifiedIds) {
    removeTab(id);
  }
}

// ==================== 文件操作 ====================

async function openFile(): Promise<void> {
  try {
    const selected = await open({
      filters: [{ name: "Markdown", extensions: ["md", "markdown", "txt"] }],
      multiple: false,
    });

    if (selected) {
      const path = selected as string;

      const existingTab = tabs.value.find((t) => t.path === path);
      if (existingTab) {
        activeTabId.value = existingTab.id;
        return;
      }

      const fileContent = await invoke<string>("read_file", { path });

      const tab: TabInfo = {
        id: nextTabId++,
        path,
        title: getFileName(path),
        content: fileContent,
        lastSavedContent: fileContent,
        isDirty: false,
        mode: "source",
        parsedHtml: "",
        outline: [],
      };
      tabs.value.push(tab);
      activeTabId.value = tab.id;

      try {
        await invoke("add_recent_file", { path });
        // 立即刷新侧边栏最近访问列表，使新打开的文件立即显示
        refreshSidebarLists();
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
 * 刷新侧边栏列表（最近访问 + 收藏夹）
 * 当打开新文件或收藏文件后调用，保证侧边栏数据与后端保持同步
 */
function refreshSidebarLists(): void {
  // 当前激活的是"最近访问"标签页时，立即重新加载最近文件
  if (sidebarActiveTab.value === "recent") {
    sidebarTabsRef.value?.refreshRecent();
  }
  // 当前激活的是"收藏夹"标签页时，立即重新加载收藏夹目录树
  if (sidebarActiveTab.value === "favorites") {
    sidebarTabsRef.value?.refreshFavorites();
  }
}

async function saveFile(): Promise<void> {
  if (!activeTab.value) return;

  try {
    if (activeTab.value.path) {
      await invoke("write_file", {
        path: activeTab.value.path,
        content: activeTab.value.content,
      });
      activeTab.value.lastSavedContent = activeTab.value.content;
      activeTab.value.isDirty = false;
    } else {
      await saveFileAs();
    }
  } catch (error) {
    console.error("保存文件失败:", error);
    alert(`保存文件失败: ${error}`);
  }
}

async function saveFileAs(): Promise<void> {
  if (!activeTab.value) return;

  try {
    const selected = await save({
      filters: [{ name: "Markdown", extensions: ["md"] }],
    });

    if (selected) {
      const path = selected as string;

      await invoke("write_file", {
        path,
        content: activeTab.value.content,
      });

      activeTab.value.path = path;
      activeTab.value.title = getFileName(path);
      activeTab.value.lastSavedContent = activeTab.value.content;
      activeTab.value.isDirty = false;

      try {
        await invoke("add_recent_file", { path });
        refreshSidebarLists();
      } catch (e) {
        console.error("记录最近文件失败:", e);
      }
    }
  } catch (error) {
    console.error("另存为文件失败:", error);
    alert(`另存为文件失败: ${error}`);
  }
}

// ==================== 协作事件处理 ====================

function onCollabConnectionChange(connected: boolean): void {
  collabConnected.value = connected;
  if (!connected) {
    collabPeers.value = [];
  }
}

function onCollabPeersUpdate(peers: PeerInfo[]): void {
  collabPeers.value = peers;
}

function onCollabLocalPeerId(peerId: string): void {
  localPeerId.value = peerId;
}

interface CollabDocumentUpdate {
  document: string;
  path: string | null;
}

function onCollabDocumentUpdate(update: CollabDocumentUpdate): void {
  const { document, path } = update;

  // 如果有路径信息，尝试找到对应路径的标签页并更新
  if (path) {
    const targetTab = tabs.value.find((t) => t.path === path);
    if (targetTab && targetTab.content !== document) {
      targetTab.content = document;
      return;
    }
  }

  // 如果没有找到对应路径的标签页，或没有路径信息，更新当前活动标签页
  if (!activeTab.value) return;
  if (document !== activeTab.value.content) {
    activeTab.value.content = document;
  }
}

async function onEditorOperation(op: Operation): Promise<void> {
  if (!collabConnected.value) return;
  try {
    const opJson = JSON.stringify(op);
    await invoke("send_collab_operation", { opJson });
  } catch (error) {
    console.error("发送协作操作失败:", error);
  }
}

async function onEditorCursor(position: number): Promise<void> {
  if (!collabConnected.value) return;
  try {
    await invoke("send_collab_cursor", { position });
  } catch (error) {
    console.error("发送光标位置失败:", error);
  }
}

// ==================== 监听器 ====================

watch(
  () => {
    const tab = activeTab.value;
    return {
      content: tab?.content ?? "",
      mode: tab?.mode ?? "source",
      tabId: tab?.id ?? 0,
    };
  },
  ({ content: newContent, mode: newMode, tabId }) => {
    if ((newMode === "preview" || newMode === "split") && tabId > 0) {
      debouncedParseMarkdown(newContent, tabId);
    }
  },
  { immediate: true }
);

watch(
  () => {
    const tab = activeTab.value;
    return { content: tab?.content ?? "", tabId: tab?.id ?? 0 };
  },
  ({ content: newContent, tabId }) => {
    if (tabId > 0) {
      debouncedExtractOutline(newContent, tabId);
    }
  },
  { immediate: true }
);

watch(
  () => {
    const tab = activeTab.value;
    return { content: tab?.content ?? "", tabId: tab?.id ?? 0 };
  },
  async ({ content: newContent, tabId }) => {
    if (tabId <= 0) return;
    const tab = tabs.value.find((t) => t.id === tabId);
    if (!tab) return;

    tab.isDirty = await invoke("check_dirty_cmd", {
      current: newContent,
      saved: tab.lastSavedContent,
    });
  }
);

function handleCtrlF(e: KeyboardEvent): void {
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "f") {
    const textarea =
      document.querySelector<HTMLTextAreaElement>(".editor-textarea");
    if (textarea) {
      textarea.focus();
    }
  }
}

async function restoreTabs(): Promise<void> {
  try {
    const tabsJson: string = await invoke("get_open_tabs");
    if (!tabsJson) return;

    const savedTabs: Array<{
      path: string;
      title: string;
      content: string;
      isDirty: boolean;
      mode: string;
      active: boolean;
    }> = JSON.parse(tabsJson);

    if (!savedTabs || savedTabs.length === 0) return;

    for (const savedTab of savedTabs) {
      const tab: TabInfo = {
        id: nextTabId++,
        path: savedTab.path,
        title: savedTab.title,
        content: savedTab.content,
        lastSavedContent: savedTab.isDirty ? "" : savedTab.content,
        isDirty: savedTab.isDirty,
        mode: savedTab.mode as "source" | "preview" | "split",
        parsedHtml: "",
        outline: [],
      };
      tabs.value.push(tab);

      // 恢复激活的标签页
      if (savedTab.active) {
        activeTabId.value = tab.id;
      }
    }

    // 如果没有标记为激活的标签页，默认激活第一个
    if (tabs.value.length > 0 && activeTabId.value === 0) {
      activeTabId.value = tabs.value[0].id;
    }
  } catch (e) {
    console.error("恢复标签页失败:", e);
  }
}

onMounted(async () => {
  document.addEventListener("keydown", handleCtrlF);

  try {
    const settingsJson = await invoke<string>("load_all_settings_cmd");
    const settings = JSON.parse(settingsJson);
    if (settings.theme) {
      theme.value = settings.theme as "light" | "dark";
    }
    if (settings.image_cache_dir) {
      imageCacheDir.value = settings.image_cache_dir;
    }
    if (settings.restore_tabs_on_startup) {
      restoreTabsOnStartup.value = settings.restore_tabs_on_startup === "true";
    }
    const generalCategory = settingsCategories.value.find(
      (c) => c.id === "general"
    );
    if (generalCategory) {
      for (const setting of generalCategory.settings) {
        if (setting.key === "image_cache_dir") {
          setting.value = imageCacheDir.value;
        } else if (setting.key === "restore_tabs_on_startup") {
          setting.value = String(restoreTabsOnStartup.value);
        }
      }
    }
  } catch (e) {
    console.error("加载设置失败:", e);
  }

  if (restoreTabsOnStartup.value) {
    await restoreTabs();
  }

  unlistenCloseRequested = await getCurrentWindow().onCloseRequested(
    async (event) => {
      const dirtyTabs = tabs.value.filter((t) => t.isDirty);

      if (dirtyTabs.length === 0) {
        await saveOpenTabsAndClose();
        return;
      }

      event.preventDefault();

      for (const tab of dirtyTabs) {
        activeTabId.value = tab.id;
        const choice = await tabSaveConfirmRef.value!.show(tab.title);

        if (choice === "save") {
          try {
            await saveFile();
          } catch (e) {
            console.error("关闭前自动保存失败:", e);
            return;
          }
        } else if (choice === "cancel") {
          return;
        }
      }

      await saveOpenTabsAndClose();
    }
  );
});

async function saveOpenTabsAndClose(): Promise<void> {
  try {
    // 将标签页数据序列化为 JSON，供后端保存到数据库
    const tabsData = tabs.value.map((t) => ({
      path: t.path,
      title: t.title,
      content: t.content,
      isDirty: t.isDirty,
      mode: t.mode,
    }));
    const activeIndex = tabs.value.findIndex((t) => t.id === activeTabId.value);
    await invoke("save_open_tabs", {
      tabsJson: JSON.stringify(tabsData),
      activeIndex: activeIndex >= 0 ? activeIndex : 0,
    });
  } catch (e) {
    console.error("保存打开标签页信息失败:", e);
  }

  if (unlistenCloseRequested !== null) {
    unlistenCloseRequested();
    unlistenCloseRequested = null;
  }

  await getCurrentWindow().destroy();
}

onUnmounted(() => {
  document.removeEventListener("keydown", handleCtrlF);
  if (unlistenCloseRequested !== null) {
    unlistenCloseRequested();
    unlistenCloseRequested = null;
  }
});

async function onOutlineNavigate(line: number): Promise<void> {
  if (!activeTab.value) return;

  const charIndex: number = await invoke("compute_line_position_cmd", {
    content: activeTab.value.content,
    lineNumber: line,
  });

  const textarea =
    document.querySelector<HTMLTextAreaElement>(".editor-textarea");
  if (!textarea) return;

  textarea.focus();
  textarea.setSelectionRange(charIndex, charIndex);

  const lineHeight = parseFloat(getComputedStyle(textarea).lineHeight) || 24;
  textarea.scrollTop = (line - 1) * lineHeight;
}

// ==================== 侧边栏事件处理 ====================

/**
 * 处理最近访问或收藏夹中点击打开文件
 * 如果文件已打开则切换到对应标签页，否则创建新标签页
 */
async function handleSidebarOpenFile(path: string): Promise<void> {
  // 检查文件是否已打开
  const existingTab = tabs.value.find((t) => t.path === path);
  if (existingTab) {
    activeTabId.value = existingTab.id;
    return;
  }

  // 检查文件是否存在
  const exists: boolean = await invoke("check_file_exists", { path });
  if (!exists) {
    const shouldRemove = confirm(
      `文件不存在：\n${path}\n\n是否从列表中移除该项记录？`
    );
    if (shouldRemove) {
      // 尝试从最近访问和收藏夹中移除（静默处理错误）
      try {
        await invoke("remove_recent_file", { path });
      } catch { /* 忽略 */ }
    }
    return;
  }

  // 读取文件内容并创建新标签页
  try {
    const content = await invoke<string>("read_file", { path });
    const newTab: TabInfo = {
      id: nextTabId++,
      path,
      title: getFileName(path),
      content,
      lastSavedContent: content,
      isDirty: false,
      mode: "source",
      parsedHtml: "",
      outline: [],
    };
    tabs.value.push(newTab);
    activeTabId.value = newTab.id;

    // 触发解析和大纲提取
    if (newTab.mode === "preview" || newTab.mode === "split") {
      debouncedParseMarkdown(newTab.content, newTab.id);
    }
    debouncedExtractOutline(newTab.content, newTab.id);

    // 记录到最近文件
    try {
      await invoke("add_recent_file", { path });
      // 立即刷新侧边栏最近访问列表
      refreshSidebarLists();
    } catch { /* 忽略 */ }
  } catch (error) {
    console.error("打开文件失败:", error);
    alert(`打开文件失败: ${error}`);
  }
}

/**
 * 处理协作面板中点击共享文件的操作
 * 与普通文件打开不同，协作共享文件使用已经通过协作同步过来的内容创建标签页，
 * 而不是尝试从本地文件系统读取（因为客户端本地不存在主机端的文件路径）。
 *
 * 关键差异：
 * - 主机端（`is_local = true`）：使用主机端实际路径创建标签页，保存时直接写入原文件
 * - 客户端（`is_local = false`）：使用空路径创建标签页，保存时自动触发"另存为"对话框
 *   （与新建文档的保存行为一致），避免覆盖主机端文件或写入无效路径
 *
 * @param file - 共享文件信息（包含路径、标题、已同步的内容和 is_local 标记）
 */
function handleCollabOpenFile(file: SharedFileInfo): void {
  const { path, title, content, is_local } = file;

  // 检查文件是否已在标签页中打开
  // 主机端通过路径匹配（路径唯一）
  // 客户端通过标题+空路径匹配（因为客户端没有真实路径）
  const existingTab = is_local
    ? tabs.value.find((t) => t.path === path)
    : tabs.value.find((t) => t.path === "" && t.title === title);

  if (existingTab) {
    activeTabId.value = existingTab.id;
    return;
  }

  // 创建新标签页
  // 主机端：使用真实路径，保存时直接覆盖原文件
  // 客户端：使用空路径，保存时由 App.vue 的 saveFile 逻辑自动走"另存为"流程
  const newTab: TabInfo = {
    id: nextTabId++,
    path: is_local ? path : "",
    title: title || getFileName(path),
    content,
    lastSavedContent: content,
    isDirty: false,
    mode: "source",
    parsedHtml: "",
    outline: [],
  };
  tabs.value.push(newTab);
  activeTabId.value = newTab.id;

  // 触发解析和大纲提取
  if (newTab.mode === "preview" || newTab.mode === "split") {
    debouncedParseMarkdown(newTab.content, newTab.id);
  }
  debouncedExtractOutline(newTab.content, newTab.id);
}

/**
 * 处理最近访问中点击"收藏"后的操作
 * 弹出收藏选择弹窗，用户选择目录后添加收藏
 */
async function handleAddFavorite(path: string): Promise<void> {
  if (!favoriteSelectRef.value) return;
  const dirId = await favoriteSelectRef.value.show();
  if (dirId !== null && dirId !== undefined) {
    try {
      await invoke("add_favorite_file", { path, dirId });
    } catch (error) {
      console.error("添加收藏失败:", error);
      alert(`添加收藏失败: ${error}`);
    }
  }
}

</script>

<template>
  <div class="app-container">
    <Toolbar
      :mode="activeMode"
      :theme="theme"
      :collab-connected="collabConnected"
      @update:mode="(val: 'source' | 'preview' | 'split') => activeMode = val"
      @update:theme="(val: 'light' | 'dark') => theme = val"
      @open-file="openFile"
      @save-file="saveFile"
      @toggle-collab="collabPanelVisible = !collabPanelVisible"
      @toggle-settings="settingsPanelVisible = !settingsPanelVisible"
    />

    <TabBar
      :tabs="tabs"
      :active-tab-id="activeTabId"
      @select-tab="selectTab"
      @close-tab="closeTab"
      @new-tab="newTab"
      @close-all-tabs="closeAllTabs"
      @close-unmodified-tabs="closeUnmodifiedTabs"
    />

    <div class="main-layout">
      <div
        class="outline-wrapper"
        :class="{ 'outline-wrapper--collapsed': outlineCollapsed }"
      >
        <SidebarTabs
          v-if="!outlineCollapsed"
          ref="sidebarTabsRef"
          :active-tab="sidebarActiveTab"
          :outline="activeOutline"
          @update:active-tab="(val) => sidebarActiveTab = val as 'outline' | 'recent' | 'favorites'"
          @navigate="onOutlineNavigate"
          @open-file="handleSidebarOpenFile"
          @add-favorite="handleAddFavorite"
        />
      </div>

      <div class="outline-toggle" @click="outlineCollapsed = !outlineCollapsed">
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

      <main class="main-content">
        <div v-if="!activeTab" class="empty-state">
          <div class="empty-state__icon">
            <svg
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="1.5"
              width="64"
              height="64"
            >
              <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
              <polyline points="14 2 14 8 20 8" />
              <line x1="16" y1="13" x2="8" y2="13" />
              <line x1="16" y1="17" x2="8" y2="17" />
              <polyline points="10 9 9 9 8 9" />
            </svg>
          </div>
          <h2 class="empty-state__title">欢迎使用 MarkStudio</h2>
          <p class="empty-state__desc">
            点击上方"新建"按钮创建新文档，或点击"打开"按钮打开已有的 Markdown 文件
          </p>
          <div class="empty-state__actions">
            <button class="empty-state__btn" @click="newTab">
              <svg
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                width="16"
                height="16"
              >
                <line x1="12" y1="5" x2="12" y2="19" />
                <line x1="5" y1="12" x2="19" y2="12" />
              </svg>
              新建文档
            </button>
            <button class="empty-state__btn" @click="openFile">
              <svg
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                width="16"
                height="16"
              >
                <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" />
              </svg>
              打开文件
            </button>
          </div>
        </div>

        <template v-if="activeTab">
          <Editor
            v-if="activeTab.mode === 'source'"
            ref="sourceEditorRef"
            v-model="activeContent"
            placeholder="请输入 Markdown 内容..."
            :collab-enabled="collabConnected"
            :collab-peers="collabPeers"
            :local-peer-id="localPeerId"
            :image-cache-dir="imageCacheDir"
            @collab-operation="onEditorOperation"
            @collab-cursor="onEditorCursor"
          />

          <Preview
            v-else-if="activeTab.mode === 'preview'"
            :html="activeParsedHtml"
          />

          <SplitPane v-else>
            <template #left>
              <Editor
                ref="splitEditorRef"
                v-model="activeContent"
                placeholder="请输入 Markdown 内容..."
                :collab-enabled="collabConnected"
                :collab-peers="collabPeers"
                :local-peer-id="localPeerId"
                :image-cache-dir="imageCacheDir"
                @collab-operation="onEditorOperation"
                @collab-cursor="onEditorCursor"
              />
            </template>
            <template #right>
              <Preview :html="activeParsedHtml" />
            </template>
          </SplitPane>
        </template>
      </main>

      <CollaborationPanel
        v-show="collabPanelVisible"
        :current-document="activeTab?.content ?? ''"
        :tabs="tabs"
        :active-tab-id="activeTabId"
        @connection-change="onCollabConnectionChange"
        @peers-update="onCollabPeersUpdate"
        @document-update="onCollabDocumentUpdate"
        @local-peer-id="onCollabLocalPeerId"
        @open-file="handleCollabOpenFile"
      />
    </div>

    <div v-if="settingsPanelVisible" class="settings-overlay">
      <div class="settings-overlay__header">
        <button
          class="settings-overlay__close"
          @click="settingsPanelVisible = false"
          title="关闭设置"
        >
          <svg
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            width="20"
            height="20"
          >
            <line x1="18" y1="6" x2="6" y2="18" />
            <line x1="6" y1="6" x2="18" y2="18" />
          </svg>
        </button>
      </div>
      <SettingsPanel
        :categories="settingsCategories"
        @update-setting="onSettingUpdate"
      />
    </div>

    <CloseConfirmDialog ref="closeConfirmRef" />
    <TabSaveConfirmDialog ref="tabSaveConfirmRef" />
    <FavoriteSelectDialog ref="favoriteSelectRef" />
  </div>
</template>

<style>
* {
  margin: 0;
  padding: 0;
  box-sizing: border-box;
}

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
.app-container {
  display: flex;
  flex-direction: column;
  height: 100vh;
  width: 100vw;
  overflow: hidden;
  background-color: var(--bg-color);
  transition: background-color 0.3s ease;
}

.main-layout {
  flex: 1;
  display: flex;
  flex-direction: row;
  overflow: hidden;
}

.outline-wrapper {
  width: 200px;
  min-width: 200px;
  height: 100%;
  overflow: hidden;
  transition: width 0.2s ease, min-width 0.2s ease;
}

.outline-wrapper--collapsed {
  width: 0;
  min-width: 0;
}

.outline-toggle {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 18px;
  height: 100%;
  cursor: pointer;
  background-color: var(--toolbar-bg-color);
  border-right: 1px solid var(--border-color);
  transition: background-color 0.2s ease;
  user-select: none;
  flex-shrink: 0;
}

.outline-toggle:hover {
  background-color: var(--button-hover-bg);
}

.outline-toggle__icon {
  width: 12px;
  height: 12px;
  color: var(--text-color);
  transition: transform 0.2s ease;
}

.outline-toggle__icon--collapsed {
  transform: rotate(180deg);
}

.main-content {
  flex: 1;
  display: flex;
  overflow: hidden;
}

.empty-state {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 16px;
  padding: 40px;
  color: var(--text-color);
  opacity: 0.7;
}

.empty-state__icon {
  color: var(--text-color);
  opacity: 0.4;
  margin-bottom: 8px;
}

.empty-state__title {
  font-size: 20px;
  font-weight: 600;
  color: var(--heading-color);
  opacity: 0.8;
}

.empty-state__desc {
  font-size: 14px;
  color: var(--text-color);
  opacity: 0.5;
  max-width: 360px;
  text-align: center;
  line-height: 1.6;
}

.empty-state__actions {
  display: flex;
  gap: 12px;
  margin-top: 8px;
}

.empty-state__btn {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 10px 20px;
  border: 1px solid var(--border-color);
  border-radius: 8px;
  background: var(--button-hover-bg);
  cursor: pointer;
  font-size: 14px;
  font-weight: 500;
  color: var(--text-color);
  transition: all 0.2s ease;
}

.empty-state__btn:hover {
  background: var(--button-active-bg);
  color: var(--button-active-text);
  border-color: var(--button-active-bg);
}

.settings-overlay {
  position: absolute;
  top: 84px;
  left: 0;
  right: 0;
  bottom: 0;
  z-index: 100;
  display: flex;
  flex-direction: column;
  background-color: var(--bg-color);
}

.settings-overlay__header {
  display: flex;
  justify-content: flex-end;
  padding: 8px 16px;
  background-color: var(--toolbar-bg-color);
  border-bottom: 1px solid var(--border-color);
}

.settings-overlay__close {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 32px;
  height: 32px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--text-color);
  cursor: pointer;
  transition: background-color 0.15s ease;
}

.settings-overlay__close:hover {
  background-color: var(--button-hover-bg);
}
</style>