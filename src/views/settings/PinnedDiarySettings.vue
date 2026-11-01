<template>
  <q-item clickable v-ripple class="settings-item" @click="openDialog">
    <q-item-section avatar class="settings-icon-section">
      <q-icon name="push_pin"/>
    </q-item-section>
    <q-item-section>
      <q-item-label class="label-text text-weight-medium setting-label-with-hint">
        <span>置顶日记</span>
        <CloudSyncHint/>
      </q-item-label>
      <q-item-label caption class="desc-text">
        {{ pinnedSummary }}；请在日记详情中添加置顶
      </q-item-label>
    </q-item-section>
    <q-item-section side>
      <q-icon name="chevron_right" class="desc-text"/>
    </q-item-section>
  </q-item>

  <q-dialog v-model="showDialog" no-refocus>
    <q-card class="pinned-dialog">
      <q-card-section class="dialog-heading">
        <div>
          <div class="text-h6 title-text">置顶日记</div>
          <div class="text-caption desc-text">
            拖动把手或使用上下按钮调整顺序，也可取消置顶；添加置顶请前往日记详情菜单。
          </div>
        </div>
        <q-space/>
        <q-btn flat round dense icon="close" aria-label="关闭置顶日记弹窗" v-close-popup/>
      </q-card-section>

      <q-separator/>
      <q-linear-progress v-if="loading" indeterminate color="primary"/>

      <q-list v-if="pinnedDiaryIds.length > 0" ref="listRef" class="pinned-list">
        <TransitionGroup name="pinned-order" tag="div">
        <q-item v-for="(diaryId, index) in pinnedDiaryIds" :key="diaryId" :data-pinned-diary-id="diaryId" class="pinned-item"
                :class="{'is-dragging': draggedDiary === diaryId,
                  'drop-before': dropIndicator?.key === diaryId && dropIndicator.position === 'before',
                  'drop-after': dropIndicator?.key === diaryId && dropIndicator.position === 'after'}">
          <q-item-section avatar class="pinned-drag-section">
            <q-btn flat round dense icon="drag_indicator" color="primary" class="drag-handle" :ripple="false"
                   :disable="pinnedDiaryIds.length < 2" :aria-label="`拖动${diaryTitle(diaryId)}调整置顶顺序`"
                   title="拖动调整置顶顺序" @pointerdown="startDrag($event, diaryId)"/>
          </q-item-section>
          <q-item-section>
            <q-item-label class="diary-title">{{ diaryTitle(diaryId) }}</q-item-label>
            <q-item-label caption class="desc-text">ID：{{ diaryId }}</q-item-label>
          </q-item-section>
          <q-item-section side class="row-actions">
            <q-btn flat round dense icon="keyboard_arrow_up" color="primary" :disable="index === 0"
                   :aria-label="`上移${diaryTitle(diaryId)}`" @click="move(diaryId, -1)">
              <q-tooltip>上移</q-tooltip>
            </q-btn>
            <q-btn flat round dense icon="keyboard_arrow_down" color="primary" :disable="index === pinnedDiaryIds.length - 1"
                   :aria-label="`下移${diaryTitle(diaryId)}`" @click="move(diaryId, 1)">
              <q-tooltip>下移</q-tooltip>
            </q-btn>
            <q-btn
              flat
              round
              dense
              icon="remove_circle_outline"
              class="unpin-button"
              :aria-label="`取消置顶${diaryTitle(diaryId)}`"
              @click="unpinDiary(diaryId)"
            >
              <q-tooltip>取消置顶</q-tooltip>
            </q-btn>
          </q-item-section>
        </q-item>
        </TransitionGroup>
      </q-list>

      <q-card-section v-else class="empty-state">
        <q-icon name="push_pin" size="32px"/>
        <span>暂无置顶日记</span>
        <small>可在日记详情菜单中置顶</small>
      </q-card-section>

      <q-separator/>
      <q-card-actions align="right">
        <q-btn flat label="关闭" color="primary" v-close-popup/>
      </q-card-actions>
    </q-card>
  </q-dialog>
</template>

<script setup lang="ts">
import {computed, ref, type ComponentPublicInstance} from 'vue';
import type {DiarySummary} from '../../bindings';
import {useConfigStore} from '../../stores/config';
import {useDataStore} from '../../stores/data';
import api from '../../utils/api';
import CloudSyncHint from './CloudSyncHint.vue';
import {useListDragSort} from '../../composables/useListDragSort';
import {reorderPinnedDiaryIds} from '../../utils/pinnedDiaries';

const SUMMARY_LOAD_CONCURRENCY = 5;

