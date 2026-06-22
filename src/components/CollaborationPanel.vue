<script setup lang="ts">
/**
 * CollaborationPanel 组件 - 协作面板
 *
 * 功能：
 * - 未连接状态：显示"创建房间"和"加入房间"按钮
 * - 创建房间对话框：输入端口号、密码、用户名，创建房间
 * - 加入房间对话框：输入主机IP、端口、房间ID、密码、用户名，加入房间
 * - 已连接状态：显示房间信息、在线用户列表、离开按钮
 * - 连接状态指示器
 */

import { ref, reactive, computed, onMounted, onUnmounted } from "vue";
import { invoke } from "@tauri-apps/api/core";

// ==================== 类型定义 ====================

/** 房间信息 */
export interface RoomInfo {
  /** 房间唯一标识 */
  room_id: string;
  /** 主机 IP 地址 */
  host_ip: string;
  /** 端口号 */
  port: number;
  /** 在线用户数 */
  peer_count: number;
}

/** 协作者信息 */
export interface PeerInfo {
  /** 协作者唯一标识 */
  peer_id: string;
  /** 协作者用户名 */
  username: string;
  /** 协作者光标位置 */
  cursor_position: number;
  /** 是否为房间主机 */
  is_host: boolean;
}

/** 协作状态 */
export interface CollabStatus {
  /** 是否已连接 */
  connected: boolean;
  /** 房间 ID */
  room_id: string;
  /** 是否为房间主机 */
  is_host: boolean;
  /** 在线用户数 */
  peer_count: number;
  /** 协作者列表 */
  peers: PeerInfo[];
  /** 本地对等方 ID（用于区分"我"和他人） */
  local_peer_id: string;
  /** 本地用户名 */
  local_username: string;
  /** 共享文档内容 */
  document: string;
  /** 当前协作编辑的文档对应的共享文件路径 */
  current_document_path: string | null;
  /** 共享文件列表 */
  shared_files: SharedFileInfo[];
  /** 断开连接原因（当 connected 为 false 时，可能包含原因说明） */
  disconnect_reason: string | null;
}

/** 共享文件信息 */
export interface SharedFileInfo {
  /** 文件完整路径 */
  path: string;
  /** 文件显示名称 */
  title: string;
  /** 文件内容 */
  content: string;
}

// ==================== Props 定义 ====================

/** 当前编辑器中的文档内容，创建房间时作为初始文档同步给加入者 */
const props = defineProps<{
  currentDocument?: string;
  /** 当前打开的标签页列表（用于创建房间时选择共享文件） */
  tabs?: { id: number; path: string; title: string; content: string }[];
  /** 当前活跃标签页的 ID */
  activeTabId?: number;
}>();

// ==================== Emits 定义 ====================

const emit = defineEmits<{
  /** 连接状态变更 */
  "connection-change": [connected: boolean];
  /** 协作者列表更新 */
  "peers-update": [peers: PeerInfo[]];
  /** 远程文档内容更新（包含文档内容和对应路径） */
  "document-update": [update: { document: string; path: string | null }];
  /** 本地对等方 ID 更新（用于过滤自己的光标） */
  "local-peer-id": [peerId: string];
  /** 从共享文件列表打开文件 */
  "open-file": [path: string];
}>();

// ==================== 状态管理 ====================

/** 连接状态：disconnected | connecting | connected */
const connectionStatus = ref<"disconnected" | "connecting" | "connected">(
  "disconnected"
);

/** 当前房间信息 */
const roomInfo = ref<RoomInfo | null>(null);

/** 在线用户列表 */
const peers = ref<PeerInfo[]>([]);

/** 是否为房间主机 */
const isHost = ref(false);

/** 本地用户名 */
const localUsername = ref("");

/** 本地对等方 ID（用于精确匹配"我"） */
const localPeerId = ref("");

/** 错误消息 */
const errorMessage = ref("");

/** 成功消息 */
const successMessage = ref("");

/** 复制反馈文本（为空时不显示） */
const copiedLabel = ref("");

/** 共享文件列表 */
const sharedFiles = ref<SharedFileInfo[]>([]);

/** 共享文件右键菜单是否可见 */
const fileMenuVisible = ref(false);

/** 共享文件右键菜单位置 */
const fileMenuX = ref(0);
const fileMenuY = ref(0);

/** 当前右键菜单对应的文件 */
const fileMenuTarget = ref<SharedFileInfo | null>(null);

/** 状态轮询定时器 ID（用于清除定时器） */
let statusPollTimer: ReturnType<typeof setInterval> | null = null;

/** 上一次轮询的文档内容（用于检测文档变化） */
let lastDocument = "";

/** 轮询间隔（毫秒） */
const POLL_INTERVAL_MS = 500;

// ==================== 对话框消息状态 ====================

/** 创建房间对话框错误消息 */
const createDialogError = ref("");
/** 加入房间对话框错误消息 */
const joinDialogError = ref("");

// ==================== 对话框状态 ====================

/** 是否显示创建房间对话框 */
const showCreateDialog = ref(false);

/** 是否显示加入房间对话框 */
const showJoinDialog = ref(false);

/** 是否显示添加共享文件对话框 */
const showAddSharedDialog = ref(false);

/** 创建房间时选中的标签页 ID 集合 */
const selectedTabIds = ref<Set<number>>(new Set());

/** 添加共享文件时选中的标签页 ID 列表 */
const addSharedSelectedIds = ref<number[]>([]);

/** 未共享的标签页列表（计算属性：排除已在共享列表中的标签页） */
const unsharedTabs = computed(() => {
  if (!props.tabs) return [];
  const sharedPaths = new Set(sharedFiles.value.map((f) => f.path));
  return props.tabs.filter((t) => t.path && !sharedPaths.has(t.path));
});

// ==================== 创建房间表单 ====================

/** 创建房间表单数据 */
const createForm = reactive({
  port: 8080,
  password: "",
  username: "",
});

/** 创建房间表单加载状态 */
const createLoading = ref(false);

// ==================== 加入房间表单 ====================

/** 加入房间表单数据 */
const joinForm = reactive({
  host: "",
  port: 8080,
  roomId: "",
  password: "",
  username: "",
});

/** 加入房间表单加载状态 */
const joinLoading = ref(false);

