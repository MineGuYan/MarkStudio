<script setup lang="ts">
import { ref, watch, onMounted, onUnmounted, nextTick } from "vue";
import katex from "katex";
import "katex/dist/katex.min.css";
import mermaid from "mermaid";
import hljs from "highlight.js";
import "highlight.js/styles/github-dark.css";

const props = defineProps<{
  html: string;
}>();

const previewRef = ref<HTMLElement | null>(null);

// 存储 IntersectionObserver 引用，用于组件卸载时断开连接，避免内存泄漏
const lazyLoadObserver = ref<IntersectionObserver | null>(null);

mermaid.initialize({
  startOnLoad: false,
  theme: "dark",
  themeVariables: {
    primaryColor: "#58a6ff",
    primaryTextColor: "#c9d1d9",
    primaryBorderColor: "#30363d",
    lineColor: "#8b949e",
    secondaryColor: "#21262d",
    tertiaryColor: "#161b22",
  },
});

function renderMath() {
  if (!previewRef.value) return;

  const blocks = previewRef.value.querySelectorAll(".math-block");
  blocks.forEach((block) => {
    const text = block.textContent || "";
    try {
      katex.render(text, block as HTMLElement, {
        displayMode: true,
        throwOnError: false,
      });
    } catch {
      block.textContent = text;
    }
  });

  const inline = previewRef.value.querySelectorAll(".math-inline");
  inline.forEach((span) => {
    const text = span.textContent || "";
    try {
      katex.render(text, span as HTMLElement, {
        displayMode: false,
        throwOnError: false,
      });
    } catch {
      span.textContent = text;
    }
  });
}

async function renderMermaid() {
  if (!previewRef.value) return;

  const diagrams = previewRef.value.querySelectorAll(".mermaid");
  for (const diagram of diagrams) {
    // 跳过已渲染过的图表，避免重复渲染和 ID 冲突
    if (diagram.classList.contains("mermaid-rendered")) continue;

    const code = diagram.textContent || "";
    if (!code.trim()) continue;
    
    try {
      // 生成唯一 ID，避免重复渲染时的冲突
      const uniqueId = `mermaid-${Date.now()}-${Math.random().toString(36).slice(2, 9)}`;
      
      // 使用 mermaid.render 获取 SVG 和绑定函数
      const result = await mermaid.render(uniqueId, code);
      
      if (result && result.svg && result.svg.trim()) {
        // 先设置 SVG 内容
        diagram.innerHTML = result.svg;
        
        // 等待 DOM 更新完成后再绑定交互函数
        await nextTick();
        
        // 绑定交互功能（如点击、hover等）
        if (result.bindFunctions && diagram instanceof HTMLElement) {
          result.bindFunctions(diagram);
        }
      } else {
        diagram.textContent = "Mermaid 渲染结果为空";
      }
      // 标记为已渲染，避免下次重复处理
      diagram.classList.add("mermaid-rendered");
    } catch (err) {
      console.error("Mermaid 渲染错误:", err);
      const errorHtml = `<div style="color: #f85149; padding: 8px; border: 1px solid #f85149; border-radius: 4px;">
        <strong>Mermaid 渲染错误:</strong><br>
        ${err instanceof Error ? err.message : String(err)}
      </div>`;
      diagram.innerHTML = errorHtml;
      // 即使渲染失败也标记为已处理，避免反复报错
      diagram.classList.add("mermaid-rendered");
    }
  }
}

function renderCodeHighlight() {
  if (!previewRef.value) return;

  const codes = previewRef.value.querySelectorAll("pre code");
  codes.forEach((code) => {
    const parent = code.parentElement;
    if (!parent) return;

    // 跳过 Mermaid 代码块（由 Mermaid 渲染器处理）
    if (parent.classList.contains("mermaid")) return;

    const language = code.className.replace("language-", "") || "plaintext";
    const htmlCode = code as HTMLElement;
    
    if (hljs.getLanguage(language)) {
      hljs.highlightElement(htmlCode);
    } else {
      const result = hljs.highlight(code.textContent || "", { language: "plaintext" });
      code.innerHTML = result.value;
    }

    parent.classList.add("hljs");
    
    // 为代码块添加复制按钮
    addCopyButton(parent as HTMLElement);
  });
}

