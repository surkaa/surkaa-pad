import {defineStore} from "pinia";
import {useQuasar} from "quasar";
import {computed, ref} from "vue";
import {exit} from "@tauri-apps/plugin-process";
import {useTimestamp} from "@vueuse/core";

// 解锁后1小时进入关闭提醒，再留出最后一分钟保存工作。
const AUTO_CLOSE_APP_TIMEOUT = 60 * 60 * 1000;
// 时间到时剩余操作时间
const AUTO_CLOSE_APP_WARNING_TIME = 60 * 1000;

export const useTimeoutStore = defineStore('timeout', () => {
    let closeTimer: ReturnType<typeof setTimeout> | null = null;
    let exitTimer: ReturnType<typeof setTimeout> | null = null;
    let dismissWarning: (() => void) | null = null;
    let timerGeneration = 0;
    const pauseCount = ref(0);
    const autoClosePaused = computed(() => pauseCount.value > 0);
    const phase = ref<'idle' | 'countdown' | 'grace' | 'expired'>('idle');
    const deadline = ref(0);
    const $q = useQuasar();

    // 倒计时
    const now = useTimestamp();
    const remainingMs = computed(() => phase.value === 'idle' || phase.value === 'expired'
        ? 0 : Math.max(0, deadline.value - now.value));
    const isExpiring = computed(() => !autoClosePaused.value && (
        phase.value === 'grace' || phase.value === 'expired'
        || (phase.value === 'countdown' && remainingMs.value <= AUTO_CLOSE_APP_TIMEOUT / 10)
    ));

    // 计算剩余时间字符串，格式为 MM:SS
    const remainingStr = computed(() => {
        if (autoClosePaused.value) return "已暂停";
        // 向上取整，避免尚有不足一秒时提前显示 00:00。
        const totalSeconds = Math.ceil(remainingMs.value / 1000);
        const seconds = totalSeconds % 60;
        const minutes = Math.floor(totalSeconds / 60);
        return `${minutes.toString().padStart(2, '0')}:${seconds.toString().padStart(2, '0')}`;
    });


    function setTimeoutForCloseApp() {
        clearCloseTimers();
        // 改密期间即使解锁/其他代码请求重新计时，也不能恢复自动退出。
        if (autoClosePaused.value) return;
        console.log('设置自动关闭应用定时器');
        now.value = Date.now();
        deadline.value = now.value + AUTO_CLOSE_APP_TIMEOUT;
        phase.value = 'countdown';
        const generation = timerGeneration;
        const isCurrent = () => generation === timerGeneration && !autoClosePaused.value;
        closeTimer = setTimeout(() => {
            if (!isCurrent() || phase.value !== 'countdown') return;
            closeTimer = null;
            console.log('即将自动关闭应用');
            // 进入独立的最后一分钟，显示从 01:00 重新倒计时，始终保持红色。
            now.value = Date.now();
            deadline.value = now.value + AUTO_CLOSE_APP_WARNING_TIME;
            phase.value = 'grace';
            dismissWarning = $q.notify({
                icon: 'warning_amber',
                position: 'top',
                timeout: 0,
                group: false,
                classes: 'pad-auto-close-warning',
                message: '应用将在 1 分钟后自动关闭以保护您的数据安全',
                caption: '请及时保存当前工作'
            });
            exitTimer = setTimeout(
                () => {
                    if (!isCurrent() || phase.value !== 'grace') return;
                    exitTimer = null;
                    now.value = Date.now();
                    phase.value = 'expired';
                    dismissWarning?.();
                    dismissWarning = null;
                    void exit(0);
                },
                AUTO_CLOSE_APP_WARNING_TIME
            );
        }, AUTO_CLOSE_APP_TIMEOUT);
    }

    function clearCloseTimers() {
        timerGeneration += 1;
        if (closeTimer) clearTimeout(closeTimer);
        if (exitTimer) clearTimeout(exitTimer);
        closeTimer = null;
        exitTimer = null;
        dismissWarning?.();
        dismissWarning = null;
        phase.value = 'idle';
    }

    /** 改密任务持有暂停直到 finally；重复释放无效，全部任务结束后重新计时一小时。 */
    function pauseAutoClose() {
        pauseCount.value += 1;
        clearCloseTimers();
        let released = false;
        return () => {
            if (released) return;
            released = true;
            pauseCount.value -= 1;
            if (!autoClosePaused.value) setTimeoutForCloseApp();
        };
    }

    return {
        remainingStr,
        isExpiring,
        autoClosePaused,
        pauseAutoClose,
        setTimeoutForCloseApp,
        clearCloseTimers,
    }
});
