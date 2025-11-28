import {createApp} from "vue";
import {createPinia} from "pinia";
import Unlock from "./views/Unlock.vue";
import '/src/assets/style.css';

createApp(Unlock)
    .use(createPinia())
    .mount("#app");