/**
 * 为代码块添加复制按钮
 * @param pre 代码块 pre 元素
 */
function addCopyButton(pre: HTMLElement): void {
  if (pre.querySelector(".code-copy-btn")) return;

  const button = document.createElement("button");
  button.className = "code-copy-btn";
  button.title = "复制代码";
  button.setAttribute("aria-label", "复制代码");
  
  // 使用 SVG 图标
  button.innerHTML = `
    <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
      <rect x="9" y="9" width="13" height="13" rx="2" ry="2"></rect>
      <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"></path>
    </svg>
  `;

  button.addEventListener("click", async () => {
    const code = pre.querySelector("code");
    if (!code) return;
    const text = code.textContent || "";
    try {
      await navigator.clipboard.writeText(text);
      button.classList.add("copied");
      button.innerHTML = `
        <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <polyline points="20 6 9 17 4 12"></polyline>
        </svg>
      `;
      setTimeout(() => {
        button.classList.remove("copied");
        button.innerHTML = `
          <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <rect x="9" y="9" width="13" height="13" rx="2" ry="2"></rect>
            <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"></path>
          </svg>
        `;
      }, 2000);
    } catch (err) {
      console.error("复制失败:", err);
    }
  });

  pre.appendChild(button);
}

/**
 * 为图片添加懒加载支持
 * 使用 IntersectionObserver 监听图片是否进入视口
 */
function setupLazyLoad(): void {
  if (!previewRef.value) return;

  // 如果之前已创建过 observer，先断开连接，避免重复观察导致内存泄漏
  if (lazyLoadObserver.value) {
    lazyLoadObserver.value.disconnect();
    lazyLoadObserver.value = null;
  }

  const images = previewRef.value.querySelectorAll("img");

  // 标记原始 src 为 data-src
  images.forEach((img) => {
    if (img.dataset.src) return;
    const src = img.getAttribute("src");
    if (src && !src.startsWith("data:")) {
      img.dataset.src = src;
      img.removeAttribute("src");
      img.classList.add("lazy-image");
    }
  });

  if (images.length === 0) return;

  // 创建 IntersectionObserver 并存储引用
  lazyLoadObserver.value = new IntersectionObserver(
    (entries, obs) => {
      entries.forEach((entry) => {
        if (entry.isIntersecting) {
          const img = entry.target as HTMLImageElement;
          const src = img.dataset.src;
          if (src) {
            img.src = src;
            img.classList.add("loaded");
            img.removeAttribute("data-src");
          }
          obs.unobserve(img);
        }
      });
    },
    { rootMargin: "100px", threshold: 0.01 }
  );

  images.forEach((img) => {
    if (img.classList.contains("lazy-image")) {
      lazyLoadObserver.value!.observe(img);
    }
  });
}

async function processHtml() {
  await nextTick();
  renderCodeHighlight();
  renderMath();
  setupLazyLoad();
  await renderMermaid();
}

watch(
  () => props.html,
  () => {
    processHtml();
  },
  { immediate: false }
);

onMounted(() => {
  processHtml();
});

onUnmounted(() => {
  // 组件卸载时断开 IntersectionObserver，释放内存，避免内存泄漏
  if (lazyLoadObserver.value) {
    lazyLoadObserver.value.disconnect();
    lazyLoadObserver.value = null;
  }
});
</script>

<template>
  <div class="preview-container">
    <div ref="previewRef" class="preview-content markdown-body" v-html="html" />
  </div>
</template>

<style scoped>
.preview-container {
  display: flex;
  flex-direction: column;
  height: 100%;
  width: 100%;
  overflow-y: auto;
  background-color: var(--preview-bg-color);
  color: var(--text-color);
  transition: background-color 0.3s ease, color 0.3s ease;
}

.preview-content {
  padding: 24px 32px;
  max-width: 860px;
  margin: 0 auto;
  width: 100%;
}

.preview-container::-webkit-scrollbar {
  width: 8px;
}

.preview-container::-webkit-scrollbar-track {
  background: var(--preview-bg-color);
}

.preview-container::-webkit-scrollbar-thumb {
  background-color: var(--border-color);
  border-radius: 4px;
}

