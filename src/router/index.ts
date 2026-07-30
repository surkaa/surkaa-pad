import {createRouter, createWebHashHistory, type RouteRecordRaw} from "vue-router";
import Unlock from "../views/unlock/Unlock.vue";
import DiaryList from "../views/diary-list/DiaryList.vue";
import {addCache, removeCache} from "../composables/useKeepAlive.ts";
import Layout from "../layout/Layout.vue";
import {useLayoutStore} from "../stores/layout.ts";

const routes: RouteRecordRaw[] = [
    {
        name: 'Unlock',
        path: '/',
        component: Unlock,
        meta: {
            title: '解锁屏幕'
        }
    },
    {
        path: '/',
        component: Layout,
        children: [
            {
                name: 'DiaryList',
                path: 'diary-list',
                component: DiaryList,
                meta: {
                    title: '日记列表',
                    depth: 1,
                    keepAlive: true,
                }
            },
            {
                name: 'DiaryDetail',
                path: 'diary-detail/:id?',
                component: () => import('../views/diary-detial/DiaryDetail.vue'),
                meta: {
                    title: '日记详情',
                    // 可以从日记列表或者日记搜索进入，所以深度设置为 3
                    depth: 3,
                    keepAlive: true,
                    hideFooter: true
                }
            },
            {
                name: 'DiarySearch',
                path: 'diary-search',
                component: () => import('../views/diary-search/DiarySearch.vue'),
                meta: {
                    title: '🔍',
                    depth: 2,
                    keepAlive: true,
                    searchMod: true
                }
            },
            {
                name: 'AiAssistant',
                path: 'ai-assistant',
                component: () => import('../views/ai-assistant/AiAssistant.vue'),
                meta: {
                    title: 'AI 助手',
                    depth: 2,
                    keepAlive: true,
                    hideFooter: true
                }
            },
            {
                name: 'Settings',
                path: 'settings',
                component: () => import('../views/settings/Settings.vue'),
                meta: {
                    title: '设置',
                    depth: 2,
                    hideFooter: true
                }
            }
        ]
    }
];

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

router.afterEach(() => {
    // 重置标题避免污染其他页面
    const layoutStore = useLayoutStore();
    layoutStore.resetTitle();
});

export default router;
