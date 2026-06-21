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

import { ref, reactive, onUnmounted } from "vue";
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
}

// ==================== Emits 定义 ====================

const emit = defineEmits<{
  /** 连接状态变更 */
  "connection-change": [connected: boolean];
  /** 协作者列表更新 */
  "peers-update": [peers: PeerInfo[]];
  /** 远程文档内容更新 */
  "document-update": [document: string];
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

// ==================== 状态轮询 ====================

/** 状态轮询定时器 ID */
let statusPollTimer: ReturnType<typeof setInterval> | null = null;

// ==================== 方法 ====================

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
 * 调用后端 IPC 命令创建房间，成功后更新连接状态并开始轮询
 */
async function handleCreateRoom(): Promise<void> {
  // 表单验证
  if (!createForm.username.trim()) {
    createDialogError.value = "请输入用户名";
    setTimeout(() => {
      createDialogError.value = "";
    }, 5000);
    return;
  }

  createLoading.value = true;
  createDialogError.value = "";

  try {
    // 调用后端 IPC 创建房间
    const result = await invoke<RoomInfo>("create_collab_room", {
      port: createForm.port,
      password: createForm.password,
      username: createForm.username.trim(),
      document: "", // 初始文档为空
    });

    // 更新房间信息
    roomInfo.value = result;
    isHost.value = true;
    localUsername.value = createForm.username.trim();
    connectionStatus.value = "connected";

    // 关闭对话框
    showCreateDialog.value = false;

    showSuccess("房间创建成功！");

    // 通知父组件连接状态变更
    emit("connection-change", true);

    // 开始轮询状态
    startStatusPolling();
  } catch (error) {
    createDialogError.value = `创建房间失败: ${error}`;
  } finally {
    createLoading.value = false;
  }
}

/**
 * 加入协作房间
 * 调用后端 IPC 命令加入房间，成功后更新连接状态并开始轮询
 */
async function handleJoinRoom(): Promise<void> {
  // 表单验证
  if (!joinForm.host.trim()) {
    joinDialogError.value = "请输入主机 IP 地址";
    setTimeout(() => {
      joinDialogError.value = "";
    }, 5000);
    return;
  }
  if (!joinForm.roomId.trim()) {
    joinDialogError.value = "请输入房间 ID";
    setTimeout(() => {
      joinDialogError.value = "";
    }, 5000);
    return;
  }
  if (!joinForm.username.trim()) {
    joinDialogError.value = "请输入用户名";
    setTimeout(() => {
      joinDialogError.value = "";
    }, 5000);
    return;
  }

  joinLoading.value = true;
  joinDialogError.value = "";

  try {
    // 调用后端 IPC 加入房间
    const result = await invoke<RoomInfo>("join_collab_room", {
      host: joinForm.host.trim(),
      port: joinForm.port,
      roomId: joinForm.roomId.trim(),
      password: joinForm.password,
      username: joinForm.username.trim(),
    });

    // 更新房间信息
    roomInfo.value = result;
    isHost.value = false;
    localUsername.value = joinForm.username.trim();
    connectionStatus.value = "connected";

    // 关闭对话框
    showJoinDialog.value = false;

    showSuccess("成功加入房间！");

    // 通知父组件连接状态变更
    emit("connection-change", true);

    // 开始轮询状态
    startStatusPolling();
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

  // 停止轮询
  stopStatusPolling();

  // 通知父组件断开连接
  emit("connection-change", false);
  emit("peers-update", []);
}

/**
 * 开始定期轮询协作状态
 * 每 500ms 调用 get_collab_status 获取最新状态
 */
function startStatusPolling(): void {
  // 清除已有定时器
  stopStatusPolling();

  statusPollTimer = setInterval(async () => {
    try {
      const statusJson = await invoke<string>("get_collab_status");
      const status: CollabStatus = JSON.parse(statusJson);

      // 检测连接是否已断开（如主机关闭了房间）
      if (!status.connected) {
        console.warn("[协作] 检测到房间已断开，自动退出");
        errorMessage.value = "主机已关闭房间，您已被强制退出";
        // 8 秒后自动清除提示信息
        setTimeout(() => {
          errorMessage.value = "";
        }, 8000);
        handleLeaveRoom();
        return;
      }

      // 更新协作者列表
      peers.value = status.peers || [];
      emit("peers-update", peers.value);

      // 更新本地对等方 ID（用于精确匹配"我"）
      localPeerId.value = status.local_peer_id || "";

      // 更新房间信息
      if (roomInfo.value) {
        roomInfo.value.peer_count = status.peer_count;
      }

      // 如果远程文档有更新，通知父组件
      if (status.document !== undefined) {
        emit("document-update", status.document);
      }
    } catch (error) {
      console.error("获取协作状态失败:", error);
    }
  }, 500);
}

/**
 * 停止状态轮询
 */
function stopStatusPolling(): void {
  if (statusPollTimer !== null) {
    clearInterval(statusPollTimer);
    statusPollTimer = null;
  }
}

// ==================== 生命周期 ====================

/** 组件卸载时清理定时器 */
onUnmounted(() => {
  stopStatusPolling();
});

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

      <!-- 离开按钮 -->
      <button
        class="collab-panel__btn collab-panel__btn--danger"
        @click="handleLeaveRoom"
      >
        离开房间
      </button>
    </div>

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
</style>