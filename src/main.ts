import { createApp } from "vue";
import App from "./App.vue";
import {createPinia} from "pinia";
import router from "./router";

const app = createApp(App);
app.use(createPinia()); // <-- 初始化 Pinia
app.use(router);       // <-- 启用路由
app.mount('#app');
