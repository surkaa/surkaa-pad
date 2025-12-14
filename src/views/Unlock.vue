<template>
  <main id="unlock">
    <div class="unlock-container">
      <!-- 应用标题区域 -->
      <div class="app-header">
        <h1 class="app-title">
          <img alt="app-logo" class="app-logo" src="/app-icon.png"/>
          SurKaa Pad
        </h1>
      </div>

      <!-- 分界线 -->
      <div class="divider"></div>

      <!-- 主要内容区域 -->
      <div class="content-area">
        <!-- 加载配置状态 -->
        <section v-if="pipeline === 'wait-load-config'" class="state-section loading-state">
          <div class="loading-indicator">
            <div class="loading-spinner"></div>
            <p class="loading-text">正在加载配置...</p>
          </div>
        </section>

        <!-- 登录解锁状态 -->
        <section v-else-if="pipeline === 'login'" class="state-section login-section">
          <div class="section-header">
            <h2 class="section-title">欢迎回来</h2>
          </div>

          <form @submit.prevent="unlock" class="unlock-form">
            <div class="input-group">
              <input
                  autofocus
                  id="master-password"
                  type="password"
                  required
                  placeholder="输入主密码"
                  v-model="masterPassword"
                  class="password-input"
                  :disabled="loading"
              />
            </div>

            <button
                type="submit"
                :disabled="loading"
                class="submit-btn"
                :class="{'loading': loading}"
            >
              <span class="btn-text">{{ loading ? '正在验证...' : '解锁' }}</span>
              <span class="btn-icon">
                <svg v-if="!loading" class="unlock-icon" viewBox="0 0 24 24">
                  <path
                      d="M18 8h-1V6c0-2.76-2.24-5-5-5S7 3.24 7 6v2H6c-1.1 0-2 .9-2 2v10c0 1.1.9 2 2 2h12c1.1 0 2-.9 2-2V10c0-1.1-.9-2-2-2zm-6 9c-1.1 0-2-.9-2-2s.9-2 2-2 2 .9 2 2-.9 2-2 2zm3.1-9H8.9V6c0-1.71 1.39-3.1 3.1-3.1 1.71 0 3.1 1.39 3.1 3.1v2z"/>
                </svg>
                <span v-else class="loading-animation">
                  <span class="loading-dot dot-1"></span>
                  <span class="loading-dot dot-2"></span>
                  <span class="loading-dot dot-3"></span>
                </span>
              </span>
            </button>
          </form>

          <div class="footer-actions">
            <button class="reset-btn link-btn" @click="confirmReset" :disabled="loading">
              重置配置
            </button>
          </div>
        </section>

        <!-- 首次配置状态 -->
        <section v-else-if="pipeline === 'config'" class="state-section config-section">
          <div class="section-header">
            <h2 class="section-title">首次配置</h2>
          </div>

          <form @submit.prevent="saveConfigAndLogin" class="config-form">
            <div class="input-group">
              <input
                  id="master-password"
                  type="password"
                  required
                  placeholder="设置主密码"
                  v-model="masterPassword"
                  class="password-input"
                  :disabled="loading"
              />
            </div>

            <div class="config-toggle">
              <button
                  type="button"
                  class="toggle-btn"
                  @click="showQuickInput = !showQuickInput"
              >
                <span class="toggle-icon">
                  <svg viewBox="0 0 24 24" width="16" height="16">
                    <path v-if="!showQuickInput" d="M9 16.17L4.83 12l-1.42 1.41L9 19 21 7l-1.41-1.41z"/>
                    <path v-else d="M12 4l-1.41 1.41L16.17 11H4v2h12.17l-5.58 5.59L12 20l8-8z"/>
                  </svg>
                </span>
                <span class="toggle-text">
                  {{ showQuickInput ? '使用常规配置' : '使用快速配置' }}
                </span>
              </button>
            </div>

            <div v-if="!showQuickInput" class="oss-config-group">
              <div class="input-group">
                <input
                    id="access-key-id"
                    required
                    type="text"
                    placeholder="输入 AccessKey ID"
                    v-model="ossConfig.akid"
                    class="config-input"
                    :disabled="loading"
                />
              </div>

              <div class="input-group">
                <input
                    id="access-key-secret"
                    required
                    type="password"
                    placeholder="输入 AccessKey Secret"
                    v-model="ossConfig.aks"
                    class="config-input"
                    :disabled="loading"
                />
              </div>

              <div class="input-group">
                <input
                    id="bucket-name"
                    required
                    type="text"
                    placeholder="输入 Bucket 名称"
                    v-model="ossConfig.bucket"
                    class="config-input"
                    :disabled="loading"
                />
              </div>

              <div class="input-group">
                <input
                    id="endpoint"
                    required
                    type="text"
                    placeholder="输入 Endpoint"
                    v-model="ossConfig.endpoint"
                    class="config-input"
                    :disabled="loading"
                />
              </div>
            </div>

            <div v-else class="quick-config-group">
              <div class="input-group">
                <textarea
                    id="quickConfig"
                    required
                    placeholder="ALIYUN_KEY=xxx
