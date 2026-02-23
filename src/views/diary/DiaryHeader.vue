<script setup lang="ts">
const {title} = defineProps<{
  title?: string
}>();

const emit = defineEmits<{
  (e: 'back'): void;
  (e: 'info'): void;
  (e: 'operate'): void;
}>();
</script>

<template>
  <header class="custom-header">
    <button class="icon-btn" @click="emit('back')" aria-label="返回">
      <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none"
           stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
        <path d="m15 18-6-6 6-6"/>
      </svg>
    </button>

    <span class="header-title">{{ title || '新建' }}</span>

    <button class="icon-btn" @click="emit('info')" aria-label="详细信息">
      <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none"
           stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
        <circle cx="12" cy="12" r="10"/>
        <path d="M12 16v-4"/>
        <path d="M12 8h.01"/>
      </svg>
    </button>
    <button class="icon-btn" @click="emit('operate')" aria-label="操作">
      <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none"
           stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
        <circle cx="12" cy="12" r="1"/>
        <circle cx="19" cy="12" r="1"/>
        <circle cx="5" cy="12" r="1"/>
      </svg>
    </button>
  </header>
</template>

<style scoped lang="scss">
.custom-header {
  position: fixed;
  top: 0;
  left: 0;
  width: 100%;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 12px;
  background-color: var(--pad-bg-color-300);
  border-bottom: 1px solid var(--pad-border-color);
  box-shadow: 0 2px 10px var(--pad-shadow-color-200);
  box-sizing: border-box;

  .header-title {
    font-size: 1.1rem;
    font-weight: 600;
    color: var(--pad-text-color);
    margin: 0;
    flex: 1;
    text-align: center;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .icon-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 36px;
    height: 36px;
    border: none;
    border-radius: 8px;
    background: transparent;
    color: var(--pad-text-color-100);
    cursor: pointer;
    transition: all 0.2s ease;

    &:hover {
      background-color: var(--pad-bg-color-400);
      color: var(--pad-text-color);
    }

    &:active {
      transform: scale(0.92);
      background-color: var(--pad-bg-color-500);
    }

    svg {
      display: block;
    }
  }
}
</style>
