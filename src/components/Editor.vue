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
    /** 图片缓存目录路径（非协作模式下粘贴图片时使用的保存目录） */
    imageCacheDir?: string;
  }>(),
  {
    placeholder: "请输入 Markdown 内容...",
    collabEnabled: false,
    collabPeers: () => [],
    localPeerId: "",
    imageCacheDir: "",
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

/**
 * textarea 的本地显示值，独立于 modelValue
 *
 * 为何需要独立管理：
 * - 协作模式下，远程操作会更新 modelValue，若直接通过 :value="modelValue" 绑定到 textarea，
 *   则 IME 组合期间远程操作到达时会覆盖 textarea 内容，强制中断浏览器的 IME 组合会话，
 *   导致用户正在输入的中文拼音/日文假名等组合文字丢失。
 * - 通过独立的 textareaValue 管理，组合期间拒绝外部更新，保护 IME 会话完整性。
 * - 组合结束后，再将本地操作合并到当前 modelValue（可能包含组合期间到达的远程操作），
 *   确保本地与远程变更均被保留。
 */
const textareaValue = ref<string>(modelValue.value ?? "");

/** 是否正在应用远程操作（防止远程操作触发本地事件循环） */
const applyingRemote = ref(false);

/** 是否正在通过 IME 输入法组合文字（如中文输入法的拼音阶段） */
const isComposing = ref(false);

/** 上一次发送的光标位置，用于去重，避免重复发送相同位置 */
const lastEmittedPosition = ref<number>(-1);

// ==================== 监听器 ====================

/**
 * 监听 modelValue 外部变化，同步 textareaValue 和 previousText 快照
 *
 * 注意：
 * - IME 组合期间不更新 textareaValue，避免中断浏览器的 IME 组合会话
 * - IME 组合期间不更新 previousText，保持组合前的文本作为 diff 基准
 * - 远程操作（applyingRemote=true）时，previousText 由 applyRemoteOperation 负责更新，
 *   但组合期间同样跳过（见 applyRemoteOperation 中的保护逻辑）
 */
watch(
  () => modelValue.value,
  (newVal) => {
    const textarea = textareaRef.value;
    const safeValue = newVal ?? "";

    // IME 组合期间不更新 textarea 显示值，保护 IME 会话不被中断
    if (!isComposing.value) {
      // 保存当前光标/选区位置，用于远程文档同步后恢复
      // 若 textarea 未挂载或未聚焦，则无需恢复光标
      const savedStart = textarea?.selectionStart ?? null;
      const savedEnd = textarea?.selectionEnd ?? null;

      textareaValue.value = safeValue;

      // 恢复光标位置：仅当之前有有效选区且文本长度允许时恢复
      // 若远程编辑删除了光标之前的文本，将光标 clamp 到新文本末尾
      if (textarea && savedStart !== null && savedEnd !== null) {
        const maxPos = safeValue.length;
        const clampedStart = Math.min(savedStart, maxPos);
        const clampedEnd = Math.min(savedEnd, maxPos);
        // 使用 requestAnimationFrame 确保 Vue 的 DOM 更新已完成
        requestAnimationFrame(() => {
          textarea.selectionStart = clampedStart;
          textarea.selectionEnd = clampedEnd;
        });
      }
    }
    // IME 组合期间不更新快照，保持组合前的文本作为 diff 基准
    if (!applyingRemote.value && !isComposing.value) {
      previousText.value = safeValue;
    }
  }
);

// ==================== 方法 ====================

/**
 * 处理编辑器 input 事件
 * 对比新旧文本值，生成对应的 OT 操作（Insert 或 Delete）
 *
 * IME 组合期间的处理策略：
 * - 不更新 modelValue（避免将中间态拼音传播到父组件）
 * - 不生成 OT 操作（中间态文本不是最终的中文字符）
 * - 仅更新 textareaValue（反映 textarea 的实际显示内容）
 *
 * 组合结束后的处理策略：
 * - 将本地 OT 操作应用到当前 modelValue（可能包含组合期间到达的远程操作）
 * - 合并本地与远程变更，确保两者均被保留
 *
 * @param event - InputEvent 事件对象
 */
async function handleInput(event: Event): Promise<void> {
  const target = event.target as HTMLTextAreaElement;
  const newText = target.value;
  const oldText = previousText.value;

  // 始终更新本地 textarea 显示值
  textareaValue.value = newText;

  // IME 组合期间（如中文拼音输入）不更新 modelValue，也不生成 OT 操作
  // 原因：组合期间的文本是中间态（拼音字母），不是最终的中文字符
  if (isComposing.value) {
    return;
  }

  // 协作模式下，将本地操作应用到当前 modelValue（可能包含组合期间到达的远程操作）
  // 这样做可以保留远程变更，同时应用本地变更，避免数据丢失
  if (props.collabEnabled && !applyingRemote.value) {
    // 调用后端 IPC 计算新旧文本之间的 OT 操作
    const opsJson = await invoke<string>("compute_operation_cmd", { oldText, newText });
    const ops: Operation[] = JSON.parse(opsJson);
    if (ops.length > 0) {
      const currentModel = modelValue.value ?? "";
      let mergedText = currentModel;
      // 逐个应用操作到当前模型，合并可能存在的远程变更
      for (const op of ops) {
        mergedText = await invoke<string>("apply_operation_cmd", { text: mergedText, opJson: JSON.stringify(op) });
        emit("collabOperation", op);
      }

      emit("update:modelValue", mergedText);
      previousText.value = mergedText;
      textareaValue.value = mergedText;
      handleCursorChange();
      return;
    }
  }

  // 非协作模式或无变化：直接更新 modelValue
  emit("update:modelValue", newText);
  previousText.value = newText;
  handleCursorChange();
}

/**
 * 处理 IME 输入法组合开始事件
 * 设置组合状态标志，防止组合期间的 input 事件生成错误的 OT 操作
 */
function handleCompositionStart(): void {
  isComposing.value = true;
}

/**
 * 处理 IME 输入法组合结束事件
 *
 * 关键设计：在此处主动处理组合结果，而非依赖后续的 input 事件。
 *
 * 原因：在 Chrome 等主流浏览器中，IME 组合的最终 input 事件（携带确认后的中文文本）
 * 在 compositionend 之前触发，此时 isComposing 仍为 true，handleInput 会跳过 OT 操作生成。
 * 因此必须在 compositionend 中主动处理，否则中文输入永远不会被同步。
 *
 * 如果后续 input 事件再次触发（某些浏览器的行为），由于 previousText 已更新，
 * 后端将返回空操作数组（无差异），不会重复发送 collabOperation。
 */
function handleCompositionEnd(): void {
  isComposing.value = false;

  // 主动处理组合结果，确保中文输入能被同步
  processCompositionResult();
}

/**
 * 处理 IME 组合的最终结果
 *
 * 将组合期间积累的文本变更（textarea 中的最终中文文本 vs 组合前的 previousText）
 * 通过后端 IPC 计算 OT 操作，合并到当前 modelValue 后 emit。
 *
 * 此函数由 handleCompositionEnd 调用，也可能被后续的 input 事件间接触发
 * （此时 previousText 已更新，后端返回空操作数组无副作用）。
 */
async function processCompositionResult(): Promise<void> {
  const textarea = textareaRef.value;
  if (!textarea) return;

  const newText = textarea.value;
  const oldText = previousText.value;

  // 无变化则跳过
  if (newText === oldText) return;

  // 确保 textareaValue 与 DOM 同步
  textareaValue.value = newText;

  if (props.collabEnabled && !applyingRemote.value) {
    // 调用后端 IPC 计算新旧文本之间的 OT 操作
    const opsJson = await invoke<string>("compute_operation_cmd", { oldText, newText });
    const ops: Operation[] = JSON.parse(opsJson);
    if (ops.length > 0) {
      const currentModel = modelValue.value ?? "";
      let mergedText = currentModel;
      // 逐个应用操作到当前模型，合并可能存在的远程变更
      for (const op of ops) {
        mergedText = await invoke<string>("apply_operation_cmd", { text: mergedText, opJson: JSON.stringify(op) });
        emit("collabOperation", op);
      }

      emit("update:modelValue", mergedText);
      previousText.value = mergedText;
      textareaValue.value = mergedText;
      handleCursorChange();
      return;
    }
  }

  // 非协作模式或无有效操作：直接同步
  emit("update:modelValue", newText);
  previousText.value = newText;
  handleCursorChange();
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
 * 处理粘贴事件，拦截剪贴板中的图片进行保存并插入 Markdown 图片语法
 *
 * 协作模式下：
 * - 将图片保存到临时目录，通过协作会话发送给对等方
 * - 在 Markdown 中插入 `![image](local_path)` 语法
 *
 * 非协作模式下：
 * - 将图片保存到用户配置的缓存目录（默认 data/image_cache/）
 * - 在 Markdown 中插入 `![image](relative_path)` 语法
 *
 * @param event - ClipboardEvent 粘贴事件对象
 */
async function handlePaste(event: ClipboardEvent): Promise<void> {
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

        // 调用后端 IPC 统一处理图片粘贴（保存图片、更新内容、生成 OT 操作）
        // 注意：将路径中的反斜杠替换为正斜杠，因为 Markdown 中 \ 是转义字符，
        // 如果不替换，pulldown-cmark 会将 \图、\T 等转义处理，导致路径错误
        const textarea = textareaRef.value;
        if (textarea) {
          const cursorPos = textarea.selectionStart;
          const content = modelValue.value ?? "";

          const resultJson = await invoke<string>("paste_image_cmd", {
            base64Data: base64Content,
            fileName,
            content,
            cursorPos,
            cacheDir: props.imageCacheDir || null,
            collabEnabled: props.collabEnabled,
          });

          const result = JSON.parse(resultJson);
          const newContent: string = result.new_content;
          const newFilePath: string = result.file_path;
          const operationJson: string | null = result.operation_json;

          // 更新文档内容
          emit("update:modelValue", newContent);
          previousText.value = newContent;
          textareaValue.value = newContent;

          // 协作模式下，发送 OT 操作给其他对等方
          // 注意：operation_json 是单个 Operation 对象的 JSON 字符串，不是数组
          if (props.collabEnabled && operationJson) {
            const op: Operation = JSON.parse(operationJson);
            emit("collabOperation", op);

            // 同时发送图片文件给协作对等方（OT 操作只同步文本引用，图片文件需要单独传输）
            try {
              await invoke("send_collab_image", { filePath: newFilePath });
            } catch (err) {
              console.error("发送协作图片失败:", err);
            }
          }

          // 将光标移动到插入图片引用之后
          const normalizedPath = newFilePath.replace(/\\/g, "/");
          const imageMarkdown = `![image](${normalizedPath})`;
          setTimeout(() => {
            textarea.selectionStart = cursorPos + imageMarkdown.length;
            textarea.selectionEnd = cursorPos + imageMarkdown.length;
            textarea.focus();
          }, 0);
        }
      } catch (err) {
        console.error("图片粘贴失败:", err);
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
 * IME 组合期间的保护：
 * - 仍然更新 modelValue（确保数据模型包含远程变更）
 * - 但不更新 previousText（保持组合前的快照，用于组合结束后正确计算 diff）
 * - textareaValue 由 watch 根据 isComposing 状态决定是否同步
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

  // 更新 modelValue（watch 会根据 isComposing 决定是否同步到 textareaValue）
  modelValue.value = newText;

  // IME 组合期间不更新 previousText，保持组合前的快照
  // 这样组合结束后 handleInput 可以正确计算本地 diff
  if (!isComposing.value) {
    previousText.value = newText;
  }

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
      :value="textareaValue"
      :placeholder="placeholder"
      @input="handleInput"
      @compositionstart="handleCompositionStart"
      @compositionend="handleCompositionEnd"
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
      :textarea-ref="textareaRef"
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