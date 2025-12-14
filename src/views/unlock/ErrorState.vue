<template>
  <section class="state-section error-section">
    <div class="error-icon">⚠️</div>
    <h2 class="error-title">发生了未知的错误</h2>
    <p class="error-message">请刷新页面重试或检查应用状态</p>
  </section>
</template>

<style scoped lang="scss">
.state-section {
  width: 100%;
}

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
</style>