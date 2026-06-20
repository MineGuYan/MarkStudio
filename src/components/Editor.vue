<script setup lang="ts">
/**
 * Editor 组件 - Markdown 源代码编辑器
 *
 * 功能：
 * - 提供 textarea 文本编辑区域，用于编辑 Markdown 源代码
 * - 通过 v-model 双向绑定内容，支持父组件同步更新
 * - 提供 placeholder 占位提示文字，引导用户输入
 * - 协作模式下生成 OT（Operational Transformation）操作
 * - 协作模式下发送光标位置
 * - 集成 CursorOverlay 组件显示远程协作者光标
 */

import { ref, watch } from "vue";
import CursorOverlay from "./CursorOverlay.vue";
import type { PeerInfo } from "./CursorOverlay.vue";
import { invoke } from "@tauri-apps/api/core";

// ==================== 类型定义 ====================

/** OT 插入操作 */
interface InsertOperation {
  Insert: {
    /** 插入位置（字符偏移量） */
    position: number;
    /** 插入的文本内容 */
    text: string;
  };
}

/** OT 删除操作 */
interface DeleteOperation {
  Delete: {
    /** 删除起始位置（字符偏移量） */
    position: number;
    /** 删除的字符长度 */
    length: number;
  };
}

/** OT 操作联合类型 */
export type Operation = InsertOperation | DeleteOperation;

// ==================== Props 定义 ====================

/** 编辑器内容模型，通过 v-model 与父组件双向绑定 */
const modelValue = defineModel<string>("modelValue", { required: true });

/** 编辑器空白时的占位提示文字 */
const props = withDefaults(
  defineProps<{
    placeholder?: string;
    /** 是否启用协作模式 */
    collabEnabled?: boolean;
    /** 协作者列表，传给 CursorOverlay */
    collabPeers?: PeerInfo[];
    /** 本地对等方 ID，用于过滤掉自己的光标 */
    localPeerId?: string;
  }>(),
  {
    placeholder: "请输入 Markdown 内容...",
    collabEnabled: false,
    collabPeers: () => [],
    localPeerId: "",
  }
);

// ==================== Emits 定义 ====================

const emit = defineEmits<{
  /** v-model 双向绑定更新事件 */
  "update:modelValue": [value: string];
  /** 协作模式下的本地编辑操作 */
  collabOperation: [op: Operation];
  /** 协作模式下的光标位置变化 */
  collabCursor: [position: number];
}>();

// ==================== 协作状态 ====================

/** 上一次的文本内容快照，用于对比生成 OT 操作 */
const previousText = ref<string>(modelValue.value ?? "");

/** textarea 元素引用，用于获取光标位置 */
const textareaRef = ref<HTMLTextAreaElement | null>(null);

/** 是否正在应用远程操作（防止远程操作触发本地事件循环） */
const applyingRemote = ref(false);

/** 上一次发送的光标位置，用于去重，避免重复发送相同位置 */
const lastEmittedPosition = ref<number>(-1);

// ==================== 监听器 ====================

/**
 * 监听 modelValue 外部变化，更新 previousText 快照
 * 当外部（如父组件）直接修改内容时同步快照，避免误生成 OT 操作
 */
watch(
  () => modelValue.value,
  (newVal) => {
    if (!applyingRemote.value) {
      previousText.value = newVal ?? "";
    }
  }
);

// ==================== 方法 ====================

/**
 * 处理编辑器 input 事件
 * 对比新旧文本值，生成对应的 OT 操作（Insert 或 Delete）
 *
 * @param event - InputEvent 事件对象
 */
function handleInput(event: Event): void {
  const target = event.target as HTMLTextAreaElement;
  const newText = target.value;
  const oldText = previousText.value;

  // 更新 v-model 绑定
  emit("update:modelValue", newText);

  // 仅在协作模式下生成 OT 操作
  if (!props.collabEnabled || applyingRemote.value) {
    previousText.value = newText;
    return;
  }

  // 对比新旧文本，计算操作
  const op = computeOperation(oldText, newText);
  if (op) {
    emit("collabOperation", op);
  }

  // 更新快照
  previousText.value = newText;

  // 同步光标位置（输入会导致光标移动，覆盖粘贴等非键盘操作场景）
  handleCursorChange();
}

