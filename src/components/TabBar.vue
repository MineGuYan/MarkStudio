<script setup lang="ts">
/**
 * TabBar 组件 - 标签页栏
 *
 * 功能：
 * - 横向展示所有打开的标签页，每个标签页显示文件名和关闭按钮
 * - 未保存的标签页（isDirty）在文件名后显示 `*` 号
 * - 激活的标签页高亮显示
 * - 点击标签页切换激活状态
 * - 点击关闭按钮关闭标签页（由父组件处理保存确认）
 * - "+" 按钮创建新的空白标签页
 * - 右键菜单提供"关闭所有标签页"和"关闭未修改标签页"选项
 * - 标签页溢出折叠：当标签页超出可用宽度时，折叠溢出部分并通过下拉箭头展示
 * - 使用 ResizeObserver 检测标签页溢出
 * - 点击下拉箭头切换溢出标签页下拉菜单
 * - 点击下拉菜单中的标签页可选中并自动关闭下拉菜单
 * - 点击下拉菜单外部区域关闭下拉菜单
 *
 * 位置：位于 Toolbar 下方、编辑器区域上方，高度为 36px。
 */

import { ref, computed, onMounted, onUnmounted, nextTick, watch } from "vue";

// ==================== 类型定义 ====================

/** 标签页数据结构 */
interface Tab {
  /** 标签页唯一标识 */
  id: number;
  /** 标签页显示标题 */
  title: string;
  /** 文件完整路径 */
  path: string;
  /** 是否有未保存的修改 */
  isDirty: boolean;
}

// ==================== Props 定义 ====================

const props = defineProps<{
  /** 所有标签页数组 */
  tabs: Tab[];
  /** 当前激活的标签页 ID */
  activeTabId: number;
}>();

// ==================== Emits 定义 ====================

const emit = defineEmits<{
  /** 选中标签页 */
  "select-tab": [tabId: number];
  /** 关闭标签页 */
  "close-tab": [tabId: number];
  /** 新建标签页 */
  "new-tab": [];
  /** 关闭所有标签页 */
  "close-all-tabs": [];
  /** 关闭所有未修改的标签页 */
  "close-unmodified-tabs": [];
}>();

// ==================== 工具函数 ====================

/**
 * 从完整文件路径中提取文件名
 * 支持 Windows（\）和 Unix（/）路径分隔符
 *
 * @param path - 完整文件路径
 * @returns 文件名，若路径为空则返回 "未命名"
 */
function getFileName(path: string): string {
  if (!path) return "未命名";
  const parts = path.replace(/\\/g, "/").split("/");
  return parts[parts.length - 1] || "未命名";
}

// ==================== 右键菜单状态 ====================

/** 右键菜单是否可见 */
const contextMenuVisible = ref(false);

/** 右键菜单的水平位置（px） */
const contextMenuX = ref(0);

/** 右键菜单的垂直位置（px） */
const contextMenuY = ref(0);

/** 当前右键菜单作用于哪个标签页（null 表示非标签页区域） */
const contextMenuTabId = ref<number | null>(null);

/**
 * 处理标签页上的右键点击事件
 * 显示上下文菜单，记录点击位置和关联的标签页
 *
 * @param event - 鼠标事件
 * @param tabId - 被右键点击的标签页 ID
 */
function handleTabContextMenu(event: MouseEvent, tabId: number): void {
  event.preventDefault();
  // 关闭可能已打开的下拉菜单
  overflowDropdownVisible.value = false;
  contextMenuTabId.value = tabId;
  contextMenuX.value = event.clientX;
  contextMenuY.value = event.clientY;
  contextMenuVisible.value = true;
}

/**
 * 处理标签栏空白区域的右键点击事件
 * 显示上下文菜单但不关联特定标签页
 *
 * @param event - 鼠标事件
 */
function handleBarContextMenu(event: MouseEvent): void {
  event.preventDefault();
  // 关闭可能已打开的下拉菜单
  overflowDropdownVisible.value = false;
  contextMenuTabId.value = null;
  contextMenuX.value = event.clientX;
  contextMenuY.value = event.clientY;
  contextMenuVisible.value = true;
}

