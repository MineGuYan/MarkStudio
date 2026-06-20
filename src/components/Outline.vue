<script setup lang="ts">
/**
 * Outline 组件 - 大纲面板
 *
 * 功能：
 * - 接收 outline prop（数组，每项包含 { level: number, text: string, line: number }）
 * - 按层级缩进显示标题（level 1 不缩进，level 2 缩进 1em，level 3 缩进 2em...）
 * - 支持按层级折叠/展开：点击标题前的箭头可收起或展开其下的子标题
 * - 点击标题文本时 emit navigate 事件，传递行号
 * - 当前激活的标题高亮显示
 * - 面板标题显示"大纲"
 * - 添加滚动条（overflow-y: auto），避免大纲过长时撑破面板
 */

import { ref, computed } from "vue";

// ==================== 类型定义 ====================

/** 大纲条目接口 */
export interface OutlineItem {
  /** 标题级别（1-6），对应 h1 ~ h6 */
  level: number;
  /** 标题文本内容 */
  text: string;
  /** 标题所在行号 */
  line: number;
}

// ==================== Props 定义 ====================

/** 接收大纲数据数组 */
const props = defineProps<{
  /** 大纲条目列表 */
  outline: OutlineItem[];
}>();

// ==================== Emits 定义 ====================

/** 发出导航事件 */
const emit = defineEmits<{
  /** 点击大纲条目时，携带行号供父组件跳转 */
  navigate: [line: number];
}>();

// ==================== 状态定义 ====================

/** 当前激活的标题行号，用于高亮显示 */
const activeLine = ref<number | null>(null);

/** 已折叠的标题行号集合，使用 Set 管理折叠状态 */
const collapsedSet = ref<Set<number>>(new Set());

// ==================== 计算属性 ====================

/**
 * 判断指定索引处的大纲条目是否有子标题
 * 判断逻辑：从该条目之后查找，若存在 level 大于该条目的子标题，
 * 且中间没有 level 小于等于该条目的截断标题，则说明有子标题
 *
 * @param index - 当前条目在大纲数组中的索引
 * @returns 是否有子标题
 */
function hasChildren(index: number): boolean {
  const currentLevel = props.outline[index].level;
  // 从下一个条目开始遍历
  for (let i = index + 1; i < props.outline.length; i++) {
    const nextLevel = props.outline[i].level;
    // 如果遇到 level 更大的，说明有子标题
    if (nextLevel > currentLevel) {
      return true;
    }
    // 如果遇到 level 小于等于当前的，说明已脱离当前层级，没有子标题
    if (nextLevel <= currentLevel) {
      return false;
    }
  }
  return false;
}

/**
 * 计算可见的大纲条目列表
 * 根据 collapsedSet 中的折叠状态，过滤掉被折叠的父标题下的所有子标题
 */
const visibleOutline = computed<OutlineItem[]>(() => {
  const result: OutlineItem[] = [];
  // 记录当前遍历路径上所有折叠的父级 level，用于判断子标题是否需要隐藏
  const collapsedLevels: { level: number; line: number }[] = [];

  for (let i = 0; i < props.outline.length; i++) {
    const item = props.outline[i];

    // 清理已失效的折叠记录：移除所有 level >= 当前 item.level 的记录
    // 因为当前 item 已脱离那些折叠的父级范围
    while (
      collapsedLevels.length > 0 &&
      collapsedLevels[collapsedLevels.length - 1].level >= item.level
    ) {
      collapsedLevels.pop();
    }

    // 检查当前 item 是否在某个折叠的父级之下
    if (collapsedLevels.length > 0) {
      // 当前 item 的 level 大于栈顶折叠父级的 level，说明它是子标题，需要隐藏
      const topCollapsed = collapsedLevels[collapsedLevels.length - 1];
      if (item.level > topCollapsed.level) {
        // 如果当前 item 本身也被折叠了，需要入栈
        if (collapsedSet.value.has(item.line)) {
          collapsedLevels.push({ level: item.level, line: item.line });
        }
        continue; // 跳过被折叠的条目
      }
    }

    // 当前 item 可见，加入结果列表
    result.push(item);

    // 如果当前 item 处于折叠状态，将其入栈以隐藏后续子标题
    if (collapsedSet.value.has(item.line) && hasChildren(i)) {
      collapsedLevels.push({ level: item.level, line: item.line });
    }
  }

  return result;
});

// ==================== 方法定义 ====================

/**
 * 切换指定标题的折叠/展开状态
 * 只有拥有子标题的条目才可切换折叠状态
 *
 * @param item - 待切换折叠状态的大纲条目
 */
function toggleCollapse(item: OutlineItem): void {
  const newSet = new Set(collapsedSet.value);
  if (newSet.has(item.line)) {
    newSet.delete(item.line);
  } else {
    newSet.add(item.line);
  }
  collapsedSet.value = newSet;
}

/**
 * 处理大纲条目点击事件
 * 设置当前激活行号，并发出 navigate 事件
 *
 * @param item - 被点击的大纲条目
 */
function onItemClick(item: OutlineItem): void {
  activeLine.value = item.line;
  emit("navigate", item.line);
}
</script>

