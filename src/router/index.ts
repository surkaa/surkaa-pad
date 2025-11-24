import { createRouter, createWebHashHistory, RouteRecordRaw } from 'vue-router';
import { useAppStore } from '../stores/app';

// 导入视图组件
import LoginView from '../views/Login.vue';
import HomeView from '../views/Home.vue';

const routes: Array<RouteRecordRaw> = [
    {
        path: '/',
        name: 'Home',
        component: HomeView,
        meta: { requiresAuth: true }
    },
    {
        path: '/login',
        name: 'Login',
        component: LoginView,
    },
    // 将 'list' 和 'editor' 模式放在 Home 视图内部管理
];

const router = createRouter({
    history: createWebHashHistory(),
    routes,
});

// 导航守卫：如果未登录，则重定向到 /login
router.beforeEach((to, _from, next) => {
    const store = useAppStore();

    // 如果正在初始化，等待
    if (!store.hasSavedConfig && to.name !== 'Login') {
        // 如果没有配置，直接去登录页面进行设置
        return next({ name: 'Login' });
    }

    if (to.meta.requiresAuth && !store.isLoggedIn) {
        // 如果需要认证但未登录，重定向到登录页
        next({ name: 'Login' });
    } else if (to.name === 'Login' && store.isLoggedIn) {
        // 如果已登录但尝试访问登录页，重定向到主页
        next({ name: 'Home' });
    } else {
        next();
    }
});


export default router;