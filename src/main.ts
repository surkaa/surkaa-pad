import {createApp} from "vue";
import {createPinia} from "pinia";
import Root from "./Root.vue";
import '/src/assets/style.css';
import router from "./router";
import clickOutsideDirective from './directives/vClickOutside.ts';

import '@quasar/extras/material-icons/material-icons.css'
import 'quasar/src/css/index.sass'
import {BottomSheet, Dialog, Loading, Notify, Quasar} from 'quasar'
import {
    installStartupErrorHandlers,
    logStartupError,
    logStartupPhase,
} from './utils/startupLog';
import {startSyncedSettingsSync} from './utils/syncedSettings';

installStartupErrorHandlers();
logStartupPhase('frontend entry evaluated');

const app = createApp(Root);
app.config.errorHandler = (error, _instance, info) => {
    logStartupError(`Vue error (${info})`, error);
};
logStartupPhase('Vue app created');
app.use(createPinia());
startSyncedSettingsSync();
logStartupPhase('Pinia installed');
app.use(router);
logStartupPhase('Router installed');
app.use(Quasar, {
    plugins: {
        Notify,
        Dialog,
        BottomSheet,
        Loading,
    },
});
logStartupPhase('Quasar installed');

app.directive('click-outside', clickOutsideDirective);

app.mount("#app");
logStartupPhase('Vue root mounted');
void router.isReady()
    .then(() => logStartupPhase(`Router ready (${String(router.currentRoute.value.name)})`))
    .catch(error => logStartupError('Router initialization failed', error));