// ==================== 方法 ====================

/**
 * 切换标签页选择状态（创建房间对话框中的多选）
 *
 * @param tabId - 标签页 ID
 */
function toggleTabSelection(tabId: number): void {
  const newSet = new Set(selectedTabIds.value);
  if (newSet.has(tabId)) {
    newSet.delete(tabId);
  } else {
    newSet.add(tabId);
  }
  selectedTabIds.value = newSet;
}

/**
 * 添加选中的标签页到共享文件列表
 * 调用后端 add_shared_file 命令逐个添加
 */
async function handleAddSharedFiles(): Promise<void> {
  if (!props.tabs) return;

  for (const tabId of addSharedSelectedIds.value) {
    const tab = props.tabs.find((t) => t.id === tabId);
    if (!tab) continue;

    try {
      await invoke("add_shared_file", {
        path: tab.path,
        title: tab.title,
        content: tab.content,
      });
    } catch (error) {
      console.error("添加共享文件失败:", error);
    }
  }

  // 刷新共享文件列表
  try {
    const filesJson = await invoke<string>("get_shared_files");
    sharedFiles.value = JSON.parse(filesJson);
  } catch (error) {
    console.error("获取共享文件列表失败:", error);
  }

  showAddSharedDialog.value = false;
  addSharedSelectedIds.value = [];
}

/**
 * 从共享文件列表中移除指定文件
 *
 * @param path - 文件路径
 */
async function handleRemoveSharedFile(path: string): Promise<void> {
  try {
    await invoke("remove_shared_file", { path });
    // 刷新本地共享文件列表
    sharedFiles.value = sharedFiles.value.filter((f) => f.path !== path);
  } catch (error) {
    console.error("移除共享文件失败:", error);
  }
}

/**
 * 处理共享文件右键菜单
 *
 * @param event - 鼠标事件
 * @param file - 被右键点击的共享文件
 */
function handleFileContextMenu(event: MouseEvent, file: SharedFileInfo): void {
  event.preventDefault();
  fileMenuTarget.value = file;
  fileMenuX.value = event.clientX;
  fileMenuY.value = event.clientY;
  fileMenuVisible.value = true;
}

/** 关闭共享文件右键菜单 */
function closeFileMenu(): void {
  fileMenuVisible.value = false;
  fileMenuTarget.value = null;
}

/**
 * 处理右键菜单的"移除共享"操作
 */
async function handleRemoveSharedFromMenu(): Promise<void> {
  const file = fileMenuTarget.value;
  if (!file) return;
  closeFileMenu();
  await handleRemoveSharedFile(file.path);
}

/**
 * 显示成功消息，3 秒后自动清除
 *
 * @param msg - 成功消息文本
 */
function showSuccess(msg: string): void {
  successMessage.value = msg;
  errorMessage.value = "";
  setTimeout(() => {
    successMessage.value = "";
  }, 3000);
}

/**
 * 创建协作房间
 * 调用后端 IPC 命令创建房间，成功后更新连接状态并通知父组件
 */
async function handleCreateRoom(): Promise<void> {
  createDialogError.value = "";

  // 表单验证：端口号必须在有效范围内
  if (!createForm.port || createForm.port < 1 || createForm.port > 65535) {
    createDialogError.value = "请输入有效的端口号（1-65535）";
    return;
  }

  // 表单验证：用户名不能为空
  const trimmedUsername = createForm.username.trim();
  if (!trimmedUsername) {
    createDialogError.value = "请输入用户名";
    return;
  }

  createLoading.value = true;

  try {
    // 调用后端 IPC 创建房间，基于当前编辑器内容作为协作的初始文档
    const result = await invoke<RoomInfo>("create_collab_room", {
      port: createForm.port,
      password: createForm.password,
      username: trimmedUsername,
      document: props.currentDocument ?? "",
    });

    // 更新房间信息
    roomInfo.value = result;
    isHost.value = true;
    localUsername.value = trimmedUsername;
    connectionStatus.value = "connected";

    // 添加选中的共享文件
    if (props.tabs && selectedTabIds.value.size > 0) {
      for (const tabId of selectedTabIds.value) {
        const tab = props.tabs.find((t) => t.id === tabId);
        if (tab && tab.path) {
          try {
            await invoke("add_shared_file", {
              path: tab.path,
              title: tab.title,
              content: tab.content,
            });
          } catch (e) {
            console.error("添加共享文件失败:", tab.path, e);
          }
        }
      }
    }

    // 关闭对话框
    showCreateDialog.value = false;

    showSuccess("房间创建成功！");

    // 启动协作状态轮询（获取成员列表和文档同步）
    startStatusPolling();

    // 通知父组件连接状态变更（父组件 App.vue 负责协调状态轮询）
    emit("connection-change", true);
  } catch (error) {
    createDialogError.value = `创建房间失败: ${error}`;
  } finally {
    createLoading.value = false;
  }
}

/**
 * 加入协作房间
 * 调用后端 IPC 命令加入房间，成功后更新连接状态并通知父组件
 */
async function handleJoinRoom(): Promise<void> {
  joinDialogError.value = "";

  // 表单验证：房间 ID 不能为空
  if (!joinForm.roomId.trim()) {
    joinDialogError.value = "请输入房间 ID";
    return;
  }

  // 表单验证：主机 IP 地址不能为空
  if (!joinForm.host.trim()) {
    joinDialogError.value = "请输入主机 IP 地址";
    return;
  }

  // 表单验证：端口号必须在有效范围内
  if (!joinForm.port || joinForm.port < 1 || joinForm.port > 65535) {
    joinDialogError.value = "请输入有效的端口号（1-65535）";
    return;
  }

  // 表单验证：用户名不能为空
  const trimmedUsername = joinForm.username.trim();
  if (!trimmedUsername) {
    joinDialogError.value = "请输入用户名";
    return;
  }

  joinLoading.value = true;

  try {
    // 调用后端 IPC 加入房间
    const result = await invoke<RoomInfo>("join_collab_room", {
      host: joinForm.host.trim(),
      port: joinForm.port,
      roomId: joinForm.roomId.trim(),
      password: joinForm.password,
      username: trimmedUsername,
    });

    // 更新房间信息
    roomInfo.value = result;
    isHost.value = false;
    localUsername.value = trimmedUsername;
    connectionStatus.value = "connected";

    // 关闭对话框
    showJoinDialog.value = false;

    showSuccess("成功加入房间！");

    // 启动协作状态轮询（获取成员列表和文档同步）
    startStatusPolling();

    // 通知父组件连接状态变更（父组件 App.vue 负责协调状态轮询）
    emit("connection-change", true);
  } catch (error) {
    joinDialogError.value = `加入房间失败: ${error}`;
  } finally {
    joinLoading.value = false;
  }
}

