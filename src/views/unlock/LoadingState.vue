<template>
  <section class="state-section loading-state">
    <div class="loading-indicator">
      <div class="loading-spinner"></div>
      <p class="loading-text">正在加载配置...</p>
    </div>
  </section>
</template>

<style scoped lang="scss">
.state-section {
  width: 100%;
}

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

@keyframes spinner-rotate {
  from { transform: rotate(0deg); }
  to { transform: rotate(360deg); }
}
</style>