/**
 * 计算新旧文本之间的 OT 操作
 *
 * 算法：
 * 1. 找到新旧文本首次出现差异的位置
 * 2. 如果新文本更长 → 在该位置插入了一段文本 → 生成 Insert 操作
 * 3. 如果新文本更短 → 在该位置删除了一段文本 → 生成 Delete 操作
 *
 * @param oldText - 旧文本内容
 * @param newText - 新文本内容
 * @returns OT 操作，如果无变化则返回 null
 */
function computeOperation(
  oldText: string,
  newText: string
): Operation | null {
  // 找到第一个不同的字符位置
  let diffStart = 0;
  const minLen = Math.min(oldText.length, newText.length);
  while (diffStart < minLen && oldText[diffStart] === newText[diffStart]) {
    diffStart++;
  }

  // 无变化
  if (diffStart === oldText.length && diffStart === newText.length) {
    return null;
  }

  // 计算插入或删除
  if (newText.length > oldText.length) {
    // 插入操作：新文本更长，在 diffStart 位置插入了字符
    const insertedText = newText.substring(diffStart, diffStart + (newText.length - oldText.length));
    return {
      Insert: {
        position: diffStart,
        text: insertedText,
      },
    };
  } else if (newText.length < oldText.length) {
    // 删除操作：新文本更短，在 diffStart 位置删除了字符
    const deletedLength = oldText.length - newText.length;
    return {
      Delete: {
        position: diffStart,
        length: deletedLength,
      },
    };
  }

  // 长度相同但内容不同 → 替换操作（先删除后插入）
  const deletedLength = oldText.length - diffStart;
  const insertedText = newText.substring(diffStart);
  // 简化处理：返回 Delete + Insert 的组合（分两次 emit）
  // 先发 Delete，后发 Insert
  emit("collabOperation", {
    Delete: {
      position: diffStart,
      length: deletedLength,
    },
  });
  return {
    Insert: {
      position: diffStart,
      text: insertedText,
    },
  };
}

/**
 * 处理光标位置变化（select、click 和 keyup 事件）
 * 在协作模式下，将当前光标位置发送给后端
 * 包含去重逻辑：连续相同位置不会重复发送
 */
function handleCursorChange(): void {
  if (!props.collabEnabled) return;

  const textarea = textareaRef.value;
  if (!textarea) return;

  // 获取当前光标位置（selectionStart 即为字符偏移量）
  const position = textarea.selectionStart;

  // 去重：位置未变化时跳过，避免重复发送
  if (position === lastEmittedPosition.value) return;

  lastEmittedPosition.value = position;
  emit("collabCursor", position);
}

/**
 * 处理粘贴事件，拦截剪贴板中的图片进行协作同步
 *
 * 在协作模式下，如果用户粘贴的内容包含图片，
 * 则将图片保存到本地临时目录，通过协作会话发送给对等方，
 * 并在 Markdown 中插入 `![image](local_path)` 语法。
 * 非图片的文本粘贴行为保持不变。
 *
 * @param event - ClipboardEvent 粘贴事件对象
 */