/** 关闭右键菜单 */
function closeContextMenu(): void {
  contextMenuVisible.value = false;
  contextMenuTabId.value = null;
}

/**
 * 处理右键菜单项"关闭所有标签页"
 * 发出事件并关闭菜单
 */
function handleCloseAllTabs(): void {
  emit("close-all-tabs");
  closeContextMenu();
}

/**
 * 处理右键菜单项"关闭未修改标签页"
 * 发出事件并关闭菜单
 */
function handleCloseUnmodifiedTabs(): void {
  emit("close-unmodified-tabs");
  closeContextMenu();
}

// ==================== 溢出折叠状态 ====================

/** 标签栏容器元素的引用 */
const tabContainerRef = ref<HTMLElement | null>(null);

/** 标签列表容器元素的引用 */
const tabListRef = ref<HTMLElement | null>(null);

/** 溢出标签页的 ID 集合 */
const overflowedTabIds = ref<Set<number>>(new Set());

/** 溢出下拉菜单是否可见 */
const overflowDropdownVisible = ref(false);

/** ResizeObserver 实例引用 */
let resizeObserver: ResizeObserver | null = null;

/**
 * 检测标签页是否溢出容器宽度
 * 遍历所有标签页元素，累加宽度，超出可用宽度时标记为溢出
 * 当有溢出标签页时，需要预留溢出箭头按钮的宽度
 */
function checkOverflow(): void {
  if (!tabContainerRef.value || !tabListRef.value) return;

  const containerWidth = tabContainerRef.value.clientWidth;
  // "+" 按钮固定宽度
  const plusButtonWidth = 36;
  // 溢出箭头按钮宽度（仅在有溢出时显示）
  const overflowArrowWidth = 28;

  // 获取所有标签页元素
  const tabElements =
    tabListRef.value.querySelectorAll<HTMLElement>(".tab-item");

  let totalWidth = 0;
  const overflowed = new Set<number>();

  for (let i = 0; i < tabElements.length; i++) {
    const tab = props.tabs[i];
    if (!tab) continue;

    totalWidth += tabElements[i].offsetWidth;

    // 如果已有溢出标签页，需要预留箭头按钮宽度
    const threshold =
      overflowed.size > 0
        ? containerWidth - plusButtonWidth - overflowArrowWidth
        : containerWidth - plusButtonWidth;

    if (totalWidth > threshold) {
      overflowed.add(tab.id);
    }
  }

  overflowedTabIds.value = overflowed;
}

/**
 * 可见的标签页（未溢出部分）
 */
const visibleTabs = computed(() => {
  return props.tabs.filter((tab) => !overflowedTabIds.value.has(tab.id));
});

/**
 * 溢出的标签页（需要折叠到下拉菜单中）
 */
const overflowedTabs = computed(() => {
  return props.tabs.filter((tab) => overflowedTabIds.value.has(tab.id));
});

/**
 * 是否需要显示溢出箭头按钮
 * 仅当存在溢出标签页时才显示
 */
const showOverflowArrow = computed(() => overflowedTabs.value.length > 0);

// ==================== 溢出下拉菜单 ====================

/** 溢出下拉菜单容器元素的引用 */
const overflowDropdownRef = ref<HTMLElement | null>(null);

/**
 * 切换溢出下拉菜单的可见状态
 */
function toggleOverflowDropdown(): void {
  overflowDropdownVisible.value = !overflowDropdownVisible.value;
  // 打开下拉菜单时关闭右键菜单
  if (overflowDropdownVisible.value) {
    closeContextMenu();
  }
}

/**
 * 从溢出下拉菜单中选中标签页
 * 发出选中事件并自动关闭下拉菜单
 *
 * @param tabId - 选中的标签页 ID
 */
function selectOverflowedTab(tabId: number): void {
  emit("select-tab", tabId);
  overflowDropdownVisible.value = false;
}