.markdown-body :deep(h1) {
  font-size: 2em;
  font-weight: 700;
  margin: 0.67em 0 0.5em;
  padding-bottom: 0.3em;
  border-bottom: 1px solid var(--border-color);
  color: var(--heading-color);
}

.markdown-body :deep(h2) {
  font-size: 1.5em;
  font-weight: 600;
  margin: 0.83em 0 0.5em;
  padding-bottom: 0.25em;
  border-bottom: 1px solid var(--border-color);
  color: var(--heading-color);
}

.markdown-body :deep(h3) {
  font-size: 1.25em;
  font-weight: 600;
  margin: 1em 0 0.5em;
  color: var(--heading-color);
}

.markdown-body :deep(h4) {
  font-size: 1.1em;
  font-weight: 600;
  margin: 1em 0 0.5em;
  color: var(--heading-color);
}

.markdown-body :deep(h5),
.markdown-body :deep(h6) {
  font-size: 1em;
  font-weight: 600;
  margin: 1em 0 0.5em;
  color: var(--heading-color);
}

.markdown-body :deep(p) {
  margin: 0.8em 0;
  line-height: 1.75;
}

.markdown-body :deep(a) {
  color: var(--link-color);
  text-decoration: none;
  border-bottom: 1px solid transparent;
  transition: border-color 0.2s ease;
}

.markdown-body :deep(a:hover) {
  border-bottom-color: var(--link-color);
}

.markdown-body :deep(pre) {
  position: relative;
  background-color: var(--code-bg-color);
  border: 1px solid var(--border-color);
  border-radius: 8px;
  padding: 16px 20px;
  /* 当代码过长时显示水平滚动条 */
  overflow-x: auto;
  margin: 1em 0;
  font-family: "Cascadia Code", "Fira Code", "JetBrains Mono", "Consolas",
    "Monaco", monospace;
  font-size: 13px;
  line-height: 1.6;
}

.markdown-body :deep(pre)::-webkit-scrollbar {
  height: 8px;
}

.markdown-body :deep(pre)::-webkit-scrollbar-track {
  background: var(--code-bg-color);
  border-radius: 4px;
}

.markdown-body :deep(pre)::-webkit-scrollbar-thumb {
  background-color: var(--border-color);
  border-radius: 4px;
}

/* 代码块复制按钮 */
.markdown-body :deep(.code-copy-btn) {
  position: absolute;
  top: 8px;
  right: 8px;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  padding: 0;
  background-color: var(--preview-bg-color);
  color: var(--text-color);
  border: 1px solid var(--border-color);
  border-radius: 4px;
  cursor: pointer;
  opacity: 0;
  transition: opacity 0.2s ease, background-color 0.2s ease;
}

.markdown-body :deep(pre:hover .code-copy-btn) {
  opacity: 1;
}

.markdown-body :deep(.code-copy-btn:hover) {
  background-color: var(--link-color);
  color: #ffffff;
  border-color: var(--link-color);
}

.markdown-body :deep(.code-copy-btn.copied) {
  opacity: 1;
  background-color: #28a745;
  color: #ffffff;
  border-color: #28a745;
}

/* 图片懒加载样式 */
.markdown-body :deep(.lazy-image) {
  opacity: 0;
  transition: opacity 0.3s ease;
  min-height: 50px;
  background-color: var(--code-bg-color);
}

.markdown-body :deep(.lazy-image.loaded) {
  opacity: 1;
}

.markdown-body :deep(pre code) {
  background: none;
  padding: 0;
  border-radius: 0;
  font-size: inherit;
}

.markdown-body :deep(code) {
  background-color: var(--code-bg-color);
  padding: 2px 6px;
  border-radius: 4px;
  font-family: "Cascadia Code", "Fira Code", "JetBrains Mono", "Consolas",
    "Monaco", monospace;
  font-size: 0.9em;
  color: var(--code-text-color);
}

.markdown-body :deep(blockquote) {
  margin: 1em 0;
  padding: 8px 16px;
  border-left: 4px solid var(--link-color);
  background-color: var(--blockquote-bg-color);
  border-radius: 0 6px 6px 0;
  color: var(--text-color);
}

