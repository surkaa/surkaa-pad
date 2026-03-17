import {defineStore} from "pinia";
import {useQuasar} from "quasar";
import {computed, ref} from "vue";
import {exit} from "@tauri-apps/plugin-process";
import {useTimestamp} from "@vueuse/core";
import {useConfigStore} from "./config.ts";

// 解锁后1小时自动关闭应用
const AUTO_CLOSE_APP_TIMEOUT = 60 * 60 * 1000;
// 时间到时剩余操作时间
const AUTO_CLOSE_APP_WARNING_TIME = 60 * 1000;

export const useTimeoutStore = defineStore('timeout', () => {
    let closeTimer: ReturnType<typeof setTimeout> | null = null;
    // 初始设置为 null，等待从 config 中恢复
    const startTime = ref<number | null>(null);
    const $q = useQuasar();
    const configStore = useConfigStore();

    // 倒计时
    const now = useTimestamp();

    // 计算剩余时间字符串，格式为 MM:SS
    const remainingStr = computed(() => {
        if (!startTime.value) return "00:00"; // 尚未加载时显示 00:00

        const diff = new Date(startTime.value + AUTO_CLOSE_APP_TIMEOUT).getTime() - now.value;
        const ms = Math.max(0, diff);
        const seconds = Math.floor(ms / 1000) % 60;
        const minutes = Math.floor(ms / (1000 * 60));
        return `${minutes.toString().padStart(2, '0')}:${seconds.toString().padStart(2, '0')}`;
    });

    function executeWarningAndExit() {
        $q.dialog({
            title: '安全提示',
            message: '为了保护您的数据安全，应用将于一分钟后自动关闭。请保存您的工作。'
        });

        setTimeout(() => exit(0), AUTO_CLOSE_APP_WARNING_TIME);

        setTimeout(() => {
            $q.dialog({
                title: '安全提示',
                message: '应用即将自动关闭以保护您的数据安全，请保存您的工作。'
            });
        }, AUTO_CLOSE_APP_WARNING_TIME / 2);
    }

    // 设置或恢复定时器
    function setupTimers(startMs: number) {
        if (closeTimer) clearTimeout(closeTimer);

        const timeElapsed = Date.now() - startMs;
        const timeToWarning = AUTO_CLOSE_APP_TIMEOUT - timeElapsed;

        if (timeToWarning > 0) {
            // 倒计时还没结束，正常设置剩余时间的定时器
            console.log(`恢复自动关闭应用定时器，将在 ${Math.floor(timeToWarning / 1000 / 60)} 分钟后警告`);
            closeTimer = setTimeout(() => {
                executeWarningAndExit();
            }, timeToWarning);
        } else {
            // 已经超过了 60 分钟，但还在警告宽限期
            const timeToExit = (AUTO_CLOSE_APP_TIMEOUT + AUTO_CLOSE_APP_WARNING_TIME) - timeElapsed;
            if (timeToExit > 0) {
                // 还在宽限期内，直接触发警告并开始最后倒计时
                executeWarningAndExit();
            } else {
                // 已经彻底超时，直接退出应用
                console.log('应用已超时，强制退出');
                exit(0).then();
            }
        }
    }

    // 应用初始化时调用：尝试从配置文件中恢复时间
    async function initTimeoutFromConfig() {
        const savedTime = await configStore.getNormalConfig('unlock_start_time');
        if (savedTime) {
            startTime.value = savedTime;
            setupTimers(savedTime);
        }
    }

    // 每次重新解锁时调用
    async function setTimeoutForCloseApp() {
        console.log('重置自动关闭应用定时器并保存配置');
        const currentMs = Date.now();
        startTime.value = currentMs;

        // 持久化存储起始时间
        await configStore.saveNormalConfig('unlock_start_time', currentMs);

        // 重新设置定时器
        setupTimers(currentMs);
    }

    // 初始化执行一次恢复逻辑
    initTimeoutFromConfig().then();

    return {
        remainingStr,
        setTimeoutForCloseApp,
    }
});