// ==================== 点击外部关闭处理 ====================

/**
 * 全局点击事件处理
 * 当点击溢出下拉菜单或右键菜单外部区域时，关闭对应菜单
 *
 * @param event - 鼠标事件
 */
function handleGlobalClick(event: MouseEvent): void {
  // 关闭溢出下拉菜单
  if (overflowDropdownVisible.value && overflowDropdownRef.value) {
    if (!overflowDropdownRef.value.contains(event.target as Node)) {
      overflowDropdownVisible.value = false;
    }
  }

  // 关闭右键菜单
  if (contextMenuVisible.value) {
    const contextMenu = document.querySelector(".tab-context-menu");
    if (contextMenu && !contextMenu.contains(event.target as Node)) {
      closeContextMenu();
    }
  }
}

// ==================== 生命周期 ====================

onMounted(() => {
  // 设置 ResizeObserver 监听容器大小变化，重新检测溢出
  if (tabContainerRef.value) {
    resizeObserver = new ResizeObserver(() => {
      checkOverflow();
    });
    resizeObserver.observe(tabContainerRef.value);
  }

  // 初始检测溢出
  nextTick(() => {
    checkOverflow();
  });

  // 监听全局点击事件，用于关闭下拉菜单和右键菜单
  document.addEventListener("click", handleGlobalClick);
});

onUnmounted(() => {
  // 清理 ResizeObserver
  if (resizeObserver) {
    resizeObserver.disconnect();
    resizeObserver = null;
  }

  // 移除全局点击事件监听
  document.removeEventListener("click", handleGlobalClick);
});

// 监听标签页数量变化，重新检测溢出
watch(
  () => props.tabs.length,
  () => {
    nextTick(() => {
      checkOverflow();
    });
  }
);

// 监听标签页标题变化，重新检测溢出（如文件重命名）
watch(
  () => props.tabs.map((t) => t.title),
  () => {
    nextTick(() => {
      checkOverflow();
    });
  }
);
</script>

<template>
  <div
    ref="tabContainerRef"
    class="tab-bar"
    @contextmenu="handleBarContextMenu"
  >
    <!-- 标签页列表容器 -->
    <div ref="tabListRef" class="tab-list">
      <!-- 遍历所有标签页，仅渲染可见（未溢出）的标签页 -->
      <div
        v-for="tab in visibleTabs"
        :key="tab.id"
        class="tab-item"
        :class="{
          'tab-item--active': tab.id === activeTabId,
        }"
        :title="tab.path || tab.title"
        @click="emit('select-tab', tab.id)"
        @contextmenu="handleTabContextMenu($event, tab.id)"
      >
        <!-- 标签页标题：文件名 + 未保存标记 -->
        <span class="tab-title">
          {{ tab.title || getFileName(tab.path) }}
          <span v-if="tab.isDirty" class="tab-dirty-mark">*</span>
        </span>

        <!-- 关闭按钮 -->
        <button
          class="tab-close-btn"
          title="关闭标签页"
          @click.stop="emit('close-tab', tab.id)"
        >
          <!-- 关闭图标（X） -->
          <svg
            class="tab-close-icon"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
          >
            <line x1="18" y1="6" x2="6" y2="18" />
            <line x1="6" y1="6" x2="18" y2="18" />
          </svg>
        </button>
      </div>
    </div>

    <!-- 溢出箭头按钮：仅在有溢出标签页时显示 -->
    <button
      v-if="showOverflowArrow"
      class="overflow-arrow-btn"
      :class="{ 'overflow-arrow-btn--active': overflowDropdownVisible }"
      title="更多标签页"
      @click.stop="toggleOverflowDropdown"
    >
      <!-- 下拉箭头图标 -->
      <svg
        class="overflow-arrow-icon"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
      >
        <polyline points="6 9 12 15 18 9" />
      </svg>

      <!-- 溢出标签页下拉菜单 -->
      <div
        v-if="overflowDropdownVisible"
        ref="overflowDropdownRef"
        class="overflow-dropdown"
      >
        <div
          v-for="tab in overflowedTabs"
          :key="tab.id"
          class="overflow-dropdown-item"
          :class="{ 'overflow-dropdown-item--active': tab.id === activeTabId }"
          :title="tab.path || tab.title"
          @click.stop="selectOverflowedTab(tab.id)"
        >
          <span class="overflow-dropdown-title">
            {{ tab.title || getFileName(tab.path) }}
            <span v-if="tab.isDirty" class="tab-dirty-mark">*</span>
          </span>
        </div>
      </div>
    </button>

    <!-- 新建标签页按钮 -->
    <button
      class="new-tab-btn"
      title="新建标签页"
      @click="emit('new-tab')"
    >
      <!-- 加号图标 -->
      <svg
        class="new-tab-icon"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
      >
        <line x1="12" y1="5" x2="12" y2="19" />
        <line x1="5" y1="12" x2="19" y2="12" />
      </svg>
    </button>

    <!-- 右键上下文菜单（通过 Teleport 挂载到 body，避免受 overflow 影响） -->
    <Teleport to="body">
      <div
        v-if="contextMenuVisible"
        class="tab-context-menu"
        :style="{
          left: contextMenuX + 'px',
          top: contextMenuY + 'px',
        }"
      >
        <div
          class="context-menu-item"
          @click="handleCloseAllTabs"
        >
          关闭所有标签页
        </div>
        <div
          class="context-menu-item"
          @click="handleCloseUnmodifiedTabs"
        >
          关闭未修改标签页
        </div>
      </div>
    </Teleport>
  </div>
