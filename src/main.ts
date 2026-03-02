import {createApp} from "vue";
import {createPinia} from "pinia";
import Root from "./Root.vue";
import '/src/assets/style.css';
import router from "./router";
import clickOutsideDirective from './directives/vClickOutside.ts';

import '@quasar/extras/material-icons/material-icons.css'
import 'quasar/src/css/index.sass'
import {BottomSheet, Dialog, Notify, Quasar} from 'quasar'

const app = createApp(Root);
app.use(createPinia());
app.use(router);
app.use(Quasar, {
    plugins: {
        Notify,
        Dialog,
        BottomSheet
    },
});

app.directive('click-outside', clickOutsideDirective);

app.mount("#app");