.markdown-body :deep(blockquote p) {
  margin: 0.4em 0;
}

.markdown-body :deep(ul),
.markdown-body :deep(ol) {
  padding-left: 1.8em;
  margin: 0.8em 0;
}

.markdown-body :deep(li) {
  margin: 0.3em 0;
  line-height: 1.7;
}

/* 表格滚动容器 */
.markdown-body :deep(.table-wrapper) {
  margin: 1em 0;
  overflow-x: auto;
  border-radius: 4px;
}

.markdown-body :deep(.table-wrapper)::-webkit-scrollbar {
  height: 8px;
}

.markdown-body :deep(.table-wrapper)::-webkit-scrollbar-track {
  background: var(--code-bg-color);
  border-radius: 4px;
}

.markdown-body :deep(.table-wrapper)::-webkit-scrollbar-thumb {
  background-color: var(--border-color);
  border-radius: 4px;
}

.markdown-body :deep(table) {
  border-collapse: collapse;
  width: 100%;
  /* 表格本身不设置边框和 display:block，保持正常表格布局 */
}

.markdown-body :deep(th),
.markdown-body :deep(td) {
  border: 1px solid var(--border-color);
  padding: 8px 12px;
  text-align: left;
  /* 不强制 white-space: nowrap，让表格内容自动换行 */
}

.markdown-body :deep(th) {
  background-color: var(--code-bg-color);
  font-weight: 600;
}

.markdown-body :deep(tr:nth-child(even)) {
  background-color: var(--table-stripe-color);
}

.markdown-body :deep(hr) {
  border: none;
  border-top: 1px solid var(--border-color);
  margin: 1.5em 0;
}

.markdown-body :deep(img) {
  max-width: 100%;
  border-radius: 6px;
  margin: 1em 0;
}

.markdown-body :deep(strong) {
  font-weight: 600;
  color: var(--heading-color);
}

.markdown-body :deep(em) {
  font-style: italic;
}

.markdown-body :deep(.math-block) {
  margin: 1em 0;
  /* 当公式超宽时显示滚动条 */
  overflow-x: auto;
  padding: 0.5em 0;
}

.markdown-body :deep(.math-block)::-webkit-scrollbar {
  height: 6px;
}

.markdown-body :deep(.math-block)::-webkit-scrollbar-track {
  background: var(--code-bg-color);
  border-radius: 3px;
}

.markdown-body :deep(.math-block)::-webkit-scrollbar-thumb {
  background-color: var(--border-color);
  border-radius: 3px;
}

.markdown-body :deep(.math-inline) {
  font-size: 0.9em;
}

.markdown-body :deep(.mermaid) {
  margin: 1em 0;
  display: flex;
  justify-content: center;
}

.markdown-body :deep(.mermaid svg) {
  max-width: 100%;
  height: auto;
}

.markdown-body :deep(.footnotes) {
  margin-top: 2em;
  padding-top: 1em;
  border-top: 1px solid var(--border-color);
  font-size: 0.85em;
  color: var(--text-color);
  opacity: 0.8;
}

.markdown-body :deep(.footnote-reference) {
  font-size: 0.7em;
  vertical-align: super;
}

.markdown-body :deep(.footnote-definition) {
  margin: 0.5em 0;
  padding-left: 1em;
  border-left: 2px solid var(--border-color);
}

.markdown-body :deep(.footnote-definition-label) {
  margin-right: 0.4em;
  font-size: 0.7em;
  vertical-align: super;
  color: var(--link-color);
}

.markdown-body :deep(.toc-container) {
  background-color: var(--code-bg-color);
  border: 1px solid var(--border-color);
  border-radius: 8px;
  padding: 16px 20px;
  margin: 1em 0;
}

.markdown-body :deep(.toc-container ul) {
  padding-left: 1.5em;
  margin: 0.5em 0;
}

.markdown-body :deep(.toc-container li) {
  margin: 0.3em 0;
}

.markdown-body :deep(.toc-container a) {
  color: var(--link-color);
  text-decoration: none;
  transition: opacity 0.2s ease;
}

.markdown-body :deep(.toc-container a:hover) {
  opacity: 0.7;
}
</style>
