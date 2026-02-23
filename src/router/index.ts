import {createRouter, createWebHashHistory, type RouteRecordRaw} from "vue-router";
import Unlock from "../views/unlock/Unlock.vue";
import DiaryList from "../views/diary-list/DiaryList.vue";
import Diary from "../views/diary/Diary.vue";
import PreviewMedia from "../views/PreviewMedia.vue";
import Settings from "../views/settings/Settings.vue";
import {addCache, removeCache} from "../composables/useKeepAlive.ts";

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
        title: '日志列表',
        depth: 1
    }
}, {
    name: 'DiaryDetail',
    path: '/diary-detail/:id?',
    component: Diary,
    meta: {
        title: '日志详情',
        depth: 2
    }
}, {
    name: 'PreviewMedia',
    path: '/preview-media/:type/:diaryId/:filename',
    component: PreviewMedia,
    meta: {
        title: '媒体预览',
        depth: 3
    }
}, {
    name: 'Settings',
    path: '/settings',
    component: Settings,
    meta: {
        title: '设置',
        depth: 2
    }
}];

const router = createRouter({
    history: createWebHashHistory(),
    routes
});

router.beforeEach((to, from) => {
    const toDepth = (to.meta.depth as number) || 0;
    const fromDepth = (from.meta.depth as number) || 0;

    // 如果目标页面配置了需要缓存，先将它加入白名单
    if (to.name && to.meta.keepAlive) {
        addCache(to.name as string);
    }

    // 核心逻辑：如果是“后退”操作 (向浅层级跳转)
    if (toDepth < fromDepth) {
        // 并且离开的页面有 name，则销毁离开页面的缓存
        if (from.name) {
            removeCache(from.name as string);
        }
    }
});

export default router;