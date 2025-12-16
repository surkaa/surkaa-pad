<script lang="ts" setup>
const {
  version,
  appName
} = defineProps<{
  version: string;
  appName: string;
}>();
</script>

<template>
  <div class="app-header">
    <h1 class="app-title">
      <img alt="app-logo" class="app-logo" src="/app-icon.png"/>
      <span class="app-name" v-text="appName"/>
      <span class="version" v-text="version"/>
    </h1>
  </div>
</template>

<style scoped lang="scss">
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

    .app-name {
      font-size: 28px;
      font-weight: 600;
    }

    .version {
      font-size: 16px;
      font-weight: 400;
      margin-left: 12px;
      color: var(--pad-text-color-light);
    }
  }
}

@media (max-width: 512px) {
  .app-header {
    padding: 24px 20px 20px;

    .app-title {
      font-size: 28px;

      .app-logo {
        font-size: 32px;
      }
    }
  }
}
</style>