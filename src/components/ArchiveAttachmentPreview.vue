<template>
  <div class="archive-preview">
    <div class="archive-toolbar">
      <div class="archive-summary">
        <q-chip dense square color="primary" text-color="white">
          {{ preview.format === 'zip' ? 'ZIP' : '7z' }}
        </q-chip>
        <span>{{ preview.fileCount }} 个文件</span>
        <span v-if="preview.directoryCount">{{ preview.directoryCount }} 个目录</span>
        <span>{{ formatBytes(preview.archiveSize) }}</span>
        <span>展开后 {{ formatBytes(preview.uncompressedSize) }}</span>
        <q-chip v-if="preview.isSolid" dense outline color="primary">固实压缩</q-chip>
        <q-chip v-if="preview.encrypted" dense outline color="primary" icon="lock">已加密</q-chip>
      </div>
      <q-input
        v-model="query"
        dense
        outlined
        clearable
        debounce="120"
        class="archive-search"
        placeholder="搜索压缩包内文件"
      >
        <template #prepend><q-icon name="search"/></template>
      </q-input>
    </div>
    <q-separator/>
    <div v-if="!visibleNodes.length" class="archive-empty">
      {{ query ? '没有匹配的文件' : '压缩包为空' }}
    </div>
    <q-virtual-scroll
      v-else
      class="archive-tree"
      :items="visibleNodes"
      :virtual-scroll-item-size="44"
      v-slot="{item}: {item: ArchiveTreeNode}"
    >
      <div
        :key="item.key"
        class="archive-row"
        :class="{'archive-row--directory': item.isDirectory}"
        :style="{'--archive-depth': item.depth}"
        @click="item.isDirectory && toggleDirectory(item.key)"
      >
        <q-icon
          class="archive-toggle"
          :name="item.isDirectory ? directoryToggleIcon(item.key) : ''"
          size="18px"
        />
        <q-icon
          :name="item.isDirectory ? 'folder' : fileIcon(item.path)"
          :color="item.isDirectory ? 'primary' : undefined"
          size="21px"
        />
        <div class="archive-name ellipsis" :title="item.path">{{ item.label }}</div>
        <span class="archive-lock-slot">
          <q-icon v-if="item.encrypted" name="lock" size="16px" color="primary">
            <q-tooltip>该条目已加密</q-tooltip>
          </q-icon>
        </span>
        <span
          class="archive-modified-at"
          :title="item.modifiedAt ? `修改时间：${item.modifiedAt}` : undefined"
        >{{ item.modifiedAt || '' }}</span>
        <span class="archive-size">{{ item.isDirectory ? '' : formatBytes(item.size) }}</span>
      </div>
    </q-virtual-scroll>
  </div>
</template>

<script setup lang="ts">
import {computed, ref, shallowRef, watch} from 'vue';
import type {ArchivePreview} from '../bindings';
import {formatBytes} from '../utils/format';
import {
  buildArchiveTree,
  initiallyExpandedDirectories,
  visibleArchiveNodes,
  type ArchiveTreeNode,
} from '../utils/archivePreview';

const props = defineProps<{preview: ArchivePreview}>();
const query = ref('');
const roots = shallowRef<ArchiveTreeNode[]>([]);
const expanded = ref<Set<string>>(new Set());

const visibleNodes = computed(() => (
  visibleArchiveNodes(roots.value, expanded.value, query.value || '')
));

watch(
  () => props.preview,
  (preview) => {
    roots.value = buildArchiveTree(preview.entries);
    expanded.value = initiallyExpandedDirectories(roots.value);
    query.value = '';
  },
  {immediate: true},
);

function toggleDirectory(key: string) {
  const next = new Set(expanded.value);
  if (next.has(key)) next.delete(key);
  else next.add(key);
  expanded.value = next;
}

function directoryToggleIcon(key: string): string {
  return query.value || expanded.value.has(key) ? 'expand_more' : 'chevron_right';
}

function fileIcon(path: string): string {
  const segments = path.split('.');
  const extension = segments[segments.length - 1]?.toLocaleLowerCase();
  if (['jpg', 'jpeg', 'png', 'gif', 'webp', 'avif', 'heic'].includes(extension || '')) {
    return 'image';
  }
  if (['mp3', 'm4a', 'wav', 'flac', 'aac', 'ogg'].includes(extension || '')) {
    return 'audio_file';
  }
  if (['mp4', 'mkv', 'mov', 'avi', 'webm'].includes(extension || '')) {
    return 'video_file';
  }
  if (extension === 'pdf') return 'picture_as_pdf';
  if (['zip', '7z', 'rar', 'tar', 'gz'].includes(extension || '')) return 'folder_zip';
  if (['txt', 'md', 'json', 'xml', 'yaml', 'yml'].includes(extension || '')) return 'description';
  return 'insert_drive_file';
}
</script>

<style scoped lang="scss">
.archive-preview {
  display: flex;
  flex: 1;
  min-height: 0;
  flex-direction: column;
  color: var(--pad-text-color-200);
  background: var(--pad-bg-color-100);
}

.archive-toolbar {
  display: grid;
  flex: none;
  grid-template-columns: minmax(0, 1fr) minmax(220px, 300px);
  align-items: center;
  gap: 16px;
  padding: 12px 16px;
}

.archive-summary {
  display: flex;
  min-width: 0;
  align-items: center;
  flex-wrap: wrap;
  gap: 10px;
  color: var(--pad-text-color-300);
  font-size: 13px;
}

.archive-search {
  width: 100%;
  color: var(--pad-text-color-100);
}

.archive-tree {
  flex: 1;
  min-height: 0;
  overflow: auto;
}

.archive-row {
  display: grid;
  height: 44px;
  grid-template-columns: 18px 21px minmax(80px, 1fr) 16px 152px 72px;
  align-items: center;
  column-gap: 9px;
  padding-right: 18px;
  padding-left: calc(12px + var(--archive-depth) * 22px);
  border-bottom: 1px solid color-mix(in srgb, var(--pad-border-color-100) 55%, transparent);
}

.archive-row--directory {
  cursor: pointer;
}

.archive-row--directory:hover {
  background: color-mix(in srgb, var(--q-primary) 8%, transparent);
}

.archive-toggle {
  flex: none;
  color: var(--pad-text-color-300);
}

.archive-name {
  min-width: 0;
  flex: 1;
}

.archive-lock-slot {
  width: 16px;
  height: 16px;
}

.archive-modified-at,
.archive-size {
  color: var(--pad-text-color-300);
  font-size: 12px;
  font-variant-numeric: tabular-nums;
  text-align: right;
  white-space: nowrap;
}

.archive-empty {
  display: flex;
  flex: 1;
  align-items: center;
  justify-content: center;
  color: var(--pad-text-color-300);
}

@media (max-width: 900px) {
  .archive-toolbar {
    grid-template-columns: 1fr;
    align-items: stretch;
    gap: 8px;
  }
}

@media (max-width: 500px) {
  .archive-row {
    grid-template-columns: 18px 21px minmax(48px, 1fr) 16px 88px 58px;
    column-gap: 7px;
    padding-right: 10px;
    padding-left: calc(8px + var(--archive-depth) * 16px);
  }

  .archive-modified-at {
    overflow: hidden;
    text-overflow: ellipsis;
  }
}
</style>
