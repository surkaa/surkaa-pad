<template>
  <span
    class="cloud-sync-hint"
    role="note"
    title="开启云同步后，此设置会自动同步到其他设备"
    aria-label="此设置随云同步"
  >
    <q-icon name="cloud_done" size="13px"/>
    <span>随云同步</span>
  </span>
</template>

<style scoped lang="scss">
.cloud-sync-hint {
  display: inline-flex;
  align-items: center;
  gap: 3px;
  flex: none;
  padding: 1px 6px;
  border: 1px solid color-mix(in srgb, var(--pad-primary-color) 26%, transparent);
  border-radius: 999px;
  color: var(--pad-primary-dark);
  background: color-mix(in srgb, var(--pad-primary-color) 10%, transparent);
  font-size: 0.68rem;
  font-weight: 500;
  line-height: 1.45;
  letter-spacing: normal;
  white-space: nowrap;
  vertical-align: middle;
}
</style>