const configStore = useConfigStore();
const dataStore = useDataStore();
const pinnedDiaryIds = configStore.useTauriConfig('pinned_diary_ids');
const summaries = ref<Record<string, DiarySummary | null>>({});
const showDialog = ref(false);
const loading = ref(false);
const listRef = ref<ComponentPublicInstance | null>(null);
const {draggedKey: draggedDiary, dropIndicator, startDrag} = useListDragSort({
  keys: () => pinnedDiaryIds.value,
  list: () => listRef.value?.$el instanceof HTMLElement ? listRef.value.$el : null,
  visible: () => showDialog.value,
  itemAttribute: 'data-pinned-diary-id',
  onDrop: reorder,
});

function reorder(diaryId: string, index: number) {
  const next = reorderPinnedDiaryIds(pinnedDiaryIds.value, diaryId, index);
  if (next.every((id, position) => id === pinnedDiaryIds.value[position])) return;
  pinnedDiaryIds.value = next;
}

function move(diaryId: string, direction: -1 | 1) {
  const index = pinnedDiaryIds.value.indexOf(diaryId);
  if (index >= 0) reorder(diaryId, index + direction);
}

const pinnedSummary = computed(() => pinnedDiaryIds.value.length > 0
  ? `${pinnedDiaryIds.value.length} 篇已置顶`
  : '暂无置顶日记');

async function openDialog() {
  showDialog.value = true;
  await loadMissingSummaries();
}

async function loadMissingSummaries() {
  const missingIds = pinnedDiaryIds.value.filter(id => summaries.value[id] === undefined);
  if (missingIds.length === 0) return;
  loading.value = true;
  try {
    for (let index = 0; index < missingIds.length; index += SUMMARY_LOAD_CONCURRENCY) {
      const batch = missingIds.slice(index, index + SUMMARY_LOAD_CONCURRENCY);
      await Promise.all(batch.map(async diaryId => {
        const cached = dataStore.diarySummaries[diaryId];
        if (cached) {
          summaries.value[diaryId] = cached;
          return;
        }
        try {
          summaries.value[diaryId] = await api.cmdGetDiarySummary(diaryId);
        } catch (error) {
          console.warn(`读取置顶日记 ${diaryId} 摘要失败:`, error);
          summaries.value[diaryId] = null;
        }
      }));
    }
  } finally {
    loading.value = false;
  }
}

function diaryTitle(diaryId: string): string {
  const summary = summaries.value[diaryId] ?? dataStore.diarySummaries[diaryId];
  if (summary?.title.trim()) return summary.title.trim();
  return summary === null ? '日记暂时无法读取' : '无标题日记';
}

function unpinDiary(diaryId: string) {
  pinnedDiaryIds.value = pinnedDiaryIds.value.filter(id => id !== diaryId);
  delete summaries.value[diaryId];
}
</script>

<style scoped lang="scss" src="./settingsSection.scss"></style>
<style scoped lang="scss">
.pinned-dialog {
  display: flex;
  flex-direction: column;
  width: min(620px, calc(100vw - 24px));
  max-height: min(720px, 88vh);
  color: var(--pad-text-color-100);
  background: var(--pad-bg-color-200);
  border-radius: var(--pad-radius-xl);
}

.dialog-heading {
  display: flex;
  align-items: flex-start;
  gap: 12px;
}

.title-text,
.diary-title {
  color: var(--pad-text-color-200);
}

.pinned-list {
  min-height: 0;
  overflow-y: auto;
}

.pinned-item {
  position: relative;
  min-height: 64px;
  transition: background-color 160ms ease, opacity 160ms ease;

  &:not(:last-child) {border-bottom: 1px solid var(--pad-border-color-100);}
  &.is-dragging {
    opacity: .72;
    background: color-mix(in srgb, var(--pad-primary-color) 14%, var(--pad-bg-color-100));
  }
  &.drop-before::before, &.drop-after::after {
    position: absolute; right: 12px; left: 12px; z-index: 2; height: 3px;
    border-radius: 999px; background: var(--pad-primary-color); content: ''; pointer-events: none;
  }
  &.drop-before::before {top: -2px;}
  &.drop-after::after {bottom: -2px;}
}

.unpin-button {
  color: var(--pad-primary-dark);
}

.row-actions {flex-direction: row; align-items: center; gap: 2px; padding-left: 8px;}
.pinned-drag-section {min-width: 40px; padding-right: 8px;}
.pinned-item .q-item__label {overflow-wrap: anywhere;}
.drag-handle {cursor: grab; touch-action: none; &:active {cursor: grabbing;}}
.pinned-order-move {transition: transform 180ms cubic-bezier(.2, 0, 0, 1);}
@media (prefers-reduced-motion: reduce) {.pinned-order-move {transition: none;}}

.empty-state {
  display: flex;
  flex: 1;
  min-height: 180px;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 6px;
  color: var(--pad-text-color-300);
}

.empty-state small {
  color: var(--pad-text-color-400);
}
</style>
