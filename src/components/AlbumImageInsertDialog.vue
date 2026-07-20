<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import type { AlbumSummary } from './editor/albumEditor'

const props = defineProps<{
  modelValue: boolean
  albums: AlbumSummary[]
}>()

const emit = defineEmits<{
  (e: 'update:modelValue', value: boolean): void
  (e: 'insert', albumId: string, insertionIndex: number): void
}>()

const step = ref<'album' | 'position'>('album')
const selectedAlbumId = ref('')
const selectedAlbum = computed(() =>
  props.albums.find(album => album.id === selectedAlbumId.value),
)

watch(() => props.modelValue, visible => {
  if (!visible) return
  if (props.albums.length === 1) {
    selectedAlbumId.value = props.albums[0].id
    step.value = 'position'
  } else {
    selectedAlbumId.value = ''
    step.value = 'album'
  }
})

function selectAlbum(albumId: string) {
  selectedAlbumId.value = albumId
  step.value = 'position'
}

function selectPosition(insertionIndex: number) {
  if (!selectedAlbum.value) return
  emit('insert', selectedAlbum.value.id, insertionIndex)
  emit('update:modelValue', false)
}
</script>

<template>
  <q-dialog
    :model-value="modelValue"
    @update:model-value="emit('update:modelValue', $event)"
  >
    <q-card class="album-insert-dialog">
      <q-card-section class="row items-center q-pb-sm">
        <div class="text-h6">
          {{ step === 'album' ? '选择目标图集' : '选择插入位置' }}
        </div>
        <q-space />
        <q-btn flat round dense icon="close" v-close-popup aria-label="关闭" />
      </q-card-section>

      <q-card-section v-if="step === 'album'" class="q-pt-none">
        <q-list bordered separator>
          <q-item
            v-for="(album, index) in albums"
            :key="album.id"
            clickable
            v-ripple
            @click="selectAlbum(album.id)"
          >
            <q-item-section avatar>
              <q-avatar rounded>
                <img v-if="album.urls[0]" :src="album.urls[0]" alt="" />
                <q-icon v-else name="collections" />
              </q-avatar>
            </q-item-section>
            <q-item-section>
              <q-item-label>图集 {{ index + 1 }}</q-item-label>
              <q-item-label caption>
                {{ album.images.length }} 张图片 ·
                {{ album.displayMode === 'stackedCards' ? '堆叠图集' : '横向图集' }}
              </q-item-label>
            </q-item-section>
            <q-item-section side>
              <q-icon name="chevron_right" />
            </q-item-section>
          </q-item>
        </q-list>
      </q-card-section>

      <q-card-section v-else-if="selectedAlbum" class="q-pt-none">
        <div class="text-caption text-grey-7 q-mb-md">
          点击图片之间的加号，将单图插入到该位置
        </div>
        <div class="album-position-list">
          <q-btn
            round
            unelevated
            color="primary"
            icon="add"
            aria-label="插入到图集开头"
            @click="selectPosition(0)"
          />
          <template v-for="(image, index) in selectedAlbum.images" :key="image">
            <div class="album-position-image">
              <img
                v-if="selectedAlbum.urls[index]"
                :src="selectedAlbum.urls[index]"
                :alt="image"
              />
              <q-icon v-else name="image" size="36px" />
              <div class="ellipsis">{{ image }}</div>
            </div>
            <q-btn
              round
              unelevated
              color="primary"
              icon="add"
              :aria-label="`插入到第 ${index + 1} 张图片之后`"
              @click="selectPosition(index + 1)"
            />
          </template>
        </div>
      </q-card-section>

      <q-card-actions align="right">
        <q-btn
          v-if="step === 'position' && albums.length > 1"
          flat
          label="返回选择图集"
          @click="step = 'album'"
        />
        <q-btn flat label="取消" v-close-popup />
      </q-card-actions>
    </q-card>
  </q-dialog>
</template>

<style scoped lang="scss">
.album-insert-dialog {
  width: min(92vw, 720px);
  max-width: 720px;
}

.album-position-list {
  display: flex;
  align-items: center;
  gap: 10px;
  overflow-x: auto;
  padding: 8px 4px 16px;
}

.album-position-list .q-btn {
  flex: 0 0 auto;
}

.album-position-image {
  flex: 0 0 128px;
  width: 128px;
  text-align: center;

  img {
    display: block;
    width: 128px;
    height: 128px;
    border-radius: 8px;
    object-fit: cover;
  }

  .q-icon {
    display: flex;
    width: 128px;
    height: 128px;
    align-items: center;
    justify-content: center;
    border-radius: 8px;
    background: var(--pad-bg-color-200);
  }

  div {
    margin-top: 4px;
    font-size: 12px;
  }
}
</style>