ALIYUN_SECRET=xxx
ALIYUN_BUCKET_NAME=xxx
ALIYUN_ENDPOINT=xxx"
                    v-model="quickConfig"
                    class="quick-config-input"
                    :disabled="loading"
                    rows="4"
                ></textarea>
              </div>
            </div>

            <button
                type="submit"
                :disabled="loading"
                class="submit-btn primary-btn"
                :class="{'loading': loading}"
            >
              <span class="btn-text">{{ loading ? '正在验证并保存...' : '保存并登录' }}</span>
              <span class="btn-icon">
                <svg v-if="!loading" class="save-icon" viewBox="0 0 24 24">
                  <path
                      d="M17 3H5c-1.11 0-2 .9-2 2v14c0 1.1.89 2 2 2h14c1.1 0 2-.9 2-2V7l-4-4zm-5 16c-1.66 0-3-1.34-3-3s1.34-3 3-3 3 1.34 3 3-1.34 3-3 3zm3-10H5V5h10v4z"/>
                </svg>
                <span v-else class="loading-animation">
                  <span class="loading-dot dot-1"></span>
                  <span class="loading-dot dot-2"></span>
                  <span class="loading-dot dot-3"></span>
                </span>
              </span>
            </button>
          </form>
        </section>

        <!-- 错误状态 -->
        <section v-else class="state-section error-section">
          <div class="error-icon">⚠️</div>
          <h2 class="error-title">发生了未知的错误</h2>
          <p class="error-message">请刷新页面重试或检查应用状态</p>
        </section>
      </div>
    </div>
  </main>
</template>

<script setup lang="ts">
import {onMounted, ref} from "vue";
import {useAppStore} from "../stores/app.ts";
import {OssConfigType} from "../types";
import {useRouter} from "vue-router";
import {showToast} from "../utils";

const pipeline = ref<'wait-load-config' | 'login' | 'config'>('wait-load-config');
const encryptedConfig = ref<number[]>([]);
const ossConfig = ref<OssConfigType>({
  akid: '',
  aks: '',
  bucket: '',
  endpoint: '',
});
const showQuickInput = ref<boolean>(false);
const quickConfig = ref('');
const masterPassword = ref<string>('');
const loading = ref<boolean>(false);

const appStore = useAppStore();
const router = useRouter();