/**
 * 离开协作房间
 * 调用后端 IPC 命令离开房间，清理状态
 */
async function handleLeaveRoom(): Promise<void> {
  // 先停止轮询，防止在离开过程中触发更新
  stopStatusPolling();

  try {
    await invoke("leave_collab_room");
  } catch (error) {
    console.error("离开房间失败:", error);
  }

  // 重置连接状态
  connectionStatus.value = "disconnected";
  roomInfo.value = null;
  peers.value = [];
  isHost.value = false;
  localUsername.value = "";
  localPeerId.value = "";
  sharedFiles.value = [];

  // 通知父组件断开连接
  emit("connection-change", false);
  emit("peers-update", []);
  emit("local-peer-id", "");
}

/**
 * 启动协作状态轮询
 *
 * 定期调用后端 `get_collab_status` 命令获取最新的协作状态，
 * 包括成员列表、文档内容、光标位置等，并同步更新本地状态和通知父组件。
 *
 * 这是解决"成员列表不显示"和"内容不同步"两个 bug 的关键机制。
 */
function startStatusPolling(): void {
  // 避免重复启动
  if (statusPollTimer !== null) return;

  statusPollTimer = setInterval(async () => {
    try {
      // 调用后端获取协作状态（返回 JSON 字符串）
      const statusJson = await invoke<string>("get_collab_status");
      const status: CollabStatus = JSON.parse(statusJson);

      // 如果连接已断开，停止轮询
      if (!status.connected) {
        stopStatusPolling();
        connectionStatus.value = "disconnected";
        // 如果有断开原因（如主机关闭房间），展示给用户
        if (status.disconnect_reason) {
          errorMessage.value = status.disconnect_reason;
        }
        emit("connection-change", false);
        return;
      }

      // 更新成员列表（触发 UI 响应式更新）
      peers.value = status.peers;

      // 更新本地对等方 ID（用于标记"我"）
      if (status.local_peer_id !== localPeerId.value) {
        localPeerId.value = status.local_peer_id;
        emit("local-peer-id", status.local_peer_id);
      }

      // 更新房间人数（roomInfo 中的 peer_count）
      if (roomInfo.value) {
        roomInfo.value = {
          ...roomInfo.value,
          peer_count: status.peer_count,
        };
      }

      // 通知父组件成员列表变更
      emit("peers-update", status.peers);

      // 仅当文档内容实际变化时，才通知父组件文档更新
      if (status.document !== lastDocument) {
        lastDocument = status.document;
        // 传递文档内容和对应的路径，以便父组件找到对应标签页并更新
        emit("document-update", {
          document: status.document,
          path: status.current_document_path,
        });
      }

      // 更新共享文件列表
      if (status.shared_files) {
        sharedFiles.value = status.shared_files;
      }
    } catch (error) {
      console.error("协作状态轮询失败:", error);
    }
  }, POLL_INTERVAL_MS);
}

/**
 * 停止协作状态轮询
 *
 * 清除定时器，释放资源。
 */
function stopStatusPolling(): void {
  if (statusPollTimer !== null) {
    clearInterval(statusPollTimer);
    statusPollTimer = null;
  }
  // 重置文档追踪，确保下次连接后能正确触发首次 document-update
  lastDocument = "";
}

// ==================== 组件生命周期 ====================

/**
 * 组件挂载时注册全局点击监听，用于关闭右键菜单
 */
onMounted(() => {
  document.addEventListener("click", handleGlobalClick);
});

// ==================== 组件卸载清理 ====================

/**
 * 组件卸载时清理轮询定时器，防止内存泄漏
 */
onUnmounted(() => {
  stopStatusPolling();
  document.removeEventListener("click", handleGlobalClick);
});

/**
 * 全局点击事件处理
 * 点击右键菜单外部区域时关闭菜单
 *
 * @param event - 鼠标事件
 */
function handleGlobalClick(event: MouseEvent): void {
  if (fileMenuVisible.value) {
    const menu = document.querySelector(".collab-panel__file-menu");
    if (menu && !menu.contains(event.target as Node)) {
      closeFileMenu();
    }
  }
}

// ==================== 工具函数 ====================

/**
 * 复制文本到剪贴板并显示短暂反馈
 *
 * @param text - 要复制的文本
 * @param label - 反馈标签（如 "房间ID"），用于显示"房间ID 已复制"
 */
async function copyToClipboard(text: string, label: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
    copiedLabel.value = `${label} 已复制`;
    setTimeout(() => {
      copiedLabel.value = "";
    }, 1500);
  } catch {
    // 降级方案：使用传统 execCommand
    const textarea = document.createElement("textarea");
    textarea.value = text;
    textarea.style.position = "fixed";
    textarea.style.opacity = "0";
    document.body.appendChild(textarea);
    textarea.select();
    document.execCommand("copy");
    document.body.removeChild(textarea);
    copiedLabel.value = `${label} 已复制`;
    setTimeout(() => {
      copiedLabel.value = "";
    }, 1500);
  }
}

/**
 * 根据用户名生成一致的颜色
 * 使用简单的哈希算法，确保同一用户名总是得到相同颜色
 *
 * @param username - 用户名
 * @returns 十六进制颜色字符串
 */
function getUserColor(username: string): string {
  // 预定义的颜色调色板（柔和、适合做头像背景）
  const colors = [
    "#3498db", // 蓝色
    "#e74c3c", // 红色
    "#2ecc71", // 绿色
    "#f39c12", // 橙色
    "#9b59b6", // 紫色
    "#1abc9c", // 青色
    "#e67e22", // 深橙
    "#27ae60", // 深绿
    "#8e44ad", // 深紫
    "#d35400", // 棕橙
    "#2980b9", // 深蓝
    "#16a085", // 深青
  ];

  // 简单哈希：将用户名的字符码求和取模
  let hash = 0;
  for (let i = 0; i < username.length; i++) {
    hash = (hash * 31 + username.charCodeAt(i)) & 0xffffffff;
  }
  // 处理负数情况
  if (hash < 0) hash = -hash;

  return colors[hash % colors.length];
}

