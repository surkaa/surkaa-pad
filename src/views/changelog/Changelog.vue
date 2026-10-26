<script setup lang="ts">
import {computed, onBeforeUnmount, onMounted, ref} from 'vue';
import changelog from '../../data/changelog.json';
import DetailsHelpButton from '../../components/DetailsHelpButton.vue';
import {CHANGELOG_GROUP_LABELS, formatChangelogTimestamp, loadChangelogGroup, type ChangelogGroup} from '../../utils/changelog';
import {formatError} from '../../utils/formatError';

const selectedGroup = ref<ChangelogGroup | null>(null);
const records = computed(() => selectedGroup.value ? changelog[selectedGroup.value] : []);
const groupLabel = computed(() => selectedGroup.value ? CHANGELOG_GROUP_LABELS[selectedGroup.value] : '');
const loading = ref(true);
const error = ref('');
let active = true;
onBeforeUnmount(() => { active = false; });
onMounted(refresh);

async function refresh() {
  loading.value = true;
  error.value = '';
  try {
    const group = await loadChangelogGroup();
    if (active) selectedGroup.value = group;
  } catch (cause) {
    if (active) error.value = formatError(cause);
  } finally {
    if (active) loading.value = false;
  }
}
</script>

<template>
  <main class="changelog-page">
    <div class="changelog-toolbar">
      <div class="changelog-summary">
        <span aria-live="polite">{{ selectedGroup ? `${groupLabel} · ${records.length} 个版本` : '当前版本更新记录' }}</span>
        <DetailsHelpButton title="更新日志说明">
          <p>记录随应用提供，离线也能查看；正式版仅展示正式版记录，开发版仅展示当前平台的开发记录。</p>
          <p>历史时间来自仓库版本号变更提交，按当前设备的本地时间显示，不等同于安装包发布日期。</p>
          <p>开发版可能长期沿用同一版本号，不代表每次开发构建都有独立记录。</p>
        </DetailsHelpButton>
      </div>
    </div>

    <section class="changelog-scroll" tabindex="0" :aria-label="`${groupLabel}更新日志`" :aria-busy="loading">
      <div class="changelog-content">
        <div v-if="loading" class="changelog-state" role="status">
          <q-spinner size="24px" color="primary"/><span>正在读取应用版本…</span>
        </div>
        <div v-else-if="error" class="changelog-state" role="alert">
          <p>读取更新日志失败：{{ error }}</p>
          <q-btn flat color="primary" label="重试" @click="refresh"/>
        </div>
        <ol v-else class="changelog-list">
          <li v-for="(record, index) in records" :key="`${selectedGroup}:${record.version}`">
            <article class="changelog-entry" :data-version="record.version">
              <header class="entry-header">
                <h2 class="entry-version">
                  <span>v{{ record.version }}</span>
                  <span v-if="index === 0" class="entry-latest">最新记录</span>
                </h2>
                <time :datetime="new Date(record.updatedAt).toISOString()">{{ formatChangelogTimestamp(record.updatedAt) }}</time>
              </header>
              <p class="entry-description">{{ record.description }}</p>
            </article>
          </li>
        </ol>
      </div>
    </section>
  </main>
</template>

<style scoped lang="scss">
.changelog-page {
  height: 100%;
  min-height: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  background: var(--pad-bg-color-100);
  color: var(--pad-text-color-200);
  text-align: left;
}

.changelog-toolbar,
.changelog-content {
  width: 100%;
  max-width: 960px;
  margin: 0 auto;
}

.changelog-toolbar {
  flex-shrink: 0;
  padding: 20px 24px 0;
}

.changelog-summary {
  min-height: 40px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  font-size: 13px;
  color: var(--pad-text-color-400);
}

.changelog-scroll {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  overflow-x: hidden;
  scrollbar-gutter: stable;
  outline-offset: -3px;

  &:focus-visible { outline: 2px solid var(--pad-primary-color); }
}

.changelog-content { padding: 4px 24px 24px; }
.changelog-list { list-style: none; margin: 0; padding: 0; display: grid; gap: 12px; }
.changelog-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 12px;
  padding: 32px 0;
  text-align: center;
  color: var(--pad-text-color-300);
  overflow-wrap: anywhere;

  p { margin: 0; }
  &[role="alert"] { color: var(--pad-danger-color); }
}

.changelog-entry {
  padding: 18px 20px;
  border: 1px solid var(--pad-border-color-100);
  border-radius: var(--pad-radius-lg);
  background: var(--pad-bg-color-200);
}

.entry-header {
  display: flex;
  flex-wrap: wrap;
  justify-content: space-between;
  align-items: baseline;
  gap: 8px 16px;

  time { font-size: 12px; white-space: nowrap; color: var(--pad-text-color-400); }
}

.entry-version {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
  margin: 0;
  font-size: 18px;
  font-weight: 600;
  line-height: 1.5;
  color: var(--pad-text-color-100);
}

.entry-latest {
  padding: 2px 7px;
  border-radius: var(--pad-radius-sm);
  background: var(--pad-bg-color-300);
  color: var(--pad-text-color-300);
  font-size: 11px;
  font-weight: 400;
}

.entry-description {
  margin: 10px 0 0;
  font-size: 14px;
  line-height: 1.8;
  overflow-wrap: anywhere;
  white-space: pre-wrap;
  user-select: text;
}

@media (max-width: 600px) {
  .changelog-toolbar { padding: 12px 12px 0; }
  .changelog-content { padding: 4px 12px 20px; }
  .changelog-entry { padding: 14px; }
}
</style>