function saveConfigAndLogin() {
  if (loading.value) return;
  loading.value = true;

  // 如果快速配置不为空 则解析快速配置
  if (quickConfig.value.trim() !== '') {
    if (masterPassword.value.trim() == '') {
      showToast("使用快速配置时，主密码不能为空。", 'error');
      loading.value = false;
      return;
    }
    console.log('使用快速配置：', quickConfig.value);
    const lines = quickConfig.value.split('\n').filter(line => line.includes('='));
    lines.forEach(line => {
      const [key, value] = line.split('=').map(s => s.trim());
      switch (key) {
        case 'ALIYUN_KEY':
          ossConfig.value.akid = value;
          break;
        case 'ALIYUN_SECRET':
          ossConfig.value.aks = value;
          break;
        case 'ALIYUN_BUCKET_NAME':
          ossConfig.value.bucket = value;
          break;
        case 'ALIYUN_ENDPOINT':
          ossConfig.value.endpoint = value;
          break;
        default:
          console.warn('未知的配置项：', key);
          loading.value = false;
          return;
      }
    });
  }

  appStore.saveConfigAndLogin(
      masterPassword.value,
      ossConfig.value,
  )
      .then(() => appStore.getEncryptedConfig())
      .then((ec) => {
        if (!ec) throw new Error('无法获取加密配置');
        encryptedConfig.value = ec;
        showToast("保存成功，请登录以验证主密码。", 'success');
        masterPassword.value = '';
        ossConfig.value = {
          akid: '',
          aks: '',
          bucket: '',
          endpoint: '',
        };
        pipeline.value = 'login';
      })
      .catch(err => showToast(`保存配置失败：${err.message || err}`, 'error'))
      .finally(() => loading.value = false);
}

function unlock() {
  if (loading.value) return;
  loading.value = true;
  appStore.unlock(masterPassword.value)
      .then(() => appStore.initOss(encryptedConfig.value))
      .then(() => {
        router.replace({name: 'DiaryList'});
        appStore.setTimeoutForCloseApp();
      })
      .catch(err => showToast(`解锁失败：${err.message || err}`, 'error'))
      .finally(() => loading.value = false);
}

function confirmReset() {
  // 确认是否重置
  if (confirm('确定要重置配置吗？这将删除所有本地配置。')) {
    appStore.resetConfig()
        .then(() => {
          pipeline.value = 'config';
          masterPassword.value = '';
        })
        .catch(err => showToast(`重置配置失败：${err.message || err}`, 'error'));
  }
}

onMounted(async () => {
  const ec = await appStore.getEncryptedConfig();
  if (ec) {
    pipeline.value = 'login';
    encryptedConfig.value = ec;
  } else {
    pipeline.value = 'config';
  }
});
</script>