/**
 * 获取用户名的首字母（大写）
 * 用于在头像圆形中显示
 *
 * @param username - 用户名
 * @returns 首字母大写字符串
 */
function getUserInitial(username: string): string {
  if (!username || username.length === 0) return "?";
  // 取第一个字符并转为大写
  return username.charAt(0).toUpperCase();
}
</script>

<template>
  <!-- 协作面板容器 -->
  <div class="collab-panel">
    <!-- 面板标题 -->
    <div class="collab-panel__header">
      <h3 class="collab-panel__title">协作</h3>
      <!-- 连接状态指示器 -->
      <span
        class="collab-panel__status"
        :class="'collab-panel__status--' + connectionStatus"
      >
        <span class="collab-panel__status-dot" />
        {{
          connectionStatus === "connected"
            ? "已连接"
            : connectionStatus === "connecting"
            ? "连接中..."
            : "未连接"
        }}
      </span>
    </div>

    <!-- 消息提示区域 -->
    <div v-if="errorMessage" class="collab-panel__message collab-panel__message--error">
      {{ errorMessage }}
    </div>
    <div
      v-if="successMessage"
      class="collab-panel__message collab-panel__message--success"
    >
      {{ successMessage }}
    </div>

    <!-- 未连接状态 -->
    <div v-if="connectionStatus === 'disconnected'" class="collab-panel__actions">
      <p class="collab-panel__hint">开始协作，与他人一起编辑文档</p>
      <button class="collab-panel__btn collab-panel__btn--primary" @click="showCreateDialog = true">
        创建房间
      </button>
      <button class="collab-panel__btn collab-panel__btn--secondary" @click="showJoinDialog = true">
        加入房间
      </button>
    </div>

    <!-- 已连接状态 -->
    <div v-if="connectionStatus === 'connected' && roomInfo" class="collab-panel__info">
      <!-- 房间信息 -->
      <div class="collab-panel__section">
        <h4 class="collab-panel__section-title">房间信息</h4>
        <div
          class="collab-panel__info-item collab-panel__info-item--copyable"
          title="点击复制房间 ID"
          @click="copyToClipboard(roomInfo.room_id, '房间 ID')"
        >
          <span class="collab-panel__info-label">房间 ID</span>
          <span class="collab-panel__info-value collab-panel__info-value--mono">
            {{ roomInfo.room_id }}
          </span>
        </div>
        <div
          class="collab-panel__info-item collab-panel__info-item--copyable"
          title="点击复制主机 IP"
          @click="copyToClipboard(roomInfo.host_ip, '主机 IP')"
        >
          <span class="collab-panel__info-label">主机 IP</span>
          <span class="collab-panel__info-value collab-panel__info-value--mono">
            {{ roomInfo.host_ip }}
          </span>
        </div>
        <div
          class="collab-panel__info-item collab-panel__info-item--copyable"
          title="点击复制端口"
          @click="copyToClipboard(String(roomInfo.port), '端口')"
        >
          <span class="collab-panel__info-label">端口</span>
          <span class="collab-panel__info-value">{{ roomInfo.port }}</span>
        </div>
        <div class="collab-panel__info-item">
          <span class="collab-panel__info-label">角色</span>
          <span class="collab-panel__info-value">
            {{ isHost ? "主机" : "客户端" }}
          </span>
        </div>
        <!-- 复制反馈提示 -->
        <span v-if="copiedLabel" class="collab-panel__copy-toast">{{ copiedLabel }}</span>
      </div>

      <!-- 在线用户列表 -->
      <div class="collab-panel__section">
        <h4 class="collab-panel__section-title">
          在线用户 ({{ roomInfo.peer_count }})
        </h4>
        <ul class="collab-panel__user-list">
          <!-- 用户卡片 -->
          <li
            v-for="peer in peers"
            :key="peer.peer_id"
            class="collab-panel__user-card"
            :class="{ 'collab-panel__user-card--self': peer.peer_id === localPeerId }"
          >
            <!-- 用户头像（圆形，首字母） -->
            <div
              class="collab-panel__user-avatar"
              :style="{ backgroundColor: getUserColor(peer.username) }"
            >
              {{ getUserInitial(peer.username) }}
            </div>
            <!-- 用户信息 -->
            <div class="collab-panel__user-info">
              <span class="collab-panel__user-name">
                {{ peer.username }}
                <span
                  v-if="peer.peer_id === localPeerId"
                  class="collab-panel__user-self-tag"
                >
                  我
                </span>
              </span>
              <!-- 角色标签 -->
              <span
                class="collab-panel__user-role"
                :class="{
                  'collab-panel__user-role--host': peer.is_host,
                  'collab-panel__user-role--member': !peer.is_host,
                }"
              >
                {{ peer.is_host ? "主机" : "成员" }}
              </span>
            </div>
            <!-- 在线状态指示 -->
            <span class="collab-panel__user-online" title="在线">
              <span class="collab-panel__user-online-dot" />
            </span>
          </li>
          <!-- 空状态 -->
          <li v-if="peers.length === 0" class="collab-panel__user-empty">
            暂无其他用户
          </li>
        </ul>
      </div>

      <!-- 共享文件列表 -->
      <div class="collab-panel__section">
        <h4 class="collab-panel__section-title">
          共享文件 ({{ sharedFiles.length }})
        </h4>
        <div v-if="sharedFiles.length > 0" class="collab-panel__file-list">
          <div
            v-for="file in sharedFiles"
            :key="file.path"
            class="collab-panel__file-item"
            :title="file.path"
            @click="emit('open-file', file.path)"
            @contextmenu="handleFileContextMenu($event, file)"
          >
            <span class="collab-panel__file-icon">
              <svg
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                width="14"
                height="14"
              >
                <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
                <polyline points="14 2 14 8 20 8" />
              </svg>
            </span>
            <span class="collab-panel__file-name">{{ file.title }}</span>
          </div>
        </div>
        <p v-else class="collab-panel__hint">暂无共享文件</p>
        <!-- 添加共享文件按钮（仅主机可见） -->
        <button
          v-if="isHost"
          class="collab-panel__btn collab-panel__btn--secondary"
          style="margin-top: 8px;"
          @click="showAddSharedDialog = true"
        >
          + 添加共享文件
        </button>
      </div>

      <!-- 离开按钮 -->
      <button
        class="collab-panel__btn collab-panel__btn--danger"
        @click="handleLeaveRoom"
      >
        离开房间
      </button>
    </div>

    <!-- 共享文件右键菜单 -->
    <Teleport to="body">
      <div
        v-if="fileMenuVisible"
        class="collab-panel__file-menu"
        :style="{
          left: fileMenuX + 'px',
          top: fileMenuY + 'px',
        }"
      >
        <div class="collab-panel__file-menu-item" @click="handleRemoveSharedFromMenu">
          移除共享
        </div>
      </div>
    </Teleport>

    <!-- ==================== 创建房间对话框 ==================== -->
    <Teleport to="body">
      <div v-if="showCreateDialog" class="modal-overlay" @mousedown.self="showCreateDialog = false">
        <div class="modal">
          <div class="modal__header">
            <h3 class="modal__title">创建协作房间</h3>
            <button
              class="modal__close"
              @click="showCreateDialog = false"
              title="关闭"
            >
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <line x1="18" y1="6" x2="6" y2="18" />
                <line x1="6" y1="6" x2="18" y2="18" />
              </svg>
            </button>
          </div>
          <div class="modal__body">
            <!-- 错误消息提示 -->
            <div v-if="createDialogError" class="modal__error">
              {{ createDialogError }}
            </div>
            <!-- 端口号 -->
            <label class="modal__label">
              端口号
              <input
                v-model.number="createForm.port"
                type="number"
                class="modal__input"
                placeholder="8080"
                min="1"
                max="65535"
              />
            </label>
            <!-- 用户名 -->
            <label class="modal__label">
              用户名
              <input
                v-model="createForm.username"
                type="text"
                class="modal__input"
                placeholder="请输入您的用户名"
              />
            </label>
            <!-- 选择共享标签页（多选） -->
            <div v-if="tabs && tabs.length > 0" class="modal__label">
              <span>选择要共享的标签页</span>
              <div class="modal__tab-list">
                <label
                  v-for="tab in tabs"
                  :key="tab.id"
                  class="modal__tab-checkbox"
                >
                  <input
                    type="checkbox"
                    :checked="selectedTabIds.has(tab.id)"
                    @change="toggleTabSelection(tab.id)"
                  />
                  <span>{{ tab.title }}</span>
                </label>
              </div>
            </div>
            <!-- 密码 -->
            <label class="modal__label">
              密码（可选）
              <input
                v-model="createForm.password"
                type="text"
                class="modal__input"
                placeholder="留空则不设密码"
              />
            </label>
          </div>
          <div class="modal__footer">
            <button
              class="modal__btn modal__btn--cancel"
              @click="showCreateDialog = false"
            >
              取消
            </button>
            <button
              class="modal__btn modal__btn--primary"
              :disabled="createLoading"
              @click="handleCreateRoom"
            >
              {{ createLoading ? "创建中..." : "创建房间" }}
            </button>
          </div>
        </div>
      </div>
    </Teleport>

    <!-- ==================== 加入房间对话框 ==================== -->
    <Teleport to="body">
      <div v-if="showJoinDialog" class="modal-overlay" @mousedown.self="showJoinDialog = false">
        <div class="modal">
          <div class="modal__header">
            <h3 class="modal__title">加入协作房间</h3>
            <button
              class="modal__close"
              @click="showJoinDialog = false"
              title="关闭"
            >
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <line x1="18" y1="6" x2="6" y2="18" />
                <line x1="6" y1="6" x2="18" y2="18" />
              </svg>
            </button>
          </div>
          <div class="modal__body">
            <!-- 错误消息提示 -->
            <div v-if="joinDialogError" class="modal__error">
              {{ joinDialogError }}
            </div>
            <!-- 房间 ID -->
            <label class="modal__label">
              房间 ID
              <input
                v-model="joinForm.roomId"
                type="text"
                class="modal__input"
                placeholder="请输入房间 ID"
              />
            </label>
            <!-- 主机 IP -->
            <label class="modal__label">
              主机 IP 地址
              <input
                v-model="joinForm.host"
                type="text"
                class="modal__input"
                placeholder="例如：192.168.1.100"
              />
            </label>
            <!-- 端口号 -->
            <label class="modal__label">
              端口号
              <input
                v-model.number="joinForm.port"
                type="number"
                class="modal__input"
                placeholder="8080"
                min="1"
                max="65535"
              />
            </label>
            <!-- 用户名 -->
            <label class="modal__label">
              用户名
              <input
                v-model="joinForm.username"
                type="text"
                class="modal__input"
                placeholder="请输入您的用户名"
              />
            </label>
            <!-- 密码 -->
            <label class="modal__label">
              密码（可选）
              <input
                v-model="joinForm.password"
                type="text"
                class="modal__input"
                placeholder="房间有密码时填写"
              />
            </label>
          </div>
          <div class="modal__footer">
            <button
              class="modal__btn modal__btn--cancel"
              @click="showJoinDialog = false"
            >
              取消
            </button>
            <button
              class="modal__btn modal__btn--primary"
              :disabled="joinLoading"
              @click="handleJoinRoom"
            >
              {{ joinLoading ? "加入中..." : "加入房间" }}
            </button>
          </div>
        </div>
      </div>
    </Teleport>

    <!-- ==================== 添加共享文件对话框 ==================== -->
    <Teleport to="body">
      <div v-if="showAddSharedDialog" class="modal-overlay" @mousedown.self="showAddSharedDialog = false">
        <div class="modal">
          <div class="modal__header">
            <h3 class="modal__title">添加共享文件</h3>
            <button
              class="modal__close"
              @click="showAddSharedDialog = false"
              title="关闭"
            >
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <line x1="18" y1="6" x2="6" y2="18" />
                <line x1="6" y1="6" x2="18" y2="18" />
              </svg>
            </button>
          </div>
          <div class="modal__body">
            <p class="collab-panel__hint" style="margin-bottom: 12px;">
              选择要添加到共享列表的标签页（仅显示未共享的标签页）
            </p>
            <div class="modal__tab-list">
              <label
                v-for="tab in unsharedTabs"
                :key="tab.id"
                class="modal__tab-checkbox"
              >
                <input
                  type="checkbox"
                  :value="tab.id"
                  v-model="addSharedSelectedIds"
                />
                <span>{{ tab.title }}</span>
              </label>
            </div>
            <p v-if="unsharedTabs.length === 0" class="collab-panel__hint">
              没有可添加的标签页
            </p>
          </div>
          <div class="modal__footer">
            <button
              class="modal__btn modal__btn--cancel"
              @click="showAddSharedDialog = false"
            >
              取消
            </button>
            <button
              class="modal__btn modal__btn--primary"
              :disabled="addSharedSelectedIds.length === 0"
              @click="handleAddSharedFiles"
            >
              添加选中文件
            </button>
          </div>
        </div>
      </div>
    </Teleport>
  </div>
