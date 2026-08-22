<template>
  <section class="settings-group settings-section-component">
    <div class="group-title">编辑器</div>
    <q-list bordered class="pad-card">
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
          <div class="text-caption desc-text">撤销和重做固定在末尾，不参与排序</div>
        </q-card-section>
        <q-separator/>

        <q-list separator class="toolbar-order-list">
          <TransitionGroup name="toolbar-order" tag="div">
            <q-item v-for="(action, index) in toolbarOrder" :key="action" class="toolbar-order-item">
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
import {computed, ref} from 'vue';
import {useConfigStore} from '../../stores/config';
import {
  DEFAULT_EDITOR_TOOLBAR_ORDER,
  EDITOR_TOOLBAR_ICONS,
  EDITOR_TOOLBAR_LABELS,
  moveEditorToolbarAction,
  type EditorToolbarAction,
} from '../../utils/editorToolbar';
import CloudSyncHint from './CloudSyncHint.vue';

const configStore = useConfigStore();
const showDialog = ref(false);
const toolbarOrder = configStore.useTauriConfig('editor_toolbar_order');
const orderSummary = computed(() => toolbarOrder.value
  .map(action => EDITOR_TOOLBAR_LABELS[action])
  .join(' · '));

function move(action: EditorToolbarAction, direction: -1 | 1) {
  toolbarOrder.value = moveEditorToolbarAction(toolbarOrder.value, action, direction);
}

function resetOrder() {
  toolbarOrder.value = [...DEFAULT_EDITOR_TOOLBAR_ORDER];
}
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

  &:not(:last-child) {
    border-bottom: 1px solid var(--pad-border-color-100);
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

@media (prefers-reduced-motion: reduce) {
  .toolbar-order-move {
    transition: none;
  }
}
</style>