<style scoped lang="scss">
#unlock {
  width: 100%;
  height: 100%;
  display: flex;
  justify-content: center;
  align-items: center;
  background-color: var(--pad-bg-color-100);
  font-family: var(--pad-font-family), serif;
  padding: 5%;
  box-sizing: border-box;
  user-select: none;

  .unlock-container {
    width: 100%;
    max-width: 512px;
    background-color: var(--pad-bg-color-200);
    border-radius: var(--pad-radius-xl);
    border: 1px solid var(--pad-border-color-100);
    box-shadow: var(--pad-shadow-lg);
    overflow: hidden;
    animation: container-enter 0.5s ease-out;
  }

  .app-header {
    padding: 32px 32px 24px;
    text-align: center;
    border-bottom: 1px solid var(--pad-border-color-100);
    background: linear-gradient(135deg, var(--pad-primary-color) 0%, var(--pad-primary-dark) 100%);
    color: var(--pad-text-color-light);

    .app-title {
      font-size: 32px;
      font-weight: 700;
      margin: 0 0 8px;
      display: flex;
      align-items: center;
      justify-content: center;
      gap: 12px;

      .app-logo {
        width: 48px;
        height: 48px;
        font-size: 36px;
        filter: drop-shadow(0 2px 4px rgba(0, 0, 0, 0.2));
      }
    }
  }

  .divider {
    height: 1px;
    background: var(--pad-border-color-100);
    margin: 0;
  }

  .content-area {
    padding: 32px;
  }

  .state-section {
    width: 100%;
  }

  // 加载状态样式
  .loading-state {
    display: flex;
    justify-content: center;
    align-items: center;
    min-height: 200px;

    .loading-indicator {
      text-align: center;

      .loading-spinner {
        width: 48px;
        height: 48px;
        border: 3px solid var(--pad-border-color-200);
        border-top-color: var(--pad-primary-color);
        border-radius: 50%;
        margin: 0 auto 16px;
        animation: spinner-rotate 1s linear infinite;
      }

      .loading-text {
        color: var(--pad-text-color-300);
        font-size: 16px;
        margin: 0;
      }
    }
  }

  // 登录和配置状态共用样式
  .login-section,
  .config-section {
    .section-header {
      margin-bottom: 32px;
      text-align: center;

      .section-title {
        font-size: 24px;
        font-weight: 600;
        color: var(--pad-text-color-100);
        margin: 0 0 8px;
      }
    }
  }

  // 输入组样式
  .input-group {
    margin-bottom: 20px;

    &:last-child {
      margin-bottom: 0;
    }
  }

  // 输入框样式
  .password-input,
  .config-input {
    width: 100%;
    padding: 12px 16px;
    font-size: 15px;
    line-height: 1.5;
    background-color: var(--pad-bg-color-100);
    border: 1px solid var(--pad-border-color-200);
    border-radius: var(--pad-radius-lg);
    color: var(--pad-text-color-100);
    transition: all var(--pad-transition-fast);
    box-sizing: border-box;

    &:focus {
      outline: none;
      border-color: var(--pad-primary-color);
      box-shadow: 0 0 0 3px var(--pad-primary-color-light);
    }

    &:disabled {
      opacity: 0.6;
      cursor: not-allowed;
      background-color: var(--pad-bg-color-200);
    }

    &::placeholder {
      color: var(--pad-text-color-400);
    }
  }

  .quick-config-input {
    width: 100%;
    padding: 12px 16px;
    font-size: 14px;
    line-height: 1.5;
    background-color: var(--pad-bg-color-100);
    border: 1px solid var(--pad-border-color-200);
    border-radius: var(--pad-radius-lg);
    color: var(--pad-text-color-100);
    transition: all var(--pad-transition-fast);
    box-sizing: border-box;
    font-family: 'SFMono-Regular', Consolas, 'Liberation Mono', Menlo, monospace;
    resize: vertical;

    &:focus {
      outline: none;
      border-color: var(--pad-primary-color);
      box-shadow: 0 0 0 3px var(--pad-primary-color-light);
    }

    &:disabled {
      opacity: 0.6;
      cursor: not-allowed;
      background-color: var(--pad-bg-color-200);
    }
  }

  // 配置切换按钮
  .config-toggle {
    margin: 24px 0;
    display: flex;
    justify-content: center;

    .toggle-btn {
      display: inline-flex;
      align-items: center;
      gap: 8px;
      padding: 8px 16px;
      background-color: var(--pad-bg-color-300);
      border: 1px solid var(--pad-border-color-200);
      border-radius: var(--pad-radius-full);
      color: var(--pad-text-color-300);
      font-size: 14px;
      cursor: pointer;
      transition: all var(--pad-transition-fast);

      &:hover {
        background-color: var(--pad-bg-color-400);
        color: var(--pad-text-color-200);
        border-color: var(--pad-border-color-300);
      }

      &:active {
        transform: scale(0.98);
      }

      .toggle-icon {
        display: flex;
        align-items: center;

        svg {
          fill: currentColor;
        }
      }
    }
  }

  // OSS配置组
  .oss-config-group {
    margin-bottom: 24px;
  }

  // 提交按钮
  .submit-btn {
    width: 100%;
    padding: 14px 24px;
    font-size: 16px;
    font-weight: 500;
    background: var(--pad-primary-gradient);
    color: var(--pad-text-color-light);
    border: none;
    border-radius: var(--pad-radius-lg);
    cursor: pointer;
    transition: all var(--pad-transition-base);
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 12px;
    position: relative;
    overflow: hidden;

    &:hover:not(:disabled) {
      transform: translateY(-2px);
      box-shadow: var(--pad-shadow-lg);
    }

    &:active:not(:disabled) {
      transform: translateY(0);
    }

    &:disabled {
      opacity: 0.7;
      cursor: not-allowed;
      transform: none;
      box-shadow: none;
    }

    &.loading {
      .btn-text {
        opacity: 0.8;
      }
    }

    .btn-icon {
      display: flex;
      align-items: center;
      justify-content: center;

      svg {
        width: 20px;
        height: 20px;
        fill: currentColor;
      }
    }

    // 加载动画
    .loading-animation {
      display: flex;
      align-items: center;
      justify-content: center;
      gap: 4px;

      .loading-dot {
        width: 8px;
        height: 8px;
        border-radius: 50%;
        background-color: var(--pad-text-color-light);
        display: inline-block;
        animation: dot-pulse 1.4s ease-in-out infinite;

        &.dot-2 {
          animation-delay: 0.2s;
        }

        &.dot-3 {
          animation-delay: 0.4s;
        }
      }
    }
  }

  .primary-btn {
    margin-top: 32px;
  }

  // 底部操作
  .footer-actions {
    margin-top: 32px;
    padding-top: 20px;
    border-top: 1px solid var(--pad-border-color-100);
    text-align: center;

    .reset-btn {
      background: none;
      border: none;
      color: var(--pad-text-color-400);
      font-size: 14px;
      cursor: pointer;
      padding: 8px 12px;
      border-radius: var(--pad-radius-md);
      transition: all var(--pad-transition-fast);

      &:hover:not(:disabled) {
        color: var(--pad-danger-color);
        background-color: var(--pad-bg-color-200);
        text-decoration: underline;
      }

      &:disabled {
        opacity: 0.5;
        cursor: not-allowed;
      }
    }
  }

  // 错误状态
  .error-section {
    text-align: center;
    padding: 40px 20px;

    .error-icon {
      font-size: 48px;
      margin-bottom: 20px;
    }

    .error-title {
      font-size: 20px;
      font-weight: 600;
      color: var(--pad-text-color-100);
      margin: 0 0 12px;
    }

    .error-message {
      font-size: 14px;
      color: var(--pad-text-color-300);
      margin: 0 0 24px;
    }
  }

  // 链接按钮样式
  .link-btn {
    color: var(--pad-primary-color);
    text-decoration: none;
    cursor: pointer;
    transition: color var(--pad-transition-fast);

    &:hover {
      color: var(--pad-primary-dark);
      text-decoration: underline;
    }
  }
}