</template>

<style scoped>
/* ==================== 协作面板容器 ==================== */

.collab-panel {
  /* 布局 */
  display: flex;
  flex-direction: column;

  /* 尺寸 */
  width: 260px;
  min-width: 260px;
  height: 100%;

  /* 样式 */
  background-color: var(--toolbar-bg-color);
  border-left: 1px solid var(--border-color);

  /* 过渡 */
  transition: background-color 0.3s ease, border-color 0.3s ease;
}

/* ==================== 面板标题 ==================== */

.collab-panel__header {
  /* 布局 */
  display: flex;
  align-items: center;
  justify-content: space-between;

  /* 间距 */
  padding: 12px 16px;

  /* 底部边框 */
  border-bottom: 1px solid var(--border-color);
}

.collab-panel__title {
  font-size: 14px;
  font-weight: 600;
  color: var(--heading-color);
  margin: 0;
}

/* ==================== 连接状态指示器 ==================== */

.collab-panel__status {
  /* 布局 */
  display: flex;
  align-items: center;
  gap: 6px;

  /* 字体 */
  font-size: 12px;
  font-weight: 500;
}

/* 状态指示点 */
.collab-panel__status-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  flex-shrink: 0;
}

/* 已连接状态：绿色 */
.collab-panel__status--connected {
  color: #2ecc71;
}

