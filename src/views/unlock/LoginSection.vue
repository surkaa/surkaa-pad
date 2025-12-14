<template>
  <section class="state-section login-section">
    <div class="section-header">
      <h2 class="section-title">欢迎回来</h2>
    </div>

    <form @submit.prevent="$emit('unlock')" class="unlock-form">
      <div class="input-group">
        <input
            autofocus
            id="master-password"
            type="password"
            required
            placeholder="输入主密码"
            :value="masterPassword"
            @input="$emit('update:masterPassword', ($event.target as HTMLInputElement).value)"
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
            <path d="M18 8h-1V6c0-2.76-2.24-5-5-5S7 3.24 7 6v2H6c-1.1 0-2 .9-2 2v10c0 1.1.9 2 2 2h12c1.1 0 2-.9 2-2V10c0-1.1-.9-2-2-2zm-6 9c-1.1 0-2-.9-2-2s.9-2 2-2 2 .9 2 2-.9 2-2 2zm3.1-9H8.9V6c0-1.71 1.39-3.1 3.1-3.1 1.71 0 3.1 1.39 3.1 3.1v2z"/>
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
      <button class="reset-btn link-btn" @click="$emit('reset')" :disabled="loading">
        重置配置
      </button>
    </div>
  </section>
</template>

<script setup lang="ts">
defineProps<{
  masterPassword: string;
  loading: boolean;
}>();

defineEmits(['update:masterPassword', 'unlock', 'reset']);
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

.password-input {
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

@keyframes dot-pulse {
  0%, 60%, 100% { transform: translateY(0); opacity: 0.6; }
  30% { transform: translateY(-6px); opacity: 1; }
}

@media (max-width: 512px) {
  .section-header { margin-bottom: 24px; .section-title { font-size: 22px; } }
  .password-input { padding: 10px 14px; font-size: 14px; }
  .submit-btn { padding: 12px 20px; font-size: 15px; }
}
</style>