// 动画定义
@keyframes container-enter {
  from {
    opacity: 0;
    transform: translateY(20px);
  }
  to {
    opacity: 1;
    transform: translateY(0);
  }
}

@keyframes spinner-rotate {
  from {
    transform: rotate(0deg);
  }
  to {
    transform: rotate(360deg);
  }
}

@keyframes dot-pulse {
  0%, 60%, 100% {
    transform: translateY(0);
    opacity: 0.6;
  }
  30% {
    transform: translateY(-6px);
    opacity: 1;
  }
}

// 响应式设计
@media (max-width: 512px) {
  #unlock {
    padding: 5% 0;

    .unlock-container {
      border-radius: 0;
      border: none;
    }

    .app-header {
      padding: 24px 20px 20px;

      .app-title {
        font-size: 28px;

        .app-logo {
          font-size: 32px;
        }
      }
    }

    .content-area {
      padding: 24px 20px;
    }

    .login-section,
    .config-section {
      .section-header {
        margin-bottom: 24px;

        .section-title {
          font-size: 22px;
        }
      }
    }

    .password-input,
    .config-input,
    .quick-config-input {
      padding: 10px 14px;
      font-size: 14px;
    }

    .submit-btn {
      padding: 12px 20px;
      font-size: 15px;
    }
  }
}

@media (min-width: 513px) and (max-width: 768px) {
  #unlock {
    padding: 8%;

    .unlock-container {
      max-width: 420px;
    }
  }
}
</style>
