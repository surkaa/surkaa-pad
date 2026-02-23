<script setup lang="ts">
defineEmits(['create']);
</script>

<template>
  <div class="empty-state">
    <div class="empty-content">
      <div class="empty-icon">
        📝
      </div>
      <h3 class="empty-title">
        <span>还没有日记</span>
      </h3>
      <p class="empty-message">
        <span>点击右下角按钮开始写第一篇日记</span>
      </p>
      <button
          class="empty-action"
          @click="$emit('create')"
      >
        开始写作
      </button>
    </div>
  </div>
</template>

<style scoped lang="scss">
.empty-state {
  display: flex;
  align-items: center;
  justify-content: center;
  min-height: 300px;
  text-align: center;
  padding: 40px 20px;

  .empty-content {
    max-width: 280px;

    .empty-icon {
      font-size: 48px;
      margin-bottom: 20px;
      opacity: 0.7;
    }

    .empty-title {
      font-size: 18px;
      font-weight: 600;
      color: var(--pad-text-color-100);
      margin: 0 0 12px;
    }

    .empty-message {
      font-size: 14px;
      color: var(--pad-text-color-300);
      margin: 0 0 24px;
      line-height: 1.5;
    }

    .empty-action {
      padding: 10px 24px;
      background-color: var(--pad-primary-color);
      color: var(--pad-text-color-light);
      border: none;
      border-radius: var(--pad-radius-lg);
      font-size: 14px;
      font-weight: 500;
      cursor: pointer;
      transition: all var(--pad-transition-fast);

      &:hover {
        background-color: var(--pad-primary-dark);
        transform: translateY(-1px);
      }
    }
  }
}
</style>
