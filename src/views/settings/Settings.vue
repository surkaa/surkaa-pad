<template>
  <main id="settings-page">
    <header class="settings-header">
      <button class="back-btn" @click="router.back()">
        <span class="icon">←</span>
      </button>
      <h1>设置</h1>
      <div class="header-placeholder"></div>
    </header>

    <div class="settings-content">
      <section class="settings-group">
        <h2 class="group-title">外观界面</h2>
        <div class="settings-card">
          <div class="setting-item theme-selector">
            <span class="label">显示模式</span>
            <div class="theme-options">
              <button
                  v-for="opt in themeOptions"
                  :key="opt.value"
                  :class="['theme-btn', { active: appStore.theme === opt.value }]"
                  @click="appStore.setTheme(opt.value)"
              >
                <span class="theme-icon">{{ opt.icon }}</span>
                <span class="theme-label">{{ opt.label }}</span>
              </button>
            </div>
          </div>
        </div>
      </section>

      <section class="settings-group">
        <h2 class="group-title">安全隐私</h2>
        <div class="settings-card">
          <div class="setting-item" :class="{'disabled-setting-item': !isAndroid}">
            <div class="item-info">
              <span class="label">生物识别解锁</span>
              <p class="description">使用指纹或面容快速解锁应用</p>
            </div>
            <label class="pad-switch">
              <input
                  type="checkbox"
                  :checked="appStore.isBiometricEnabled"
                  @change="handleBiometricToggle"
              >
              <span class="slider"></span>
            </label>
          </div>
        </div>
      </section>

      <section class="settings-group">
        <h2 class="group-title">数据管理</h2>
        <div class="settings-card">
          <button class="setting-item danger" @click="handleReset">
            <span class="label">重置应用配置</span>
            <span class="icon">›</span>
          </button>
        </div>
      </section>
    </div>

    <Transition name="fade">
      <div v-if="showPasswordVerify" class="modal-mask">
        <div class="modal-container">
          <h3>验证主密码</h3>
          <p>请输主密码以授权生物识别解锁</p>
          <input
              v-model="verifyPassword"
              type="password"
              placeholder="请输入主密码"
              class="pad-input"
              @keyup.enter="confirmEnableBiometric"
          >
          <div class="modal-actions">
            <button class="btn-text" @click="cancelBiometric">取消</button>
            <button
                class="btn-primary"
                :disabled="!verifyPassword || loading"
                @click="confirmEnableBiometric"
            >
              {{ loading ? '验证中...' : '确认开启' }}
            </button>
          </div>
        </div>
      </div>
    </Transition>
  </main>
</template>

<script setup lang="ts">
import {ref} from 'vue';
import {useRouter} from 'vue-router';
import {useAppStore} from "../../stores/app.ts";
import {showToast} from "../../utils";
import {platform} from "@tauri-apps/plugin-os";

const router = useRouter();
const appStore = useAppStore();

const themeOptions = [
  {label: '跟随系统', value: 'system', icon: '🖥️'},
  {label: '浅色模式', value: 'light', icon: '🌞'},
  {label: '深色模式', value: 'dark', icon: '🌜'}
] as const;

const showPasswordVerify = ref(false);
const verifyPassword = ref('');
const loading = ref(false);
const isAndroid = ref(platform() == 'android');

// 处理生物识别开关切换
async function handleBiometricToggle(e: Event) {
  const checkbox = e.target as HTMLInputElement;
  const newValue = checkbox.checked;

  if (newValue) {
    // 开启流程：先弹窗验证密码
    showPasswordVerify.value = true;
    verifyPassword.value = '';
    // 先把复选框状态还原，等验证成功再勾选
    checkbox.checked = false;
  } else {
    // 关闭流程：直接关闭
    if (confirm('确定要关闭生物识别解锁吗？')) {
      await appStore.disableBiometric();
      showToast('生物识别已禁用', 'info');
    } else {
      checkbox.checked = true;
    }
  }
}

// 确认开启生物识别
async function confirmEnableBiometric() {
  if (!verifyPassword.value) return;
  loading.value = true;
  try {
    await appStore.enableBiometric(verifyPassword.value);
    showToast('生物识别已成功开启', 'success');
    showPasswordVerify.value = false;
  } catch (err: any) {
    showToast(`验证失败: ${err.message || err}`, 'error');
  } finally {
    loading.value = false;
  }
}

function cancelBiometric() {
  showPasswordVerify.value = false;
  verifyPassword.value = '';
}

function handleReset() {
  if (confirm('确定要重置所有配置吗？此操作不可撤销。')) {
    appStore.resetConfig().then(() => {
      showToast('配置已重置', 'success');
      router.replace('/unlock');
    });
  }
}
</script>