<template>
  <div class="outline-panel">
    <!-- 面板标题 -->
    <div class="outline-panel__header">大纲</div>

    <!-- 大纲条目列表 -->
    <div class="outline-panel__list">
      <!-- 无大纲内容时的占位提示 -->
      <div v-if="outline.length === 0" class="outline-panel__empty">
        暂无大纲
      </div>

      <!-- 逐条渲染可见的大纲条目 -->
      <div
        v-for="item in visibleOutline"
        :key="item.line"
        class="outline-panel__item"
        :class="{
          'outline-panel__item--active': activeLine === item.line,
        }"
        :style="{
          /** 根据标题级别计算缩进距离：level 1 缩进 0，level 2 缩进 1em，以此类推 */
          paddingLeft: (item.level - 1) * 1 + 'em',
        }"
        :title="item.text"
      >
        <!-- 折叠/展开箭头图标：有子标题时显示，点击可折叠或展开 -->
        <span
          v-if="hasChildren(outline.indexOf(item))"
          class="outline-panel__toggle"
          :class="{
            'outline-panel__toggle--collapsed': collapsedSet.has(item.line),
          }"
          @click.stop="toggleCollapse(item)"
        >
          <!-- 右箭头 SVG 图标 -->
          <svg
            class="outline-panel__toggle-icon"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
          >
            <polyline points="9 18 15 12 9 6" />
          </svg>
        </span>

        <!-- 无子标题时显示占位图标，保持对齐 -->
        <span v-else class="outline-panel__toggle outline-panel__toggle--placeholder" />

        <!-- 标题文本，点击跳转 -->
        <span
          class="outline-panel__item-text"
          @click="onItemClick(item)"
        >
          {{ item.text }}
        </span>
      </div>
    </div>
  </div>
</template>

<style scoped>
/* ==================== 面板整体样式 ==================== */

.outline-panel {
  /* 纵向 flex 布局 */
  display: flex;
  flex-direction: column;

  /* 撑满父容器高度 */
  height: 100%;

  /* 使用主题 CSS 变量 */
  background-color: var(--toolbar-bg-color);
  border-right: 1px solid var(--border-color);

  /* 防止被选中 */
  user-select: none;
}

/* ==================== 面板标题样式 ==================== */

.outline-panel__header {
  /* 标题样式 */
  padding: 12px 16px;
  font-size: 13px;
  font-weight: 600;
  color: var(--heading-color);
  border-bottom: 1px solid var(--border-color);

  /* 防止标题被压缩 */
  flex-shrink: 0;

  /* 过渡动画 */
  transition: color 0.3s ease;
}

/* ==================== 条目列表样式 ==================== */

.outline-panel__list {
  /* 撑满剩余空间 */
  flex: 1;

  /* 支持内容溢出时滚动 */
  overflow-y: auto;

  /* 上下内边距 */
  padding: 4px 0;
}

/* ==================== 滚动条样式 ==================== */

.outline-panel__list::-webkit-scrollbar {
  width: 6px;
}

.outline-panel__list::-webkit-scrollbar-track {
  background: transparent;
}

.outline-panel__list::-webkit-scrollbar-thumb {
  background-color: var(--border-color);
  border-radius: 3px;
}

/* ==================== 空状态提示样式 ==================== */

.outline-panel__empty {
  padding: 16px;
  text-align: center;
  font-size: 13px;
  color: var(--editor-placeholder-color);
}

/* ==================== 条目样式 ==================== */

.outline-panel__item {
  /* 条目布局 */
  display: flex;
  align-items: center;
  padding: 4px 16px;
  cursor: pointer;

  /* 文字样式 */
  font-size: 13px;
  color: var(--text-color);
  line-height: 1.6;

  /* 过渡动画 */
  transition: background-color 0.15s ease, color 0.15s ease;
}

/* 条目 hover 状态 */
.outline-panel__item:hover {
  background-color: var(--button-hover-bg);
}

/* 条目激活状态 */
.outline-panel__item--active {
  background-color: var(--button-active-bg);
  color: var(--button-active-text);
  font-weight: 500;
}

/* ==================== 折叠/展开箭头样式 ==================== */

.outline-panel__toggle {
  /* 箭头图标容器 */
  display: flex;
  align-items: center;
  justify-content: center;

  /* 固定尺寸，保证对齐 */
  width: 16px;
  height: 16px;

  /* 与文本之间的间距 */
  margin-right: 4px;

  /* 防止被 flex 压缩 */
  flex-shrink: 0;

  /* 颜色 */
  color: var(--text-color);
  opacity: 0.5;

  /* 过渡动画 */
  transition: transform 0.15s ease, opacity 0.15s ease;
}

/* 箭头图标 hover 时更明显 */
.outline-panel__toggle:not(.outline-panel__toggle--placeholder):hover {
  opacity: 0.8;
}

/* 折叠状态下的箭头：旋转 90 度指向右侧 */
.outline-panel__toggle--collapsed {
  transform: rotate(0deg);
}

/* 展开状态下的箭头：旋转 90 度指向下方（默认显示右箭头，展开时旋转为下箭头） */
.outline-panel__toggle:not(.outline-panel__toggle--collapsed):not(.outline-panel__toggle--placeholder) {
  transform: rotate(90deg);
}

/* 占位图标：无子标题时显示，透明度为 0 以保持对齐 */
.outline-panel__toggle--placeholder {
  opacity: 0;
}

/* 箭头 SVG 图标尺寸 */
.outline-panel__toggle-icon {
  width: 12px;
  height: 12px;
}

/* ==================== 条目文本样式 ==================== */

.outline-panel__item-text {
  /* 超长文本省略 */
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;

  /* 撑满剩余空间 */
  flex: 1;
}
</style>