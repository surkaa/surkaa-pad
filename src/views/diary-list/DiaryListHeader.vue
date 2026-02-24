<script setup lang="ts">
import {useAppStore} from "../../stores/app.ts";
import {computed, onMounted, onUnmounted, ref} from "vue";
import {getName} from "@tauri-apps/api/app";

defineProps<{
  stats: {
    total: number;
    withAttachments: number;
  }
}>();

const emit = defineEmits(['settings']);

const appStore = useAppStore();
const futureTimestamp = ref(Date.now());
const appName = ref('App Name');
let timer: number | null = null;

// 剩余时间（秒）
const remainingSeconds = ref(0)

// 计算分秒
const minutes = computed(() =>
    Math.floor((remainingSeconds.value % 3600) / 60).toString().padStart(2, '0')
)
const seconds = computed(() =>
    Math.floor(remainingSeconds.value % 60).toString().padStart(2, '0')
)

// 更新剩余时间
const updateCountdown = () => {
  const now = Math.floor(Date.now() / 1000); // 当前时间戳（秒）
  const future = Math.floor(futureTimestamp.value / 1000); // 未来时间戳（秒）
  remainingSeconds.value = Math.max(0, future - now);
}

onMounted(async () => {
  appName.value = await getName();
  futureTimestamp.value = appStore.getEndTime();
  updateCountdown();
  timer && clearInterval(timer);
  timer = setInterval(updateCountdown, 1000);
});

onUnmounted(() => {
  timer && clearInterval(timer);
});
</script>

<template>
  <header class="app-header">
    <div class="header-content">
      <div class="logo-section">
        <span class="app-title">
          <img alt="app-logo" class="app-logo" src="/app-icon.png"/>
          {{ appName }}
        </span>
        <!--倒计时-->
        <div class="countdown-timer">
          <small class="countdown-time"
                 :style="{ color: remainingSeconds <= 300 ? 'var(--pad-danger-color)' : 'var(--pad-text-color-400)' }">
            {{ minutes }}:{{ seconds }}
          </small>
        </div>
      </div>

      <div class="right-action-section">
        <div class="stats-section">
          <div class="stat-item">
            <span class="stat-icon">📚</span>
            <div class="stat-values">
              <span class="stat-value">{{ stats.total }}</span>
              <span class="stat-label">篇日记</span>
            </div>
          </div>
          <div class="stat-item" v-if="stats.withAttachments > 0">
            <span class="stat-icon">📎</span>
            <div class="stat-values">
              <span class="stat-value">{{ stats.withAttachments }}</span>
              <span class="stat-label">含附件</span>
            </div>
          </div>
        </div>

        <q-icon name="settings" class="settings-icon" @click="emit('settings')" title="系统设置"/>
      </div>
    </div>
  </header>
</template>

<style scoped lang="scss">
.app-header {
  background-color: var(--pad-bg-color-100);
  padding: 16px 24px 12px;
  flex-shrink: 0;

  .header-content {
    display: flex;
    justify-content: space-between;
    align-items: center;

    .logo-section {
      .app-title {
        font-size: 20px;
        font-weight: 700;
        color: var(--pad-text-color-100);
        margin: 0 0 4px;
        display: flex;
        align-items: center;
        gap: 8px;

        .app-logo {
          width: 32px;
          height: 32px;
          font-size: 24px;
        }
      }

      .countdown-timer {
        display: flex;
        padding-left: 4px;

        .countdown-time {
          font-size: 12px;
        }
      }
    }

    .right-action-section {
      display: flex;
      align-items: center;
      gap: 16px;

      .stats-section {
        display: flex;
        gap: 16px;

        .stat-item {
          display: flex;
          flex-direction: column;
          align-items: center;
          min-width: 60px;

          .stat-icon {
            font-size: 20px;
            width: 40px;
            height: 30px;
            display: flex;
            align-items: center;
            justify-content: center;
            background-color: var(--pad-primary-color-light);
            border-radius: var(--pad-radius-full);
            color: var(--pad-text-color-light);
          }

          .stat-values {
            display: flex;
            flex-direction: row;
            align-items: end;
            justify-content: center;
            gap: 2px;

            .stat-value {
              font-size: 16px;
              font-weight: 600;
              color: var(--pad-text-color-200);
            }

            .stat-label {
              font-size: 10px;
              color: var(--pad-text-color-400);
            }
          }
        }
      }

      .settings-icon {
        font-size: 24px;
      }
    }
  }
}

@media (max-width: 512px) {
  .app-header {
    padding: 4px 16px 8px;

    .header-content {
      height: 100%;
      flex-direction: row;
      align-items: flex-start;
      gap: 16px;

      .logo-section {
        width: 100%;
        height: 100%;
        display: flex;
        justify-content: start;
        align-items: center;

        .app-title {
          font-size: 18px;
        }
      }

      .stats-section {
        justify-content: space-between;
        gap: 8px;

        .stat-item {

          .stat-icon {
            width: 36px;
            height: 36px;
            font-size: 18px;
          }

          .stat-values {
            flex-direction: row;
            gap: 2px;

            .stat-value {
              font-size: 16px;
            }
          }
        }
      }
    }
  }
}

@media (min-width: 513px) and (max-width: 768px) {
  .app-header {
    padding: 16px 20px 12px;
  }
}
</style>
