<template>
  <q-item clickable v-ripple class="settings-item" @click="showDialog = true">
    <q-item-section avatar class="settings-icon-section">
      <q-icon name="format_size"/>
    </q-item-section>
    <q-item-section>
      <q-item-label class="label-text text-weight-medium">编辑器字号</q-item-label>
      <q-item-label caption class="desc-text ellipsis">{{ sizeSummary }} · 仅当前设备</q-item-label>
    </q-item-section>
    <q-item-section side>
      <q-icon name="chevron_right" class="desc-text"/>
    </q-item-section>
  </q-item>

  <q-dialog v-model="showDialog">
    <q-card class="font-size-dialog">
      <q-card-section class="font-size-header">
        <div class="text-h6 title-text">编辑器字号</div>
        <div class="text-caption desc-text">分别调整正文与各级标题；配置只保存在当前设备</div>
      </q-card-section>
      <q-separator/>

      <q-card-section class="font-size-content">
        <div class="font-size-controls">
          <div v-for="key in EDITOR_FONT_SIZE_KEYS" :key="key" class="font-size-row">
            <span class="font-size-label label-text">{{ EDITOR_FONT_SIZE_LABELS[key] }}</span>
            <span class="font-size-value">{{ fontSizes[key] }} px</span>
            <q-slider
              class="font-size-slider"
              :model-value="fontSizes[key]"
              :min="MIN_EDITOR_FONT_SIZE"
              :max="MAX_EDITOR_FONT_SIZE"
              :step="1"
              color="primary"
              label
              @update:model-value="value => updateSize(key, value)"
            />
          </div>
        </div>

        <div class="font-preview" :style="previewStyle">
          <div class="font-preview__title">实时预览</div>
          <h1>一级标题预览</h1>
          <h2>二级标题预览</h2>
          <h3>三级标题预览</h3>
          <h4>四级标题预览</h4>
          <h5>五级标题预览</h5>
          <p>这是正文预览，用于观察日记编辑页面的实际字号效果。</p>
        </div>
      </q-card-section>

      <q-separator/>
      <q-card-actions align="right" class="font-size-actions">
        <q-btn flat label="恢复默认" color="primary" @click="resetSizes"/>
        <q-btn flat label="关闭" color="primary" v-close-popup/>
      </q-card-actions>
    </q-card>
  </q-dialog>
</template>

<script setup lang="ts">
import {computed, ref} from 'vue';
import {useConfigStore} from '../../stores/config';
import {
  DEFAULT_EDITOR_FONT_SIZES,
  EDITOR_FONT_SIZE_KEYS,
  EDITOR_FONT_SIZE_LABELS,
  MAX_EDITOR_FONT_SIZE,
  MIN_EDITOR_FONT_SIZE,
  editorFontSizeCssVariables,
  normalizeEditorFontSizeConfig,
  type EditorFontSizeKey,
} from '../../utils/editorFontSizes';

const configStore = useConfigStore();
const showDialog = ref(false);
const fontSizes = configStore.useTauriConfig('editor_font_sizes');
const previewStyle = computed(() => editorFontSizeCssVariables(fontSizes.value));
const sizeSummary = computed(() => [
  `正文 ${fontSizes.value.body}px`,
  `H1 ${fontSizes.value.heading1}px`,
  `H2 ${fontSizes.value.heading2}px`,
  `H3 ${fontSizes.value.heading3}px`,
  `H4 ${fontSizes.value.heading4}px`,
  `H5 ${fontSizes.value.heading5}px`,
].join(' · '));

function updateSize(key: EditorFontSizeKey, value: number | null) {
  fontSizes.value = normalizeEditorFontSizeConfig({
    ...fontSizes.value,
    [key]: value,
  });
}

function resetSizes() {
  fontSizes.value = {...DEFAULT_EDITOR_FONT_SIZES};
}
</script>

<style scoped lang="scss" src="./settingsSection.scss"></style>
<style scoped lang="scss">
.font-size-dialog {
  display: flex;
  flex-direction: column;
  width: min(760px, calc(100vw - 48px));
  max-width: none;
  max-height: min(720px, calc(100vh - 48px));
  max-height: min(720px, calc(100dvh - 48px));
  overflow: hidden;
  color: var(--pad-text-color-100);
  background: var(--pad-bg-color-200);
  border-radius: var(--pad-radius-xl);
}

.font-size-header,
.font-size-actions,
.font-size-dialog > .q-separator {
  flex: 0 0 auto;
}

.font-size-actions {
  padding: 12px 16px;
}

.title-text {
  color: var(--pad-text-color-100);
}

.font-size-content {
  display: grid;
  flex: 1 1 auto;
  min-height: 0;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  align-content: start;
  align-items: start;
  gap: 22px;
  overflow-y: auto;
  overscroll-behavior: contain;
}

.font-size-controls {
  display: flex;
  min-width: 0;
  flex-direction: column;
  gap: 4px;
}

.font-size-row {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  align-items: center;
  column-gap: 12px;
}

.font-size-slider {
  grid-column: 1 / -1;
}

.font-size-value {
  color: var(--pad-primary-dark);
  font-variant-numeric: tabular-nums;
  white-space: nowrap;
}

.font-preview {
  align-self: start;
  min-width: 0;
  padding: 16px;
  border: 1px solid var(--pad-border-color-100);
  border-radius: 12px;
  color: var(--pad-text-color-100);
  background: var(--pad-bg-color-100);
  text-align: left;
  overflow-wrap: anywhere;

  .font-preview__title {
    margin-bottom: 12px;
    color: var(--pad-text-color-400);
    font-size: 12px;
  }

  h1,
  h2,
  h3,
  h4,
  h5,
  p {
    margin: 0 0 0.45em;
    line-height: 1.25;
  }

  h1 { font-size: var(--diary-heading-1-font-size); }
  h2 { font-size: var(--diary-heading-2-font-size); }
  h3 { font-size: var(--diary-heading-3-font-size); }
  h4 { font-size: var(--diary-heading-4-font-size); }
  h5 { font-size: var(--diary-heading-5-font-size); }
  p {
    color: var(--pad-text-color-300);
    font-size: var(--diary-body-font-size);
    line-height: 1.55;
    margin-bottom: 0;
  }
}

@media (max-width: 640px) {
  .font-size-content {
    grid-template-columns: 1fr;
    gap: 16px;
  }

  .font-size-row {
    grid-template-columns: 64px minmax(0, 1fr) 44px;
    min-height: 48px;
    column-gap: 10px;
  }

  .font-size-label {
    grid-column: 1;
    grid-row: 1;
  }

  .font-size-slider {
    grid-column: 2;
    grid-row: 1;
  }

  .font-size-value {
    grid-column: 3;
    grid-row: 1;
    font-size: 13px;
    text-align: right;
  }
}
</style>
