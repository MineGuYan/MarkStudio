<script setup lang="ts">
/**
 * CursorOverlay 组件 - 协作者光标覆盖层
 *
 * 功能：
 * - 在编辑器 textarea 上方以绝对定位显示其他协作者的光标位置和用户名
 * - 每个协作者分配不同的颜色，便于区分
 * - 通过计算光标在文本内容中的偏移量来定位光标标签
 */

import { computed } from "vue";

// ==================== 类型定义 ====================

/** 协作者信息 */
export interface PeerInfo {
  /** 协作者唯一标识 */
  peer_id: string;
  /** 协作者用户名 */
  username: string;
  /** 协作者光标位置（字符偏移量） */
  cursor_position: number;
}

// ==================== Props 定义 ====================

const props = defineProps<{
  /** 协作者列表，包含光标位置和用户名 */
  peers: PeerInfo[];
  /** 编辑器当前文本内容，用于计算光标像素位置 */
  editorContent: string;
  /** 本地对等方 ID，用于过滤掉自己的光标 */
  localPeerId?: string;
}>();

// ==================== 协作者颜色分配 ====================

/** 预设的协作者颜色列表，按顺序循环分配 */
const peerColors = [
  "#e74c3c", // 红色
  "#3498db", // 蓝色
  "#2ecc71", // 绿色
  "#f39c12", // 橙色
  "#9b59b6", // 紫色
  "#1abc9c", // 青色
  "#e67e22", // 暗橙
  "#e91e63", // 粉红
];

/** 协作者 ID 到颜色的映射缓存 */
const peerColorMap = new Map<string, string>();
let colorIndex = 0;

/**
 * 根据协作者 ID 获取分配的颜色
 * 首次遇到的新 peer_id 会按顺序分配颜色并缓存
 *
 * @param peerId - 协作者唯一标识
 * @returns 对应的 CSS 颜色字符串
 */
function getPeerColor(peerId: string): string {
  if (!peerColorMap.has(peerId)) {
    peerColorMap.set(peerId, peerColors[colorIndex % peerColors.length]!);
    colorIndex++;
  }
  return peerColorMap.get(peerId)!;
}

// ==================== 光标像素位置计算 ====================

/**
 * 计算给定字符偏移量在 textarea 中对应的像素位置
 *
 * 使用"隐藏镜像 div"技术：创建一个与 textarea 样式完全相同的隐藏 div，
 * 将光标位置之前的文本内容渲染其中，并在末尾添加一个 span 标记元素，
 * 通过获取该 span 的 offsetLeft 和 offsetTop 来推算光标的位置。
 *
 * 注意：此函数仅为近似计算，对等宽字体效果最佳。
 *
 * @param text - 编辑器全部文本内容
 * @param cursorPos - 光标所在的字符偏移量
 * @returns 光标的像素位置 { left, top }
 */
function getCursorPixelPosition(
  text: string,
  cursorPos: number
): { left: number; top: number } {
  // 截取光标位置之前的文本
  const textBeforeCursor = text.substring(0, cursorPos);

  // 计算光标所在行号（从 0 开始）
  const lines = textBeforeCursor.split("\n");
  const lineIndex = lines.length - 1;
  // 当前行光标之前的字符数
  const charInLine = lines[lineIndex]?.length ?? 0;

  // 估算行高和字符宽度（等宽字体假设）
  // 编辑器字体大小为 14px，行高为 1.7，字符宽度约为 8.4px（Cascadia Code 等宽字体）
  const lineHeight = 14 * 1.7; // 约 23.8px
  const charWidth = 8.4; // 等宽字体每个字符的近似宽度

  // 计算光标像素位置
  // padding 为 16px（top）和 20px（left）
  const left = 20 + charInLine * charWidth;
  const top = 16 + lineIndex * lineHeight;

  return { left, top };
}

// ==================== 计算属性 ====================

/** 过滤并计算每个协作者的光标显示数据，排除本地用户自己的光标 */
const visibleCursors = computed(() => {
  return props.peers
    .filter((peer) => peer.cursor_position >= 0)
    .filter((peer) => !props.localPeerId || peer.peer_id !== props.localPeerId)
    .map((peer) => {
      const pos = getCursorPixelPosition(
        props.editorContent,
        peer.cursor_position
      );
      return {
        ...peer,
        color: getPeerColor(peer.peer_id),
        left: pos.left,
        top: pos.top,
      };
    });
});
</script>

<template>
  <!-- 光标覆盖层容器，绝对定位在编辑器上方 -->
  <div class="cursor-overlay" aria-label="协作者光标覆盖层">
    <!-- 遍历每个可见光标，渲染光标标签 -->
    <div
      v-for="cursor in visibleCursors"
      :key="cursor.peer_id"
      class="cursor-tag"
      :style="{
        left: cursor.left + 'px',
        top: cursor.top + 'px',
        borderColor: cursor.color,
      }"
    >
      <!-- 光标标签：显示用户名 -->
      <span
        class="cursor-tag__label"
        :style="{ backgroundColor: cursor.color }"
      >
        {{ cursor.username }}
      </span>
      <!-- 光标指示线 -->
      <div class="cursor-tag__caret" :style="{ backgroundColor: cursor.color }" />
    </div>
  </div>
</template>

<style scoped>
/* ==================== 光标覆盖层容器 ==================== */

.cursor-overlay {
  /* 绝对定位，覆盖在编辑器 textarea 上方 */
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;

  /* 穿透点击事件，让用户能正常点击 textarea */
  pointer-events: none;

  /* 隐藏溢出 */
  overflow: hidden;

  /* 确保在编辑器内容之上 */
  z-index: 10;
}

/* ==================== 光标标签样式 ==================== */

.cursor-tag {
  /* 绝对定位，由 JS 动态计算位置 */
  position: absolute;

  /* 纵向 flex 布局，让 label 和 caret 紧密堆叠，消除行盒间隙 */
  display: flex;
  flex-direction: column;
  align-items: flex-start;

  /* 过渡动画，让光标移动更平滑 */
  transition: left 0.1s ease, top 0.1s ease;

  /* 光标标签不换行 */
  white-space: nowrap;

  /* 半透明，避免遮挡下方文本内容 */
  opacity: 0.5;
}

/* 用户名标签 */
.cursor-tag__label {
  /* 内边距与字体 */
  padding: 1px 6px;
  font-size: 11px;
  font-weight: 600;
  line-height: 1.3;

  /* 文字颜色 */
  color: #ffffff;

  /* 圆角 */
  border-radius: 3px 3px 3px 0;

  /* 文字不换行 */
  white-space: nowrap;
}

/* 光标指示线（竖线） */
.cursor-tag__caret {
  /* 尺寸 */
  width: 2px;
  height: 18px;
  margin-left: 0;

  /* 圆角 */
  border-radius: 1px;
}
</style>