</template>

<style scoped>
/* ==================== 标签栏整体样式 ==================== */

.tab-bar {
  /* 横向布局 */
  display: flex;
  align-items: stretch;

  /* 尺寸 */
  height: 36px;
  flex-shrink: 0;

  /* 使用主题 CSS 变量 */
  background-color: var(--toolbar-bg-color);
  border-bottom: 1px solid var(--border-color);

  /* 过渡动画 */
  transition: background-color 0.3s ease, border-color 0.3s ease;

  /* 防止文本被选中 */
  user-select: none;

  /* 溢出隐藏（标签列表内部处理滚动） */
  overflow: hidden;
}

/* ==================== 标签列表容器 ==================== */

.tab-list {
  /* 横向布局 */
  display: flex;
  align-items: stretch;

  /* 撑满剩余空间，溢出隐藏 */
  flex: 1;
  overflow: hidden;
}

/* ==================== 单个标签页样式 ==================== */

.tab-item {
  /* 横向布局 */
  display: flex;
  align-items: center;
  gap: 4px;

  /* 尺寸 */
  height: 100%;
  padding: 0 10px;
  max-width: 200px;
  flex-shrink: 0;

  /* 边框 */
  border-right: 1px solid var(--border-color);

  /* 样式 */
  cursor: pointer;
  color: var(--text-color);

  /* 过渡动画 */
  transition: background-color 0.15s ease, color 0.15s ease;
}

.tab-item:hover {
  background-color: var(--button-hover-bg);
}

/* 激活状态的标签页 */
.tab-item--active {
  background-color: var(--editor-bg-color);
  color: var(--button-active-text);
  border-bottom: 2px solid var(--button-active-text);
}

.tab-item--active:hover {
  background-color: var(--editor-bg-color);
}

/* ==================== 标签页标题 ==================== */

.tab-title {
  /* 文字溢出省略 */
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;

  font-size: 12px;
  line-height: 1;
}

/* 未保存标记（*号） */
.tab-dirty-mark {
  color: var(--button-active-text);
  font-weight: bold;
  margin-left: 1px;
}

/* ==================== 关闭按钮 ==================== */