<style scoped lang="scss">
#settings-page {
  width: 100%;
  height: 100%;
  background-color: var(--pad-bg-color-100);
  color: var(--pad-text-color-200);
  display: flex;
  flex-direction: column;

  .settings-header {
    padding: 16px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    background-color: var(--pad-bg-color-100);
    border-bottom: 1px solid var(--pad-border-color-100);

    h1 {
      font-size: 1.2rem;
      margin: 0;
      color: var(--pad-text-color-100);
    }

    .back-btn {
      background: none;
      border: none;
      font-size: 1.5rem;
      color: var(--pad-primary-color);
      cursor: pointer;
      padding: 8px;
    }

    .header-placeholder {
      width: 40px;
    }
  }

  .settings-content {
    flex: 1;
    padding: 20px;
    overflow-y: auto;
  }

  .settings-group {
    margin-bottom: 24px;

    .group-title {
      font-size: 0.85rem;
      color: var(--pad-text-color-400);
      margin-bottom: 8px;
      padding-left: 8px;
      text-transform: uppercase;
      letter-spacing: 1px;
    }
  }

  .settings-card {
    background-color: var(--pad-bg-color-200);
    border-radius: var(--pad-radius-lg);
    border: 1px solid var(--pad-border-color-100);
    overflow: hidden;
  }

  .setting-item {
    padding: 16px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    background: none;
    border: none;
    text-align: left;
    border-bottom: 1px solid var(--pad-border-color-100);

    &:last-child {
      border-bottom: none;
    }

    .label {
      font-weight: 500;
      color: var(--pad-text-color-100);
      display: block;
    }

    .description {
      font-size: 0.8rem;
      color: var(--pad-text-color-400);
      margin: 4px 0 0 0;
    }

    &.danger {
      width: 100%;
      display: flex;
      justify-content: space-between;
      cursor: pointer;
      background-color: transparent;

      .label {
        color: var(--pad-danger-color);
      }
    }
  }

  /* 主题选择器特定样式 */
  .theme-selector {
    flex-direction: column;
    align-items: flex-start;
    gap: 12px;

    .theme-options {
      width: 100%;
      display: grid;
      grid-template-columns: repeat(3, 1fr);
      gap: 10px;
    }

    .theme-btn {
      display: flex;
      flex-direction: column;
      align-items: center;
      padding: 12px;
      background-color: var(--pad-bg-color-100);
      border: 2px solid transparent;
      border-radius: var(--pad-radius-md);
      cursor: pointer;
      transition: var(--pad-transition-base);

      .theme-icon {
        font-size: 1.2rem;
        margin-bottom: 4px;
      }

      .theme-label {
        font-size: 0.75rem;
        color: var(--pad-text-color-300);
      }

      &.active {
        border-color: var(--pad-primary-color);
        background-color: var(--pad-primary-light);

        .theme-label {
          color: var(--pad-primary-dark);
          font-weight: bold;
        }
      }
    }
  }
}

.pad-switch {
  position: relative;
  display: inline-block;
  width: 48px;
  height: 24px;

  input {
    opacity: 0;
    width: 0;
    height: 0;
  }

  .slider {
    position: absolute;
    cursor: pointer;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    background-color: var(--pad-bg-color-400);
    transition: .4s;
    border-radius: 24px;

    &:before {
      position: absolute;
      content: "";
      height: 18px;
      width: 18px;
      left: 3px;
      bottom: 3px;
      background-color: white;
      transition: .4s;
      border-radius: 50%;
      box-shadow: var(--pad-shadow-sm);
    }
  }

  input:checked + .slider {
    background-color: var(--pad-primary-color);
  }

  input:checked + .slider:before {
    transform: translateX(24px);
  }
}

/* --- 通用组件样式 --- */
.pad-input {
  width: 100%;
  padding: 12px;
  border-radius: var(--pad-radius-md);
  border: 1px solid var(--pad-border-color-200);
  background-color: var(--pad-bg-color-100);
  color: var(--pad-text-color-100);
  margin: 16px 0;
  box-sizing: border-box;

  &:focus {
    outline: none;
    border-color: var(--pad-primary-color);
  }
}

/* --- 模态框样式 --- */
.modal-mask {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.5);
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 20px;
  z-index: 100;
}

.modal-container {
  background: var(--pad-bg-color-100);
  padding: 24px;
  border-radius: var(--pad-radius-xl);
  width: 100%;
  max-width: 320px;
  box-shadow: var(--pad-shadow-xl);

  h3 {
    margin: 0 0 8px 0;
    color: var(--pad-text-color-100);
  }

  p {
    font-size: 0.9rem;
    color: var(--pad-text-color-300);
    margin: 0;
  }

  .modal-actions {
    display: flex;
    justify-content: flex-end;
    gap: 12px;
  }

  .btn-text {
    background: none;
    border: none;
    color: var(--pad-text-color-400);
    cursor: pointer;
  }

  .btn-primary {
    background: var(--pad-primary-gradient);
    color: white;
    border: none;
    padding: 8px 16px;
    border-radius: var(--pad-radius-md);
    cursor: pointer;

    &:disabled {
      opacity: 0.6;
    }
  }
}

.fade-enter-active, .fade-leave-active {
  transition: opacity 0.3s;
}

.fade-enter-from, .fade-leave-to {
  opacity: 0;
}

/* 禁用状态的设置项样式 */
.setting-item.disabled-setting-item {
  /* 基础视觉置灰 */
  opacity: 0.6;
  filter: grayscale(0.8);

  /* 禁止交互 */
  pointer-events: none;
  cursor: not-allowed;

  /* 改变背景色以区别于普通项 */
  background-color: var(--pad-bg-color-300);

  .label {
    color: var(--pad-text-color-400);
  }

  .description {
    color: var(--pad-text-color-500);
  }

  /* 针对内部开关的特殊处理 */
  .pad-switch {
    .slider {
      background-color: var(--pad-bg-color-500) !important;

      &:before {
        background-color: var(--pad-border-color-100);
        box-shadow: none;
      }
    }
  }

  /* 可选：如果想在禁用时显示一个微小的提示 */
  &::after {
    content: "系统不支持";
    font-size: 10px;
    background: var(--pad-info-light);
    color: var(--pad-info-dark);
    padding: 2px 6px;
    border-radius: var(--pad-radius-sm);
    margin-left: 8px;
    position: absolute;
    right: 70px; /* 避开开关的位置 */
  }
}
</style>
