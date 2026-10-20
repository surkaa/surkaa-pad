<script setup lang="ts">
import {computed, ref, watch} from 'vue';
import {useQuasar, type QList} from 'quasar';
import {moveDiaryBlock, type DiaryBlockDescriptor} from './blockOrder';
import {useListDragSort} from '../../composables/useListDragSort';

const props = defineProps<{
  modelValue: boolean;
  blocks: DiaryBlockDescriptor[];
}>();

const emit = defineEmits<{
  (event: 'update:modelValue', value: boolean): void;
  (event: 'confirm', order: number[]): void;
}>();

const $q = useQuasar();
const draft = ref<DiaryBlockDescriptor[]>([]);
const blockListRef = ref<QList | null>(null);
const {draggedKey: draggedBlock, dropIndicator, startDrag} = useListDragSort({
  keys: () => draft.value.map(block => block.sourceIndex),
  visible: () => props.modelValue,
  list: () => blockListRef.value?.$el instanceof HTMLElement ? blockListRef.value.$el : null,
  itemAttribute: 'data-block-index',
  onDrop: (sourceIndex, index) => move(draft.value.findIndex(block => block.sourceIndex === sourceIndex), index),
});
const visible = computed({
  get: () => props.modelValue,
  set: value => emit('update:modelValue', value),
});
const hasChanges = computed(() => draft.value.some(
  (block, index) => block.sourceIndex !== props.blocks[index]?.sourceIndex,
));

watch(() => props.modelValue, value => {
  if (value) resetOrder();
}, {immediate: true});

function resetOrder() {
  draft.value = props.blocks.map(block => ({...block}));
}

function move(fromIndex: number, toIndex: number) {
  if (draggedBlock.value !== null) return;
  draft.value = moveDiaryBlock(draft.value, fromIndex, toIndex);
}

function requestPosition(index: number) {
  $q.dialog({
    class: 'block-order-position-dialog',
    title: '移动到指定位置',
    message: `请输入 1–${draft.value.length} 之间的位置`,
    prompt: {
      model: String(index + 1),
      type: 'number',
      min: 1,
      max: draft.value.length,
      isValid: value => {
        const position = Number(value);
        return Number.isInteger(position) && position >= 1 && position <= draft.value.length;
      },
    },
    cancel: true,
    persistent: true,
    color: 'primary',
  }).onOk(value => move(index, Number(value) - 1));
}

function confirmOrder() {
  if (draggedBlock.value !== null) return;
  emit('confirm', draft.value.map(block => block.sourceIndex));
}
</script>

<template>
  <q-dialog v-model="visible" no-refocus :maximized="$q.screen.lt.sm">
    <q-card class="block-order-dialog column no-wrap">
      <q-card-section class="block-order-header">
        <div class="text-h6 block-order-title">调整内容顺序</div>
        <div class="text-caption block-order-description">
          共 {{ draft.length }} 个内容块；拖动把手调整顺序，确认后可一次撤销
        </div>
      </q-card-section>
      <q-separator/>

      <q-list ref="blockListRef" separator class="block-order-list">
        <TransitionGroup name="block-order" tag="div">
          <q-item
            v-for="(block, index) in draft"
            :key="block.sourceIndex"
            :data-block-index="block.sourceIndex"
            class="block-order-item"
            :class="{
              'is-dragging': draggedBlock === block.sourceIndex,
              'drop-before': dropIndicator?.key === block.sourceIndex && dropIndicator.position === 'before',
              'drop-after': dropIndicator?.key === block.sourceIndex && dropIndicator.position === 'after',
            }"
          >
            <q-item-section avatar class="block-order-leading">
              <div class="block-order-index">{{ index + 1 }}</div>
              <q-icon :name="block.icon" class="block-order-icon"/>
            </q-item-section>
            <q-item-section class="block-order-content">
              <q-item-label class="block-order-title">{{ block.title }}</q-item-label>
              <q-item-label caption lines="2" class="block-order-preview">
                {{ block.preview }}
              </q-item-label>
            </q-item-section>
            <q-item-section side class="block-order-actions">
              <q-btn
                flat
                round
                dense
                icon="drag_indicator"
                color="primary"
                class="block-order-drag-handle"
                :ripple="false"
                :aria-label="`拖动第 ${index + 1} 个内容块`"
                :disable="draft.length < 2"
                @pointerdown="startDrag($event, block.sourceIndex)"
                @contextmenu.prevent
              />
              <q-btn
                flat
                round
                dense
                icon="keyboard_arrow_up"
                color="primary"
                :disable="index === 0 || draggedBlock !== null"
                :aria-label="`上移第 ${index + 1} 个内容块`"
                @click="move(index, index - 1)"
              />
              <q-btn
                flat
                round
                dense
                icon="keyboard_arrow_down"
                color="primary"
                :disable="index === draft.length - 1 || draggedBlock !== null"
                :aria-label="`下移第 ${index + 1} 个内容块`"
                @click="move(index, index + 1)"
              />
              <q-btn
                flat
                round
                dense
                icon="more_vert"
                color="primary"
                :aria-label="`移动第 ${index + 1} 个内容块到其他位置`"
                :disable="draggedBlock !== null"
              >
                <q-menu class="block-order-menu">
                  <q-list dense>
                    <q-item clickable v-close-popup :disable="index === 0" @click="move(index, 0)">
                      <q-item-section avatar><q-icon name="vertical_align_top"/></q-item-section>
                      <q-item-section>移到最前</q-item-section>
                    </q-item>
                    <q-item clickable v-close-popup @click="requestPosition(index)">
                      <q-item-section avatar><q-icon name="format_list_numbered"/></q-item-section>
                      <q-item-section>移到指定位置</q-item-section>
                    </q-item>
                    <q-item
                      clickable
                      v-close-popup
                      :disable="index === draft.length - 1"
                      @click="move(index, draft.length - 1)"
                    >
                      <q-item-section avatar><q-icon name="vertical_align_bottom"/></q-item-section>
                      <q-item-section>移到最后</q-item-section>
                    </q-item>
                  </q-list>
                </q-menu>
              </q-btn>
            </q-item-section>
          </q-item>
        </TransitionGroup>
      </q-list>

      <q-separator/>
      <q-card-actions align="right" class="block-order-footer">
        <q-btn flat label="恢复原顺序" color="primary" :disable="!hasChanges || draggedBlock !== null" @click="resetOrder"/>
        <q-space/>
        <q-btn flat label="取消" color="primary" v-close-popup/>
        <q-btn
          unelevated
          label="确认调整"
          color="primary"
          :disable="!hasChanges || draggedBlock !== null"
          @click="confirmOrder"
        />
      </q-card-actions>
    </q-card>
  </q-dialog>