.tab-close-btn {
  /* 布局 */
  display: flex;
  align-items: center;
  justify-content: center;

  /* 尺寸 */
  width: 18px;
  height: 18px;
  flex-shrink: 0;

  /* 样式 */
  border: none;
  border-radius: 3px;
  background: transparent;
  color: var(--text-color);
  cursor: pointer;
  opacity: 0.6;

  /* 过渡动画 */
  transition: opacity 0.15s ease, background-color 0.15s ease;
}

.tab-close-btn:hover {
  opacity: 1;
  background-color: var(--button-hover-bg);
}

.tab-close-icon {
  width: 12px;
  height: 12px;
}

/* ==================== 溢出箭头按钮 ==================== */

.overflow-arrow-btn {
  /* 布局 */
  display: flex;
  align-items: center;
  justify-content: center;
  position: relative;

  /* 尺寸 */
  width: 28px;
  height: 100%;
  flex-shrink: 0;

  /* 样式 */
  border: none;
  border-left: 1px solid var(--border-color);
  background: transparent;
  color: var(--text-color);
  cursor: pointer;
  opacity: 0.7;

  /* 过渡动画 */
  transition: opacity 0.15s ease, background-color 0.15s ease;
}

.overflow-arrow-btn:hover {
  opacity: 1;
  background-color: var(--button-hover-bg);
}

.overflow-arrow-btn--active {
  opacity: 1;
  background-color: var(--button-hover-bg);
}

.overflow-arrow-icon {
  width: 14px;
  height: 14px;
  transition: transform 0.2s ease;
}

.overflow-arrow-btn--active .overflow-arrow-icon {
  transform: rotate(180deg);
}

/* ==================== 溢出下拉菜单 ==================== */

.overflow-dropdown {
  /* 绝对定位：在按钮下方展开 */
  position: absolute;
  top: 100%;
  right: 0;

  /* 尺寸 */
  min-width: 180px;
  max-width: 300px;
  max-height: 300px;
  overflow-y: auto;

  /* 样式 */
  background-color: var(--bg-color);
  border: 1px solid var(--border-color);
  border-radius: 6px;
  box-shadow: 0 4px 16px rgba(0, 0, 0, 0.15);

  /* 层级 */
  z-index: 200;

  /* 间距 */
  margin-top: 2px;
  padding: 4px 0;
}

/* 下拉菜单项 */
.overflow-dropdown-item {
  /* 布局 */
  display: flex;
  align-items: center;

  /* 尺寸 */
  padding: 6px 12px;

  /* 样式 */
  cursor: pointer;
  color: var(--text-color);
  font-size: 12px;

  /* 过渡动画 */
  transition: background-color 0.1s ease;
}

.overflow-dropdown-item:hover {
  background-color: var(--button-hover-bg);
}

/* 下拉菜单中激活的标签页 */
.overflow-dropdown-item--active {
  background-color: var(--button-hover-bg);
  color: var(--button-active-text);
}

/* 下拉菜单标题 */
.overflow-dropdown-title {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* ==================== 新建标签页按钮 ==================== */

.new-tab-btn {
  /* 布局 */
  display: flex;
  align-items: center;
  justify-content: center;

  /* 尺寸 */
  width: 36px;
  height: 100%;
  flex-shrink: 0;

  /* 样式 */
  border: none;
  border-left: 1px solid var(--border-color);
  background: transparent;
  color: var(--text-color);
  cursor: pointer;
  opacity: 0.7;

  /* 过渡动画 */
  transition: opacity 0.15s ease, background-color 0.15s ease;
}

.new-tab-btn:hover {
  opacity: 1;
  background-color: var(--button-hover-bg);
}

.new-tab-icon {
  width: 14px;
  height: 14px;
}

/* ==================== 右键上下文菜单 ==================== */

.tab-context-menu {
  /* 固定定位：跟随鼠标点击位置 */
  position: fixed;

  /* 尺寸 */
  min-width: 160px;

  /* 样式 */
  background-color: var(--bg-color);
  border: 1px solid var(--border-color);
  border-radius: 6px;
  box-shadow: 0 4px 16px rgba(0, 0, 0, 0.15);

  /* 层级：高于溢出下拉菜单 */
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
</style>