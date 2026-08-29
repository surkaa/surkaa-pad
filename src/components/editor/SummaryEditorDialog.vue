<script setup lang="ts">
import {computed} from 'vue';
import type {SummaryTarget} from './summarySelection';

const props = defineProps<{
  modelValue: boolean;
  mode: 'edit' | 'selection';
  summaryText: string;
  summaryContent: string;
  canDelete: boolean;
  targets: SummaryTarget[];
}>();

const emit = defineEmits<{
  (event: 'update:modelValue', value: boolean): void;
  (event: 'update:summaryText', value: string): void;
  (event: 'update:summaryContent', value: string): void;
  (event: 'createFromSelection'): void;
  (event: 'appendSelection', target: SummaryTarget): void;
  (event: 'save'): void;
  (event: 'delete'): void;
  (event: 'hide'): void;
}>();

const visible = computed({
  get: () => props.modelValue,
  set: value => emit('update:modelValue', value),
});
const editableSummary = computed({
  get: () => props.summaryText,
  set: value => emit('update:summaryText', value),
});
const editableContent = computed({
  get: () => props.summaryContent,
  set: value => emit('update:summaryContent', value),
});

function targetCaption(target: SummaryTarget): string {
  return target.content.trim().replace(/\s+/g, ' ') || '暂无内部文字';
}
</script>

<template>
  <q-dialog v-model="visible" no-refocus @hide="emit('hide')">
    <q-card class="summary-editor-dialog">
      <template v-if="mode === 'selection'">
        <q-card-section class="summary-dialog-header">
          <div class="text-h6 summary-dialog-title">处理选中文字</div>
          <div class="text-caption summary-dialog-description">
            创建新的折叠内容，或将文字移动到已有折叠内容中
          </div>
        </q-card-section>
        <q-list separator class="summary-target-list">
          <q-item clickable v-ripple @click="emit('createFromSelection')">
            <q-item-section avatar>
              <q-icon name="add" class="summary-target-icon"/>
            </q-item-section>
            <q-item-section>
              <q-item-label class="summary-dialog-title">创建新的折叠内容</q-item-label>
              <q-item-label caption class="summary-dialog-description">选中文字将作为内部文字</q-item-label>
            </q-item-section>
          </q-item>
          <q-item
            v-for="target in targets"
            :key="target.position"
            clickable
            v-ripple
            @click="emit('appendSelection', target)"
          >
            <q-item-section avatar>
              <q-icon name="unfold_more" class="summary-target-icon"/>
            </q-item-section>
            <q-item-section>
              <q-item-label class="summary-dialog-title">{{ target.summary }}</q-item-label>
              <q-item-label caption lines="1" class="summary-dialog-description">
                {{ targetCaption(target) }}
              </q-item-label>
            </q-item-section>
          </q-item>
        </q-list>
        <q-card-actions align="right" class="summary-dialog-actions">
          <q-btn flat label="取消" color="primary" v-close-popup/>
        </q-card-actions>
      </template>
      <template v-else>
        <q-card-section class="summary-dialog-header">
          <div class="text-h6 summary-dialog-title">
            {{ canDelete ? '编辑折叠内容' : '添加折叠内容' }}
          </div>
          <div class="text-caption summary-dialog-description">外显文字始终可见，内部文字可展开查看</div>
        </q-card-section>
        <q-card-section class="q-pt-none q-gutter-y-md summary-fields">
          <q-input
            v-model="editableSummary"
            outlined
            autofocus
            label="外显文字"
            maxlength="200"
            counter
            @keyup.enter="emit('save')"
          />
          <q-input
            v-model="editableContent"
            outlined
            type="textarea"
            autogrow
            label="内部文字"
          />
        </q-card-section>
        <q-card-actions align="right" class="summary-dialog-actions">
          <q-btn
            v-if="canDelete"
            flat
            label="删除"
            color="negative"
            @click="emit('delete')"
          />
          <q-space/>
          <q-btn flat label="取消" color="primary" v-close-popup/>
          <q-btn
            unelevated
            label="保存"
            color="primary"
            :disable="!editableSummary.trim()"
            @click="emit('save')"
          />
        </q-card-actions>
      </template>
    </q-card>
  </q-dialog>
</template>

<style scoped lang="scss">
.summary-editor-dialog {
  display: flex;
  flex-direction: column;
  width: min(520px, calc(100vw - 24px));
  max-height: 86vh;
  max-height: min(86dvh, calc(100dvh - 24px));
  overflow: hidden;
  color: var(--pad-text-color-100);
  background: var(--pad-bg-color-200);
  border-radius: var(--pad-radius-xl);
}

.summary-dialog-header,
.summary-dialog-actions {
  flex: 0 0 auto;
}

.summary-fields {
  min-height: 0;
  overflow-y: auto;
}

.summary-dialog-title {
  color: var(--pad-text-color-100);
}

.summary-dialog-description {
  color: var(--pad-text-color-400);
}

.summary-target-list {
  flex: 1 1 auto;
  min-height: 0;
  max-height: min(56vh, 420px);
  overflow-y: auto;
  background: var(--pad-bg-color-100);
}

.summary-target-icon {
  color: var(--pad-primary-dark);
}

:deep(.q-field__control) {
  background: var(--pad-bg-color-100);
}

</style>
