import {createRouter, createWebHashHistory, type RouteRecordRaw} from "vue-router";
import Unlock from "../views/unlock/Unlock.vue";
import DiaryList from "../views/diary-list/DiaryList.vue";
import Diary from "../views/diary/Diary.vue";
import PreviewMedia from "../views/PreviewMedia.vue";
import Settings from "../views/settings/Settings.vue";

const routes: RouteRecordRaw[] = [{
    name: 'Unlock',
    path: '/',
    component: Unlock,
    meta: {
        title: '解锁屏幕'
    }
}, {
    name: 'DiaryList',
    path: '/diary-list',
    component: DiaryList,
    meta: {
        title: '日志列表'
    }
}, {
    name: 'DiaryDetail',
    path: '/diary-detail/:id?',
    component: Diary,
    meta: {
        title: '日志详情'
    }
}, {
    name: 'PreviewMedia',
    path: '/preview-media',
    component: PreviewMedia,
    meta: {
        title: '媒体预览'
    }
}, {
    name: 'Settings',
    path: '/settings',
    component: Settings,
    meta: {
        title: '设置'
    }
}];

const router = createRouter({
    history: createWebHashHistory(),
    routes
});

export default router;