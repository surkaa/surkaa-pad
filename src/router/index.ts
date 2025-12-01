import {createRouter, createWebHashHistory, type RouteRecordRaw} from "vue-router";
import Unlock from "../views/Unlock.vue";
import DiaryList from "../views/DiaryList.vue";
import Diary from "../views/Diary.vue";

const routes: RouteRecordRaw[] = [{
    name: 'unlock',
    path: '/',
    component: Unlock,
    meta: {
        title: '解锁屏幕'
    }
}, {
    name: 'diary-list',
    path: '/diary-list',
    component: DiaryList,
    meta: {
        title: '日志列表'
    }
}, {
    name: 'diary',
    path: '/diary/:id',
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