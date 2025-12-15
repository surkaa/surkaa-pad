<template>
  <section class="state-section config-section">
    <div class="section-header">
      <h2 class="section-title">首次配置</h2>
    </div>

    <form @submit.prevent="$emit('save')" class="config-form">
      <div class="input-group">
        <input
            id="master-password"
            type="password"
            required
            placeholder="设置主密码"
            :value="masterPassword"
            @input="$emit('update:masterPassword', ($event.target as HTMLInputElement).value)"
            class="config-input"
            :disabled="loading"
        />
      </div>

      <div class="config-toggle">
        <button
            type="button"
            class="toggle-btn"
            @click="$emit('update:showQuickInput', !showQuickInput)"
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
              :value="quickConfig"
              @input="$emit('update:quickConfig', ($event.target as HTMLTextAreaElement).value)"
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
            <path d="M17 3H5c-1.11 0-2 .9-2 2v14c0 1.1.89 2 2 2h14c1.1 0 2-.9 2-2V7l-4-4zm-5 16c-1.66 0-3-1.34-3-3s1.34-3 3-3 3 1.34 3 3-1.34 3-3 3zm3-10H5V5h10v4z"/>
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
</template>

<script setup lang="ts">
import { OssConfigType } from "../../types";

defineProps<{
  masterPassword: string;
  ossConfig: OssConfigType;
  quickConfig: string;
  showQuickInput: boolean;
  loading: boolean;
}>();

defineEmits(['update:masterPassword', 'update:quickConfig', 'update:showQuickInput', 'save']);
</script>

<style scoped lang="scss">
.state-section {
  width: 100%;
}

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

.input-group {
  margin-bottom: 20px;
  &:last-child { margin-bottom: 0; }
}

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

  &::placeholder { color: var(--pad-text-color-400); }
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
    &:active { transform: scale(0.98); }
    .toggle-icon { display: flex; align-items: center; svg { fill: currentColor; } }
  }
}

.oss-config-group { margin-bottom: 24px; }

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
  &:active:not(:disabled) { transform: translateY(0); }
  &:disabled {
    opacity: 0.7;
    cursor: not-allowed;
    transform: none;
    box-shadow: none;
  }
  &.loading .btn-text { opacity: 0.8; }

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
      &.dot-2 { animation-delay: 0.2s; }
      &.dot-3 { animation-delay: 0.4s; }
    }
  }
}

.primary-btn { margin-top: 32px; }

@keyframes dot-pulse {
  0%, 60%, 100% { transform: translateY(0); opacity: 0.6; }
  30% { transform: translateY(-6px); opacity: 1; }
}

@media (max-width: 512px) {
  .section-header { margin-bottom: 24px; .section-title { font-size: 22px; } }
  .config-input, .quick-config-input { padding: 10px 14px; font-size: 14px; }
  .submit-btn { padding: 12px 20px; font-size: 15px; }
}
</style>