import { createApp } from "vue";
import App from "./App.vue";

// 导入主题 CSS 变量，确保全局主题样式生效
import "./styles/themes.css";

createApp(App).mount("#app");