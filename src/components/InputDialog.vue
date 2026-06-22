<script setup lang="ts">
import { ref } from "vue";

const visible = ref(false);
const title = ref("");
const message = ref("");
const placeholder = ref("");
const defaultValue = ref("");
const error = ref("");

let resolvePromise: ((value: string | null) => void) | null = null;

function show(options: {
  title: string;
  message?: string;
  placeholder?: string;
  defaultValue?: string;
}): Promise<string | null> {
  title.value = options.title;
  message.value = options.message ?? "";
  placeholder.value = options.placeholder ?? "";
  defaultValue.value = options.defaultValue ?? "";
  error.value = "";
  visible.value = true;

  return new Promise<string | null>((resolve) => {
    resolvePromise = resolve;
  });
}

function hide(): void {
  visible.value = false;
  resolvePromise = null;
}

function onConfirm(): void {
  const value = (document.querySelector(".input-dialog__input") as HTMLInputElement)?.value.trim();
  if (!value) {
    error.value = "内容不能为空";
    return;
  }
  if (resolvePromise) {
    resolvePromise(value);
  }
  hide();
}

function onCancel(): void {
  if (resolvePromise) {
    resolvePromise(null);
  }
  hide();
}

function onKeydown(event: KeyboardEvent): void {
  if (event.key === "Enter") {
    onConfirm();
  } else if (event.key === "Escape") {
    onCancel();
  }
}

defineExpose({ show });
</script>

<template>
  <Teleport to="body">
    <div v-if="visible" class="input-dialog-overlay" @click.self="onCancel">
      <div class="input-dialog" role="dialog" aria-modal="true">
        <h3 class="input-dialog__title">{{ title }}</h3>
        <p v-if="message" class="input-dialog__message">{{ message }}</p>
        <input
          class="input-dialog__input"
          :placeholder="placeholder"
          :value="defaultValue"
          @keydown="onKeydown"
          autofocus
        />
        <p v-if="error" class="input-dialog__error">{{ error }}</p>
        <div class="input-dialog__buttons">
          <button class="input-dialog__btn input-dialog__btn--cancel" @click="onCancel">
            取消
          </button>
          <button class="input-dialog__btn input-dialog__btn--confirm" @click="onConfirm">
            确认
          </button>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.input-dialog-overlay {
  position: fixed;
  inset: 0;
  z-index: 10000;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(0, 0, 0, 0.5);
  animation: input-dialog-overlay-fade-in 0.15s ease-out;
}

@keyframes input-dialog-overlay-fade-in {
  from {
    opacity: 0;
  }
  to {
    opacity: 1;
  }
}

.input-dialog {
  width: 380px;
  max-width: 90vw;
  display: flex;
  flex-direction: column;
  padding: 24px 28px;
  border-radius: 12px;
  background: var(--color-bg-primary, #ffffff);
  box-shadow: 0 20px 60px rgba(0, 0, 0, 0.3);
  animation: input-dialog-scale-in 0.15s ease-out;
}

@keyframes input-dialog-scale-in {
  from {
    transform: scale(0.95);
    opacity: 0;
  }
  to {
    transform: scale(1);
    opacity: 1;
  }
}

.input-dialog__title {
  margin: 0 0 8px;
  font-size: 17px;
  font-weight: 600;
  color: var(--color-text-primary, #1a1a1a);
}

.input-dialog__message {
  margin: 0 0 16px;
  font-size: 13px;
  color: var(--color-text-tertiary, #999999);
}

.input-dialog__input {
  width: 100%;
  height: 36px;
  padding: 0 12px;
  border: 1px solid var(--color-border, #d9d9d9);
  border-radius: 6px;
  font-size: 14px;
  color: var(--color-text-primary, #1a1a1a);
  background: var(--color-bg-primary, #ffffff);
  outline: none;
  transition: border-color 0.2s ease, box-shadow 0.2s ease;
  box-sizing: border-box;
}

.input-dialog__input:focus {
  border-color: #409eff;
  box-shadow: 0 0 0 2px rgba(64, 158, 255, 0.2);
}

.input-dialog__input::placeholder {
  color: var(--color-text-tertiary, #999999);
}

.input-dialog__error {
  margin: 6px 0 0;
  font-size: 12px;
  color: #e74c3c;
}

.input-dialog__buttons {
  display: flex;
  justify-content: flex-end;
  gap: 10px;
  margin-top: 20px;
}

.input-dialog__btn {
  padding: 8px 18px;
  border: 1px solid transparent;
  border-radius: 6px;
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.15s ease;
  outline: none;
}

.input-dialog__btn--cancel {
  background: var(--color-bg-tertiary, #f0f0f0);
  color: var(--color-text-secondary, #666666);
  border-color: var(--color-border, #d9d9d9);
}

.input-dialog__btn--cancel:hover {
  background: var(--color-bg-hover, #e0e0e0);
}

.input-dialog__btn--confirm {
  background: #409eff;
  color: #ffffff;
  border-color: #409eff;
}

.input-dialog__btn--confirm:hover {
  background: #337ecc;
  border-color: #337ecc;
}
</style>