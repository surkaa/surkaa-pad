import {defineStore} from "pinia";
import {useQuasar} from "quasar";
import {computed, ref} from "vue";
import {exit} from "@tauri-apps/plugin-process";
import {useTimestamp} from "@vueuse/core";

// 解锁后1小时自动关闭应用
const AUTO_CLOSE_APP_TIMEOUT = 60 * 60 * 1000;
// 时间到时剩余操作时间
const AUTO_CLOSE_APP_WARNING_TIME = 60 * 1000;

export const useTimeoutStore = defineStore('timeout', () => {
    let closeTimer: ReturnType<typeof setTimeout> | null = null;
    let startTime = ref(Date.now());
    const $q = useQuasar();

    // 倒计时
    const now = useTimestamp({offset: 0, interval: 1000})

    // 计算剩余时间字符串，格式为 MM:SS
    const remainingStr = computed(() => {
        const diff = new Date(startTime.value + AUTO_CLOSE_APP_TIMEOUT).getTime() - now.value
        const ms = Math.max(0, diff);
        const seconds = Math.floor(ms / 1000) % 60;
        const minutes = Math.floor(ms / (1000 * 60)) % 60;
        return `${minutes.toString().padStart(2, '0')}:${seconds.toString().padStart(2, '0')}`;
    });

    function setTimeoutForCloseApp() {
        if (closeTimer) clearTimeout(closeTimer);
        console.log('设置自动关闭应用定时器');
        startTime.value = Date.now();
        closeTimer = setTimeout(() => {
            console.log('即将自动关闭应用');
            // 60s后退出
            $q.dialog({
                title: '安全提示',
                message: '为了保护您的数据安全，应用将于一分钟后自动关闭。请保存您的工作。'
            });
            setTimeout(
                () => exit(0),
                AUTO_CLOSE_APP_WARNING_TIME
            );
            setTimeout(() => {
                $q.dialog({
                    title: '安全提示',
                    message: '应用即将自动关闭以保护您的数据安全，请保存您的工作。'
                });
            }, AUTO_CLOSE_APP_WARNING_TIME / 2);
        }, AUTO_CLOSE_APP_TIMEOUT);
    }

    return {
        remainingStr,
        setTimeoutForCloseApp,
    }
});