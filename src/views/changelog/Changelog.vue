<script setup lang="ts">
import {computed, nextTick, ref} from 'vue';
import changelog from '../../data/changelog.json';
import DetailsHelpButton from '../../components/DetailsHelpButton.vue';
import {formatChangelogTimestamp} from '../../utils/changelog';

type ChangelogGroup = keyof typeof changelog;
const groups: {value: ChangelogGroup; label: string}[] = [
  {value: 'stable', label: '正式版'},
  {value: 'preview', label: '预览版'},
  {value: 'windowsDev', label: 'Windows Dev'},
  {value: 'androidDev', label: 'Android Dev'},
];
const selectedGroup = ref<ChangelogGroup>('stable');
const records = computed(() => changelog[selectedGroup.value]);
const groupLabel = computed(() => groups.find(group => group.value === selectedGroup.value)!.label);
const scrollContainer = ref<HTMLElement | null>(null);

async function selectGroup(group: ChangelogGroup) {
  if (selectedGroup.value === group) return;
  selectedGroup.value = group;
  await nextTick();
  if (scrollContainer.value) scrollContainer.value.scrollTop = 0;
}
</script>

<template>
  <main class="changelog-page">
    <div class="changelog-toolbar">
      <div class="changelog-groups" role="group" aria-label="更新日志版本类型">
        <q-btn v-for="group in groups" :key="group.value" flat no-caps
               class="changelog-group" :class="{'is-selected': selectedGroup === group.value}"
               :aria-label="group.label" :aria-pressed="selectedGroup === group.value"
               @click="selectGroup(group.value)">
          <span>{{ group.label }}</span>
          <span class="group-count">{{ changelog[group.value].length }}</span>
        </q-btn>
      </div>
      <div class="changelog-summary">
        <span aria-live="polite">{{ groupLabel }} · {{ records.length }} 个版本</span>
        <DetailsHelpButton title="更新日志说明">
          <p>记录随应用提供，离线也能查看；正式版、预览版和开发版的版本号分别管理。</p>
          <p>历史时间来自仓库版本号变更提交，按当前设备的本地时间显示，不等同于安装包发布日期。</p>
          <p>开发版可能长期沿用同一版本号，共享功能可查看正式版记录。</p>
        </DetailsHelpButton>
      </div>
    </div>

    <section ref="scrollContainer" class="changelog-scroll" tabindex="0" :aria-label="`${groupLabel}更新日志`">
      <div class="changelog-content">
        <ol class="changelog-list">
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

.changelog-groups {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 8px;
}

.changelog-group {
  min-height: 42px;
  border: 1px solid var(--pad-border-color-100);
  border-radius: var(--pad-radius-md);
  background: var(--pad-bg-color-200);
  color: var(--pad-text-color-200);

  .group-count {
    margin-left: 6px;
    font-size: 12px;
    font-variant-numeric: tabular-nums;
    color: var(--pad-text-color-400);
  }

  &.is-selected {
    border-color: var(--pad-primary-color);
    background: var(--pad-primary-color);
    color: var(--pad-on-primary-color);

    .group-count { color: inherit; }
  }
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
  .changelog-groups { grid-template-columns: repeat(2, minmax(0, 1fr)); }
  .changelog-entry { padding: 14px; }
}
</style>
