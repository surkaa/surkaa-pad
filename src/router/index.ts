import {createRouter, createWebHashHistory, type RouteRecordRaw} from "vue-router";
import Unlock from "../views/Unlock.vue";
import DiaryList from "../views/DiaryList.vue";
import Diary from "../views/Diary.vue";

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
    path: '/diary-detail',
    component: Diary,
    meta: {
        title: '日志详情'
    }
}];

const router = createRouter({
    history: createWebHashHistory(),
    routes
});

export default router;