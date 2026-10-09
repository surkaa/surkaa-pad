<template>
  <section class="settings-group settings-section-component">
    <div class="group-title">编辑器</div>
    <q-list bordered class="pad-card">
      <EditorFontSizeSettings/>
      <q-separator/>
      <q-item clickable v-ripple class="settings-item" @click="showDialog = true">
        <q-item-section avatar class="settings-icon-section">
          <q-icon name="tune"/>
        </q-item-section>
        <q-item-section>
          <q-item-label class="label-text text-weight-medium setting-label-with-hint">
            <span>工具栏按钮顺序</span>
            <CloudSyncHint/>
          </q-item-label>
          <q-item-label caption class="desc-text ellipsis order-summary">{{ orderSummary }}</q-item-label>
        </q-item-section>
        <q-item-section side>
          <q-icon name="chevron_right" class="desc-text"/>
        </q-item-section>
      </q-item>
    </q-list>

    <q-dialog v-model="showDialog">
      <q-card class="toolbar-order-dialog">
        <q-card-section>
          <div class="text-h6 title-text">工具栏按钮顺序</div>
          <div class="text-caption desc-text">拖动把手调整顺序，也可以使用上下按钮精确移动</div>
        </q-card-section>
        <q-separator/>

        <q-list ref="toolbarListRef" separator class="toolbar-order-list">
          <TransitionGroup name="toolbar-order" tag="div">
            <q-item
              v-for="(action, index) in toolbarOrder"
              :key="action"
              :data-toolbar-action="action"
              class="toolbar-order-item"
              :class="{'is-dragging': draggedAction === action}"
            >
              <q-item-section avatar>
                <q-icon :name="EDITOR_TOOLBAR_ICONS[action]" class="action-icon"/>
              </q-item-section>
              <q-item-section>
                <q-item-label class="label-text">{{ EDITOR_TOOLBAR_LABELS[action] }}</q-item-label>
              </q-item-section>
              <q-item-section side class="row-actions">
                <q-btn
                  flat
                  round
                  dense
                  icon="drag_indicator"
                  color="primary"
                  class="drag-handle"
                  :ripple="false"
                  :aria-label="`拖动${EDITOR_TOOLBAR_LABELS[action]}`"
                  :title="`拖动${EDITOR_TOOLBAR_LABELS[action]}调整顺序`"
                  @pointerdown="startDrag($event, action)"
                />
                <q-btn
                  flat
                  round
                  dense
                  icon="keyboard_arrow_up"
                  color="primary"
                  :disable="index === 0"
                  :aria-label="`上移${EDITOR_TOOLBAR_LABELS[action]}`"
                  @click="move(action, -1)"
                />
                <q-btn
                  flat
                  round
                  dense
                  icon="keyboard_arrow_down"
                  color="primary"
                  :disable="index === toolbarOrder.length - 1"
                  :aria-label="`下移${EDITOR_TOOLBAR_LABELS[action]}`"
                  @click="move(action, 1)"
                />
              </q-item-section>
            </q-item>
          </TransitionGroup>
        </q-list>

        <q-card-actions align="right">
          <q-btn flat label="恢复默认" color="primary" @click="resetOrder"/>
          <q-btn flat label="关闭" color="primary" v-close-popup/>
        </q-card-actions>
      </q-card>
    </q-dialog>
  </section>
</template>

<script setup lang="ts">
import {computed, onBeforeUnmount, ref, type ComponentPublicInstance} from 'vue';
import {useConfigStore} from '../../stores/config';
import {
  DEFAULT_EDITOR_TOOLBAR_ORDER,
  EDITOR_TOOLBAR_ICONS,
  EDITOR_TOOLBAR_LABELS,
  moveEditorToolbarAction,
  reorderEditorToolbarAction,
  type EditorToolbarAction,
} from '../../utils/editorToolbar';
import CloudSyncHint from './CloudSyncHint.vue';
import EditorFontSizeSettings from './EditorFontSizeSettings.vue';

const configStore = useConfigStore();
const showDialog = ref(false);
const toolbarOrder = configStore.useTauriConfig('editor_toolbar_order');
const toolbarListRef = ref<ComponentPublicInstance | null>(null);
const draggedAction = ref<EditorToolbarAction | null>(null);
let dragPointerId: number | null = null;
const orderSummary = computed(() => toolbarOrder.value
  .map(action => EDITOR_TOOLBAR_LABELS[action])
  .join(' · '));