async function handlePaste(event: ClipboardEvent): Promise<void> {
  // 仅在协作模式下处理图片粘贴
  if (!props.collabEnabled) return;

  const clipboardData = event.clipboardData;
  if (!clipboardData) return;

  const items = clipboardData.items;
  if (!items) return;

  // 遍历剪贴板中的所有项，查找图片类型
  for (let i = 0; i < items.length; i++) {
    const item = items[i];

    // 检查是否为图片类型
    if (item.type.startsWith("image/")) {
      // 阻止默认粘贴行为（文本区域会尝试粘贴图片的二进制数据）
      event.preventDefault();

      const blob = item.getAsFile();
      if (!blob) continue;

      try {
        // 将图片 Blob 读取为 Base64 编码的 data URI
        const base64Data = await readBlobAsBase64(blob);

        // 提取纯 Base64 数据（去除 data:image/png;base64, 前缀）
        const base64Content = base64Data.split(",")[1] || base64Data;

        // 生成唯一的文件名（基于时间戳和随机数）
        const ext = item.type.split("/")[1] || "png";
        const fileName = `paste_${Date.now()}_${Math.random().toString(36).substring(2, 8)}.${ext}`;

        // 调用后端命令将图片保存到临时文件
        const filePath = await invoke<string>("save_temp_image", {
          dataBase64: base64Content,
          fileName,
        });

        // 调用后端命令将图片发送给协作对等方
        await invoke("send_collab_image", {
          filePath,
        });

        // 在光标位置插入 Markdown 图片语法
        const textarea = textareaRef.value;
        if (textarea) {
          const cursorPos = textarea.selectionStart;
          const currentText = modelValue.value ?? "";
          const imageMarkdown = `![image](${filePath})`;

          // 在光标位置插入图片引用
          const newText =
            currentText.substring(0, cursorPos) +
            imageMarkdown +
            currentText.substring(cursorPos);

          // 更新编辑器内容
          emit("update:modelValue", newText);
          previousText.value = newText;

          // 将光标移动到插入图片引用之后
          // 使用 setTimeout 确保 DOM 更新后再设置光标位置
          setTimeout(() => {
            textarea.selectionStart = cursorPos + imageMarkdown.length;
            textarea.selectionEnd = cursorPos + imageMarkdown.length;
            textarea.focus();
          }, 0);
        }
      } catch (err) {
        console.error("协作图片粘贴失败:", err);
      }

      // 只处理第一张图片
      break;
    }
  }
}

/**
 * 将 Blob 对象读取为 Base64 编码的 data URI 字符串
 *
 * @param blob - 要读取的 Blob 对象
 * @returns Base64 编码的 data URI 字符串
 */
function readBlobAsBase64(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onloadend = () => {
      resolve(reader.result as string);
    };
    reader.onerror = () => {
      reject(new Error("文件读取失败"));
    };
    reader.readAsDataURL(blob);
  });
}

/**
 * 应用远程 OT 操作到本地编辑器
 * 此方法通过 defineExpose 暴露给父组件调用
 *
 * @param op - 远程 OT 操作
 */
function applyRemoteOperation(op: Operation): void {
  applyingRemote.value = true;

  const currentText = modelValue.value ?? "";

  let newText: string;

  if ("Insert" in op) {
    // 在指定位置插入文本
    const { position, text } = op.Insert;
    newText = currentText.substring(0, position) + text + currentText.substring(position);
  } else if ("Delete" in op) {
    // 删除指定位置的文本
    const { position, length } = op.Delete;
    newText = currentText.substring(0, position) + currentText.substring(position + length);
  } else {
    applyingRemote.value = false;
    return;
  }

  // 更新文本内容
  modelValue.value = newText;
  previousText.value = newText;

  // 使用 nextTick 确保 DOM 更新后再重置标志
  setTimeout(() => {
    applyingRemote.value = false;
  }, 0);
}

// ==================== 暴露给父组件的方法 ====================

defineExpose({
  /** 应用远程 OT 操作 */
  applyRemoteOperation,
});
</script>

<template>
  <!-- 编辑器容器，设置 relative 定位以支持 CursorOverlay 的绝对定位 -->
  <div class="editor-container">
    <!-- Markdown 源代码编辑区域 -->
    <textarea
      ref="textareaRef"
      class="editor-textarea"
      :value="modelValue"
      :placeholder="placeholder"
      @input="handleInput"
      @paste="handlePaste"
      @select="handleCursorChange"
      @click="handleCursorChange"
      @keyup="handleCursorChange"
    />

    <!-- 协作光标覆盖层：仅在协作模式下显示 -->
    <CursorOverlay
      v-if="collabEnabled && collabPeers.length > 0"
      :peers="collabPeers"
      :editor-content="modelValue ?? ''"
      :local-peer-id="localPeerId"
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

  /* relative 定位，为 CursorOverlay 的绝对定位提供参照 */
  position: relative;
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