</template>

<style scoped lang="scss">
.block-order-dialog {
  width: min(680px, calc(100vw - 24px));
  max-height: min(88dvh, 760px);
  overflow: hidden;
  color: var(--pad-text-color-100);
  background: var(--pad-bg-color-200);
  border-radius: var(--pad-radius-xl);
}

.block-order-header,
.block-order-footer {
  flex: 0 0 auto;
}

.block-order-title {
  min-width: 0;
  color: var(--pad-text-color-100);
}

.block-order-description,
.block-order-preview {
  color: var(--pad-text-color-400);
}

.block-order-list {
  flex: 1 1 auto;
  min-height: 0;
  overflow-y: auto;
  background: var(--pad-bg-color-100);
}

.block-order-item {
  position: relative;
  min-height: 72px;
  border-color: var(--pad-border-color-100);
  transition: background-color 160ms ease, opacity 160ms ease;
}

.block-order-item.is-dragging {
  opacity: 0.72;
  background: color-mix(in srgb, var(--pad-primary-color) 14%, var(--pad-bg-color-100));
}

.block-order-item.drop-before::before,
.block-order-item.drop-after::after {
  position: absolute;
  right: 12px;
  left: 12px;
  z-index: 1;
  height: 3px;
  border-radius: var(--pad-radius-full);
  background: var(--pad-primary-color);
  content: '';
  pointer-events: none;
}

.block-order-item.drop-before::before { top: -2px; }
.block-order-item.drop-after::after { bottom: -2px; }

.block-order-drag-handle {
  cursor: grab;
  touch-action: none;
  user-select: none;
}
.block-order-drag-handle:active { cursor: grabbing; }

.block-order-leading {
  position: relative;
  flex-direction: row;
  align-items: center;
  gap: 8px;
  min-width: 72px;
  padding-right: 12px;
}

.block-order-index {
  display: grid;
  flex: none;
  place-items: center;
  width: 24px;
  height: 24px;
  border-radius: 8px;
  color: var(--pad-text-color-300);
  background: var(--pad-bg-color-300);
  font-size: 12px;
  font-variant-numeric: tabular-nums;
}

.block-order-icon {
  flex: none;
  color: var(--pad-primary-dark);
  font-size: 23px;
}

.block-order-content {
  min-width: 0;
}

.block-order-preview {
  overflow-wrap: anywhere;
}

.block-order-actions {
  flex-direction: row;
  align-items: center;
  gap: 1px;
  padding-left: 8px;
}

.block-order-move {
  transition: transform 180ms cubic-bezier(0.2, 0, 0, 1);
}

:global(.block-order-menu) {
  color: var(--pad-text-color-200);
  background: var(--pad-bg-color-200);
}

@media (max-width: 599px) {
  .block-order-dialog {
    width: 100%;
    max-height: 100dvh;
    border-radius: 0;
  }

  .block-order-header {
    padding-top: max(16px, env(safe-area-inset-top));
  }

  .block-order-item {
    padding-right: 8px;
    padding-left: 10px;
  }

  .block-order-leading {
    gap: 6px;
    min-width: 62px;
    padding-right: 8px;
  }

  .block-order-actions {
    padding-left: 2px;
  }

  .block-order-footer {
    padding-bottom: max(8px, env(safe-area-inset-bottom));
  }
}

@media (prefers-reduced-motion: reduce) {
  .block-order-move {
    transition: none;
  }
}
</style>

<style lang="scss">
.block-order-position-dialog {
  .q-card {
    color: var(--pad-text-color-100);
    background: var(--pad-bg-color-200);
  }

  .q-field__control {
    background: var(--pad-bg-color-100);
  }

}
</style>
