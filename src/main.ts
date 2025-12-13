import {createApp} from "vue";
import {createPinia} from "pinia";
import Root from "./Root.vue";
import '/src/assets/style.css';
import router from "./router";
import clickOutsideDirective from './directives/vClickOutside.ts';

const app = createApp(Root);
app.use(createPinia());
app.use(router);

app.directive('click-outside', clickOutsideDirective)

app.mount("#app");