.collab-panel__status--connected .collab-panel__status-dot {
  background-color: #2ecc71;
  box-shadow: 0 0 4px rgba(46, 204, 113, 0.5);
}

/* 连接中状态：黄色 */
.collab-panel__status--connecting {
  color: #f39c12;
}

.collab-panel__status--connecting .collab-panel__status-dot {
  background-color: #f39c12;
  animation: pulse 1s ease-in-out infinite;
}

/* 未连接状态：灰色 */
.collab-panel__status--disconnected {
  color: #999999;
}

.collab-panel__status--disconnected .collab-panel__status-dot {
  background-color: #cccccc;
}

/* 脉冲动画 */
@keyframes pulse {
  0%,
  100% {
    opacity: 1;
  }
  50% {
    opacity: 0.4;
  }
}

/* ==================== 消息提示 ==================== */

.collab-panel__message {
  /* 间距 */
  margin: 8px 12px;
  padding: 8px 12px;

  /* 圆角 */
  border-radius: 6px;

  /* 字体 */
  font-size: 12px;
  line-height: 1.5;
}

/* 错误消息 */
.collab-panel__message--error {
  background-color: rgba(231, 76, 60, 0.1);
  color: #e74c3c;
  border: 1px solid rgba(231, 76, 60, 0.2);
}

/* 成功消息 */
.collab-panel__message--success {
  background-color: rgba(46, 204, 113, 0.1);
  color: #2ecc71;
  border: 1px solid rgba(46, 204, 113, 0.2);
}

/* ==================== 按钮区域（未连接状态） ==================== */

.collab-panel__actions {
  /* 布局 */
  display: flex;
  flex-direction: column;
  gap: 8px;

  /* 间距 */
  padding: 16px;
}

.collab-panel__hint {
  font-size: 12px;
  color: var(--text-color);
  opacity: 0.7;
  margin: 0 0 8px 0;
  line-height: 1.5;
}

/* ==================== 面板按钮 ==================== */

.collab-panel__btn {
  /* 布局 */
  display: flex;
  align-items: center;
  justify-content: center;

  /* 尺寸 */
  width: 100%;
  padding: 8px 16px;

  /* 样式 */
  border: none;
  border-radius: 6px;
  cursor: pointer;

  /* 字体 */
  font-size: 13px;
  font-weight: 500;
  line-height: 1.5;

  /* 过渡 */
  transition: all 0.2s ease;
}

.collab-panel__btn:active {
  transform: scale(0.97);
}

/* 主要按钮 */
.collab-panel__btn--primary {
  background-color: var(--button-active-text);
  color: #ffffff;
}

.collab-panel__btn--primary:hover {
  opacity: 0.9;
}

/* 次要按钮 */
.collab-panel__btn--secondary {
  background-color: var(--button-group-bg);
  color: var(--text-color);
}

.collab-panel__btn--secondary:hover {
  background-color: var(--button-hover-bg);
}

/* 危险按钮（离开） */
.collab-panel__btn--danger {
  background-color: rgba(231, 76, 60, 0.1);
  color: #e74c3c;
  border: 1px solid rgba(231, 76, 60, 0.2);
}

.collab-panel__btn--danger:hover {
  background-color: rgba(231, 76, 60, 0.2);
}

/* ==================== 已连接信息区域 ==================== */

.collab-panel__info {
  /* 布局 */
  display: flex;
  flex-direction: column;
  flex: 1;

  /* 间距 */
  padding: 12px 16px;
  gap: 16px;

  /* 溢出滚动 */
  overflow-y: auto;
}

/* ==================== 信息区块 ==================== */

