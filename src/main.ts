import {createApp} from "vue";
import {createPinia} from "pinia";
import Root from "./Root.vue";
import '/src/assets/style.css';
import router from "./router";

createApp(Root)
    .use(createPinia())
    .use(router)
    .mount("#app");