function move(action: EditorToolbarAction, direction: -1 | 1) {
  toolbarOrder.value = moveEditorToolbarAction(toolbarOrder.value, action, direction);
}

function resetOrder() {
  toolbarOrder.value = [...DEFAULT_EDITOR_TOOLBAR_ORDER];
}

function toolbarListElement(): HTMLElement | null {
  return toolbarListRef.value?.$el instanceof HTMLElement
    ? toolbarListRef.value.$el
    : null;
}

function startDrag(event: PointerEvent, action: EditorToolbarAction) {
  if (dragPointerId !== null) return;
  event.preventDefault();
  dragPointerId = event.pointerId;
  draggedAction.value = action;
  document.body.classList.add('toolbar-order-dragging');
  document.addEventListener('pointermove', handleDragMove, {passive: false});
  document.addEventListener('pointerup', finishDrag);
  document.addEventListener('pointercancel', finishDrag);
}

function handleDragMove(event: PointerEvent) {
  if (event.pointerId !== dragPointerId || !draggedAction.value) return;
  event.preventDefault();

  const list = toolbarListElement();
  if (list) {
    const bounds = list.getBoundingClientRect();
    const edgeSize = 48;
    if (event.clientY < bounds.top + edgeSize) list.scrollTop -= 12;
    else if (event.clientY > bounds.bottom - edgeSize) list.scrollTop += 12;
  }

  const row = document
    .elementFromPoint(event.clientX, event.clientY)
    ?.closest<HTMLElement>('[data-toolbar-action]');
  const target = row?.dataset.toolbarAction as EditorToolbarAction | undefined;
  if (!target || !toolbarOrder.value.includes(target) || target === draggedAction.value) return;
  toolbarOrder.value = reorderEditorToolbarAction(
    toolbarOrder.value,
    draggedAction.value,
    target,
  );
}

function finishDrag(event?: PointerEvent) {
  if (event && event.pointerId !== dragPointerId) return;
  dragPointerId = null;
  draggedAction.value = null;
  document.body.classList.remove('toolbar-order-dragging');
  document.removeEventListener('pointermove', handleDragMove);
  document.removeEventListener('pointerup', finishDrag);
  document.removeEventListener('pointercancel', finishDrag);
}

onBeforeUnmount(() => finishDrag());
</script>

<style scoped lang="scss" src="./settingsSection.scss"></style>
<style scoped lang="scss">
.order-summary {
  max-width: min(72vw, 720px);
}

.toolbar-order-dialog {
  width: min(460px, calc(100vw - 24px));
  color: var(--pad-text-color-100);
  background: var(--pad-bg-color-200);
  border-radius: var(--pad-radius-xl);
}

.title-text {
  color: var(--pad-text-color-100);
}

.desc-text {
  color: var(--pad-text-color-400);
}

.toolbar-order-list {
  max-height: min(60vh, 520px);
  overflow-y: auto;
  background: var(--pad-bg-color-100);
}

.toolbar-order-item {
  min-height: 54px;
  transition: background-color 160ms ease, opacity 160ms ease;

  &:not(:last-child) {
    border-bottom: 1px solid var(--pad-border-color-100);
  }

  &.is-dragging {
    z-index: 1;
    opacity: 0.72;
    background: color-mix(in srgb, var(--pad-primary-color) 14%, var(--pad-bg-color-100));
  }
}

.toolbar-order-move {
  transition: transform 180ms cubic-bezier(0.2, 0, 0, 1);
}

.action-icon {
  color: var(--pad-primary-dark);
}

.row-actions {
  flex-direction: row;
  align-items: center;
  gap: 2px;
  padding-left: 8px;
}

.drag-handle {
  cursor: grab;
  touch-action: none;

  &:active {
    cursor: grabbing;
  }
}

:global(body.toolbar-order-dragging) {
  cursor: grabbing;
  user-select: none;
  -webkit-user-select: none;
}

@media (prefers-reduced-motion: reduce) {
  .toolbar-order-move {
    transition: none;
  }
}
</style>