.collab-panel__section {
  /* 布局 */
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.collab-panel__section-title {
  font-size: 12px;
  font-weight: 600;
  color: var(--heading-color);
  margin: 0;
  text-transform: uppercase;
  letter-spacing: 0.5px;
  opacity: 0.8;
}

/* ==================== 信息条目 ==================== */

.collab-panel__info-item {
  /* 布局 */
  display: flex;
  justify-content: space-between;
  align-items: center;

  /* 间距 */
  padding: 4px 0;
}

.collab-panel__info-label {
  font-size: 12px;
  color: var(--text-color);
  opacity: 0.7;
}

.collab-panel__info-value {
  font-size: 12px;
  color: var(--text-color);
  font-weight: 500;
}

/* 等宽字体值（用于 IP 和 ID） */
.collab-panel__info-value--mono {
  font-family: "Cascadia Code", "Fira Code", "Consolas", monospace;
  font-size: 11px;
  max-width: 140px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* 可复制的信息条目 */
.collab-panel__info-item--copyable {
  cursor: pointer;
  border-radius: 4px;
  padding: 4px 6px;
  margin: 0 -6px;
  transition: background-color 0.15s ease;
}

.collab-panel__info-item--copyable:hover {
  background-color: var(--button-hover-bg, rgba(128, 128, 128, 0.12));
}

.collab-panel__info-item--copyable:active {
  background-color: var(--button-active-bg, rgba(128, 128, 128, 0.2));
}

/* 复制反馈提示 */
.collab-panel__copy-toast {
  font-size: 11px;
  color: #2ecc71;
  font-weight: 500;
  animation: toastFadeIn 0.2s ease;
}

@keyframes toastFadeIn {
  from {
    opacity: 0;
    transform: translateY(-4px);
  }
  to {
    opacity: 1;
    transform: translateY(0);
  }
}

/* ==================== 用户列表 ==================== */

.collab-panel__user-list {
  list-style: none;
  padding: 0;
  margin: 0;

  /* 布局 */
  display: flex;
  flex-direction: column;
  gap: 4px;
}

/* 用户卡片 */
.collab-panel__user-card {
  /* 布局 */
  display: flex;
  align-items: center;
  gap: 8px;

  /* 间距 */
  padding: 6px 8px;

  /* 样式 */
  border-radius: 6px;
  background-color: var(--card-bg-color, rgba(128, 128, 128, 0.06));

  /* 过渡 */
  transition: background-color 0.2s ease;
}

.collab-panel__user-card:hover {
  background-color: var(--card-hover-bg-color, rgba(128, 128, 128, 0.12));
}

/* 自己的卡片高亮 */
.collab-panel__user-card--self {
  background-color: var(--card-self-bg-color, rgba(26, 115, 232, 0.08));
  border: 1px solid var(--card-self-border-color, rgba(26, 115, 232, 0.2));
}

/* 用户头像（圆形） */
.collab-panel__user-avatar {
  /* 尺寸 */
  width: 28px;
  height: 28px;
  min-width: 28px;

  /* 布局 */
  display: flex;
  align-items: center;
  justify-content: center;

  /* 样式 */
  border-radius: 50%;
  color: #ffffff;

  /* 字体 */
  font-size: 12px;
  font-weight: 600;
  line-height: 1;

  /* 阴影 */
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.15);

  /* 防止文本被选中 */
  user-select: none;
}

/* 用户信息区域 */
.collab-panel__user-info {
  /* 布局 */
  display: flex;
  flex-direction: column;
  gap: 2px;
  flex: 1;
  min-width: 0;
}

/* 用户名 */
.collab-panel__user-name {
  font-size: 12px;
  font-weight: 500;
  color: var(--text-color);
  line-height: 1.3;

  /* 溢出处理 */
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* "我"标签 */
.collab-panel__user-self-tag {
  /* 间距 */
  margin-left: 4px;

  /* 字体 */
  font-size: 10px;
  font-weight: 500;
  color: var(--button-active-text, #1a73e8);
  opacity: 0.8;
}

/* 角色标签 */
.collab-panel__user-role {
  /* 尺寸 */
  padding: 1px 6px;

  /* 样式 */
  border-radius: 3px;

  /* 字体 */
  font-size: 10px;
  font-weight: 500;
  line-height: 1.5;

  /* 布局 */
  display: inline-block;
  width: fit-content;
}

/* 主机角色标签 */
.collab-panel__user-role--host {
  background-color: rgba(46, 204, 113, 0.15);
  color: #27ae60;
}

/* 成员角色标签 */
.collab-panel__user-role--member {
  background-color: rgba(52, 152, 219, 0.15);
  color: #2980b9;
}

/* 在线状态指示 */
.collab-panel__user-online {
  /* 布局 */
  display: flex;
  align-items: center;
  justify-content: center;

  /* 尺寸 */
  width: 16px;
  height: 16px;
  flex-shrink: 0;
}

/* 在线状态指示点 */
.collab-panel__user-online-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background-color: #2ecc71;
  box-shadow: 0 0 4px rgba(46, 204, 113, 0.5);
}

/* ==================== Modal 对话框样式 ==================== */

/* 半透明遮罩层 */
.modal-overlay {
  /* 固定定位，覆盖整个窗口 */
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;

  /* 半透明背景 */
  background-color: rgba(0, 0, 0, 0.5);

  /* 居中显示 */
  display: flex;
  align-items: center;
  justify-content: center;

  /* 确保在最顶层 */
  z-index: 1000;

  /* 过渡动画 */
  animation: fadeIn 0.2s ease;
}

@keyframes fadeIn {
  from {
    opacity: 0;
  }
  to {
    opacity: 1;
  }
}

/* Modal 对话框 */
.modal {
  /* 尺寸 */
  width: 400px;
  max-width: 90vw;
  max-height: 80vh;

  /* 布局 */
  display: flex;
  flex-direction: column;

  /* 样式 */
  background-color: var(--bg-color);
  border-radius: 10px;
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.2);

  /* 过渡 */
  transition: background-color 0.3s ease;

  /* 动画 */
  animation: slideUp 0.2s ease;
}

@keyframes slideUp {
  from {
    transform: translateY(20px);
    opacity: 0;
  }
  to {
    transform: translateY(0);
    opacity: 1;
  }
}

/* Modal 标题栏 */
.modal__header {
  /* 布局 */
  display: flex;
  align-items: center;
  justify-content: space-between;

  /* 间距 */
  padding: 16px 20px;

  /* 底部边框 */
  border-bottom: 1px solid var(--border-color);
}

.modal__title {
  font-size: 16px;
  font-weight: 600;
  color: var(--heading-color);
  margin: 0;
}

/* Modal 关闭按钮 */
.modal__close {
  /* 布局 */
  display: flex;
  align-items: center;
  justify-content: center;

  /* 尺寸 */
  width: 28px;
  height: 28px;

  /* 样式 */
  border: none;
  border-radius: 6px;
  background: transparent;
  cursor: pointer;

  /* 颜色 */
  color: var(--text-color);

  /* 过渡 */
  transition: background-color 0.2s ease;
}

.modal__close:hover {
  background-color: var(--button-hover-bg);
}

.modal__close svg {
  width: 16px;
  height: 16px;
}

/* Modal 错误消息提示 */
.modal__error {
  /* 间距 */
  padding: 10px 12px;

  /* 样式 */
  background-color: rgba(231, 76, 60, 0.1);
  border: 1px solid rgba(231, 76, 60, 0.2);
  border-radius: 6px;

  /* 字体 */
  font-size: 13px;
  color: #e74c3c;
  line-height: 1.4;
}

/* Modal 内容区域 */
.modal__body {
  /* 布局 */
  display: flex;
  flex-direction: column;
  gap: 14px;

  /* 间距 */
  padding: 20px;

  /* 溢出滚动 */
  overflow-y: auto;
  flex: 1;
}

/* Modal 标签 */
.modal__label {
  /* 布局 */
  display: flex;
  flex-direction: column;
  gap: 6px;

  /* 字体 */
  font-size: 13px;
  font-weight: 500;
  color: var(--text-color);
}

/* Modal 输入框 */
.modal__input {
  /* 尺寸 */
  width: 100%;
  padding: 8px 12px;

  /* 样式 */
  border: 1px solid var(--border-color);
  border-radius: 6px;
  background-color: var(--editor-bg-color);
  color: var(--editor-text-color);

  /* 字体 */
  font-size: 13px;
  font-family: inherit;
  line-height: 1.5;

  /* 过渡 */
  transition: border-color 0.2s ease, background-color 0.3s ease;
}

.modal__input:focus {
  outline: none;
  border-color: var(--button-active-text);
  box-shadow: 0 0 0 2px rgba(26, 115, 232, 0.15);
}

.modal__input::placeholder {
  color: var(--editor-placeholder-color);
  opacity: 0.6;
}

/* 数字输入框去掉上下箭头 */
.modal__input[type="number"]::-webkit-inner-spin-button,
.modal__input[type="number"]::-webkit-outer-spin-button {
  -webkit-appearance: none;
  margin: 0;
}

/* Modal 底部按钮栏 */
.modal__footer {
  /* 布局 */
  display: flex;
  justify-content: flex-end;
  gap: 8px;

  /* 间距 */
  padding: 12px 20px;

  /* 顶部边框 */
  border-top: 1px solid var(--border-color);
}

/* Modal 按钮 */
.modal__btn {
  /* 布局 */
  display: flex;
  align-items: center;
  justify-content: center;

  /* 尺寸 */
  padding: 8px 20px;

  /* 样式 */
  border: none;
  border-radius: 6px;
  cursor: pointer;

  /* 字体 */
  font-size: 13px;
  font-weight: 500;
  line-height: 1.5;

  /* 过渡 */
  transition: all 0.2s ease;
}

.modal__btn:active {
  transform: scale(0.96);
}

.modal__btn:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

/* 取消按钮 */
.modal__btn--cancel {
  background-color: var(--button-group-bg);
  color: var(--text-color);
}

.modal__btn--cancel:hover {
  background-color: var(--button-hover-bg);
}

/* 确认按钮 */
.modal__btn--primary {
  background-color: var(--button-active-text);
  color: #ffffff;
}

.modal__btn--primary:hover {
  opacity: 0.9;
}

/* ==================== 共享文件列表样式 ==================== */

.collab-panel__file-list {
  /* 撑满剩余空间 */
  flex: 1;

  /* 支持内容溢出时滚动 */
  overflow-y: auto;

  /* 上下内边距 */
  padding: 4px 0;

  /* 布局 */
  display: flex;
  flex-direction: column;
  gap: 0;
}

/* 滚动条样式 */
.collab-panel__file-list::-webkit-scrollbar {
  width: 6px;
}

.collab-panel__file-list::-webkit-scrollbar-track {
  background: transparent;
}

.collab-panel__file-list::-webkit-scrollbar-thumb {
  background-color: var(--border-color);
  border-radius: 3px;
}

/* 文件条目 */
.collab-panel__file-item {
  /* 条目布局 */
  display: flex;
  align-items: center;
  padding: 4px 8px;
  cursor: pointer;

  /* 文字样式 */
  font-size: 13px;
  color: var(--text-color);
  line-height: 1.6;

  /* 过渡动画 */
  transition: background-color 0.15s ease, color 0.15s ease;
}

/* 条目 hover 状态 */
.collab-panel__file-item:hover {
  background-color: var(--button-hover-bg);
}

/* 文件图标 */
.collab-panel__file-icon {
  /* 图标容器 */
  display: flex;
  align-items: center;
  justify-content: center;

  /* 固定尺寸，保证对齐 */
  width: 16px;
  height: 16px;

  /* 与文本之间的间距 */
  margin-right: 6px;

  /* 防止被 flex 压缩 */
  flex-shrink: 0;

  /* 颜色 */
  color: var(--text-color);
  opacity: 0.5;
}

/* 文件名 */
.collab-panel__file-name {
  /* 超长文本省略 */
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;

  /* 撑满剩余空间 */
  flex: 1;
}

/* ==================== 共享文件右键菜单 ==================== */

.collab-panel__file-menu {
  /* 固定定位：跟随鼠标点击位置 */
  position: fixed;

  /* 尺寸 */
  min-width: 120px;

  /* 样式 */
  background-color: var(--bg-color);
  border: 1px solid var(--border-color);
  border-radius: 6px;
  box-shadow: 0 4px 16px rgba(0, 0, 0, 0.15);

  /* 层级 */
  z-index: 300;

  /* 间距 */
  padding: 4px 0;
}

/* 菜单项 */
.collab-panel__file-menu-item {
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

.collab-panel__file-menu-item:hover {
  background-color: var(--button-hover-bg);
}
</style>