<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch, nextTick } from 'vue'
import { useEditor, EditorContent } from '@tiptap/vue-3'
import StarterKit from '@tiptap/starter-kit'
import Placeholder from '@tiptap/extension-placeholder'
import { useScroll, useStorage } from '@vueuse/core'
import { useQuasar } from 'quasar'
import { platform } from '@tauri-apps/plugin-os'
import { Menu, MenuItem } from '@tauri-apps/api/menu'
import type { DiaryContent, DiarySummary } from '../bindings'
import { diaryContentToHtml, htmlToDiaryContent } from './editor/markdownConverter'
import {
  shouldFocusEditorEnd,
  shouldPreventStackedAlbumEditorFocus,
} from './editor/editorClick'
import {
  addImageToAlbumDocument,
  changeAlbumDisplayMode,
  createAlbumDocument,
  listAlbums,
  splitAlbumDocument,
  type AlbumSplitOperation,
  type AlbumSummary,
} from './editor/albumEditor'
import { animateStackedAlbumCycle } from './editor/albumAnimation'
import { setupEditorImageLoading } from './editor/imageLoading'
import { ImageNode, VideoNode, AudioNode, FileNode, AlbumNode } from './editor/tiptap-extensions'
import AlbumImageInsertDialog from './AlbumImageInsertDialog.vue'
import {
  attachmentInsertionsToEditorContent,
  type AttachmentInsertion,
} from '../utils/attachmentInsertion'

const props = defineProps<{
  modelValue: DiaryContent
  diarySummary?: DiarySummary
  attachmentMap: Record<string, string>
}>()

const emit = defineEmits<{
  (e: 'update:modelValue', value: DiaryContent): void
  (e: 'pasteAttachments', files: File[]): void
  (e: 'showImage', src: string): void
  (e: 'toggleAttachmentEncryption', attachmentId: string): void
  (e: 'rotateAttachment', attachmentId: string, rotation: number): void
  (e: 'renameAttachment', attachmentId: string, filename: string, cb: (newFilename: string) => void): void
  (e: 'saveDecryptAttachment', attachmentId: string): void
  (e: 'editorFocused'): void
}>()

const $q = useQuasar()
const editorElement = ref<HTMLDivElement>()
const currentPlatform = platform()
const albumSelection = ref<string[]>([])
const albumAnchor = ref('')
const showAlbumInsertDialog = ref(false)
const albumInsertSource = ref('')
const albumInsertTargets = ref<AlbumSummary[]>([])

const storageY = useStorage(`scroll-y-${props.diarySummary?.id}`, 0, sessionStorage)
const { y } = useScroll(editorElement, {
  behavior: 'smooth',
  onScroll() {
    storageY.value = y.value
  },
})

function getAttachmentMeta(attachmentId: string) {
  return props.diarySummary?.attachments.find(attachment => attachment.id === attachmentId) || null
}

const attachmentFilenames = computed<Record<string, string>>(() => Object.fromEntries(
  (props.diarySummary?.attachments || []).map(attachment => [attachment.id, attachment.filename]),
))

const editor = useEditor({
  content: diaryContentToHtml(props.modelValue, props.attachmentMap, attachmentFilenames.value),
  extensions: [
    StarterKit.configure({
      heading: { levels: [1, 2, 3] },
    }),
    Placeholder.configure({
      placeholder: '开始记录...',
    }),
    ImageNode,
    VideoNode,
    AudioNode,
    FileNode,
    AlbumNode,
  ],
  editorProps: {
    attributes: {
      class: 'tiptap-editor',
    },
    handlePaste: (_view, event) => {
      const files = event.clipboardData?.files
      if (files && files.length > 0) {
        event.preventDefault()
        emit('pasteAttachments', Array.from(files))
        return true
      }
      return false
    },
  },
  onUpdate({ editor: ed }) {
    const html = ed.getHTML()
    emit('update:modelValue', htmlToDiaryContent(html))
  },
  onFocus() {
    emit('editorFocused')
  },
})

watch(() => props.modelValue, (newVal) => {
  if (!editor.value || newVal === undefined) return
  const currentContent = htmlToDiaryContent(editor.value.getHTML())
  if (JSON.stringify(newVal) !== JSON.stringify(currentContent)) {
    editor.value.commands.setContent(
      diaryContentToHtml(newVal, props.attachmentMap, attachmentFilenames.value),
    )
  }
})

// --- Click handler (image preview) ---

function findAttachmentNode(el: HTMLElement | null): { type: string; attachmentId: string; el: HTMLElement } | null {
  while (el && el !== editorElement.value) {
    const tag = el.tagName.toUpperCase()
    if (tag === 'IMG' && el.dataset.id) return { type: 'image', attachmentId: el.dataset.id, el }
    if (tag === 'VIDEO' && el.dataset.id) return { type: 'video', attachmentId: el.dataset.id, el }
    if (tag === 'AUDIO' && el.dataset.id) return { type: 'audio', attachmentId: el.dataset.id, el }
    if (el.classList.contains('editor-file-attachment') && (el as HTMLElement).dataset.id) {
      return { type: 'file', attachmentId: (el as HTMLElement).dataset.id!, el }
    }
    el = el.parentElement
  }
  return null
}

function handleWrapperClick(e: MouseEvent) {
  // 点击附件节点时处理图片预览
  const found = findAttachmentNode(e.target as HTMLElement)
  if (found?.type === 'image') {
    const album = found.el.closest('.editor-image-album') as HTMLElement | null
    if (albumAnchor.value) {
      if (album) return
      toggleAlbumImage(found.attachmentId)
      return
    }
    if (album?.dataset.displayMode === 'stackedCards') {
      if (currentPlatform === 'android') editor.value?.commands.blur()
      animateStackedAlbumCycle(
        album,
        () => cycleStackedAlbum(album.dataset.id || ''),
      )
      return
    }
    const url = props.attachmentMap[found.attachmentId]
    if (url) emit('showImage', url)
    return
  }
  // 点击编辑器空白区域（如底部）时聚焦到末尾
  const proseMirror = editorElement.value?.querySelector('.ProseMirror') as HTMLElement | null
  if (
    editorElement.value
    && proseMirror
    && shouldFocusEditorEnd(e.target, editorElement.value, proseMirror, e.clientY)
  ) {
    editor.value?.chain().focus('end').run()
  }
}

function handleWrapperPointerDown(e: PointerEvent) {
  if (!shouldPreventStackedAlbumEditorFocus(e.target, currentPlatform)) return
  e.preventDefault()
  e.stopPropagation()
}

function cycleStackedAlbum(albumId: string) {
  if (!editor.value || !albumId) return
  editor.value.commands.command(({ tr }) => {
    tr.doc.descendants((node, pos) => {
      if (node.type.name !== 'albumNode' || node.attrs.id !== albumId) return
      const images = [...node.attrs.images]
      const urls = [...node.attrs.urls]
      if (images.length > 1) {
        images.push(images.shift()!)
        urls.push(urls.shift()!)
        tr.setNodeMarkup(pos, undefined, {
          ...node.attrs,
          images,
          urls,
          hasCycled: true,
        })
      }
    })
    return true
  })
}

function changeAlbumMode(
  albumId: string,
  displayMode: 'horizontalList' | 'stackedCards',
) {
  if (!editor.value || !albumId) return
  editor.value.commands.setContent(
    changeAlbumDisplayMode(editor.value.getJSON(), albumId, displayMode),
  )
}

function startAlbumSelection(filename: string) {
  albumAnchor.value = filename
  albumSelection.value = [filename]
  updateAlbumSelectionClasses()
}

function toggleAlbumImage(filename: string) {
  if (filename === albumAnchor.value) return
  albumSelection.value = albumSelection.value.includes(filename)
    ? albumSelection.value.filter(image => image !== filename)
    : [...albumSelection.value, filename]
  updateAlbumSelectionClasses()
}

function updateAlbumSelectionClasses() {
  nextTick(() => {
    const selected = new Set(albumSelection.value)
    editorElement.value
      ?.querySelectorAll('.ProseMirror > img[data-id]')
      .forEach(image => {
        const element = image as HTMLElement
        element.classList.toggle('album-image-selected', selected.has(element.dataset.id || ''))
      })
  })
}

function cancelAlbumSelection() {
  albumAnchor.value = ''
  albumSelection.value = []
  updateAlbumSelectionClasses()
}

function confirmAlbum(displayMode: 'horizontalList' | 'stackedCards') {
  if (!editor.value || albumSelection.value.length < 2) return
  const nextDocument = createAlbumDocument(
    editor.value.getJSON(),
    albumSelection.value,
    albumAnchor.value,
    crypto.randomUUID(),
    displayMode,
    props.attachmentMap,
  )
  cancelAlbumSelection()
  editor.value.commands.setContent(nextDocument)
}

function openAlbumInsertDialog(filename: string) {
  if (!editor.value) return
  const albums = listAlbums(editor.value.getJSON())
  if (albums.length === 0) {
    $q.notify({ type: 'info', message: '当前日记中没有可加入的图集' })
    return
  }
  albumInsertSource.value = filename
  albumInsertTargets.value = albums
  showAlbumInsertDialog.value = true
}

function insertImageIntoAlbum(albumId: string, insertionIndex: number) {
  if (!editor.value || !albumInsertSource.value) return
  editor.value.commands.setContent(addImageToAlbumDocument(
    editor.value.getJSON(),
    albumInsertSource.value,
    albumId,
    insertionIndex,
    props.attachmentMap,
  ))
  albumInsertSource.value = ''
  albumInsertTargets.value = []
}

function applyAlbumSplit(
  albumId: string,
  filename: string,
  operation: AlbumSplitOperation,
) {
  if (!editor.value) return
  editor.value.commands.setContent(splitAlbumDocument(
    editor.value.getJSON(),
    albumId,
    filename,
    operation,
  ))
}

function requestSingleImageSplit(albumId: string, filename: string) {
  $q.dialog({
    title: '拆分当前图片',
    message: '请选择当前图片相对于剩余图集的位置',
    options: {
      type: 'radio',
      model: 'before',
      items: [
        { label: '放在剩余图集前面', value: 'before' },
        { label: '放在剩余图集后面', value: 'after' },
      ],
    },
    cancel: true,
  }).onOk((position: 'before' | 'after') => {
    applyAlbumSplit(albumId, filename, { type: 'single', position })
  })
}

function requestRangeSplit(albumId: string, filename: string) {
  $q.dialog({
    title: '拆分连续图片',
    message: '请选择要从图集中拆分的范围',
    options: {
      type: 'radio',
      model: 'before',
      items: [
        { label: '当前图片及其前面的所有图片', value: 'before' },
        { label: '当前图片及其后面的所有图片', value: 'after' },
      ],
    },
    cancel: true,
  }).onOk((direction: 'before' | 'after') => {
    applyAlbumSplit(albumId, filename, { type: 'range', direction })
  })
}

// --- Context menu ---

async function handleContextMenu(e: MouseEvent) {
  const found = findAttachmentNode(e.target as HTMLElement)
  if (!found) return
  e.preventDefault()

  const att = getAttachmentMeta(found.attachmentId)
  if (!att) return

  interface MenuAction {
    label: string
    action: () => void
  }

  const buttons: MenuAction[] = [
    {
      label: `转成${att.encrypted ? '普通' : '加密'}附件`,
      action: () => emit('toggleAttachmentEncryption', found.attachmentId),
    },
    {
      label: '保存到本地',
      action: () => emit('saveDecryptAttachment', found.attachmentId),
    },
  ]

  if (found.type === 'image') {
    const album = found.el.closest('.editor-image-album') as HTMLElement | null
    const isAlbumImage = Boolean(album)
    const isSmall = found.el.getAttribute('data-size') === 'small'
    if (!isAlbumImage) {
      buttons.push({
        label: isSmall ? '大图模式' : '小图模式',
        action: () => {
          editor.value?.commands.command(({ tr }) => {
            tr.doc.descendants((node, pos) => {
              if (node.attrs.id === found.attachmentId) {
                tr.setNodeMarkup(pos, undefined, {
                  ...node.attrs,
                  size: isSmall ? null : 'small',
                })
              }
            })
            return true
          })
        },
      })
    }
    buttons.push(
      { label: '顺时针旋转90°', action: () => emit('rotateAttachment', found.attachmentId, 90) },
      { label: '逆时针旋转90°', action: () => emit('rotateAttachment', found.attachmentId, -90) },
      { label: '旋转180°', action: () => emit('rotateAttachment', found.attachmentId, 180) },
    )
    if (!isAlbumImage) {
      buttons.push({
        label: '创建图集',
        action: () => startAlbumSelection(found.attachmentId),
      })
      if (editor.value && listAlbums(editor.value.getJSON()).length > 0) {
        buttons.push({
          label: '加入已有图集',
          action: () => openAlbumInsertDialog(found.attachmentId),
        })
      }
    } else {
      const albumId = album?.dataset.id || ''
      const currentMode = album?.dataset.displayMode
      if (currentMode === 'stackedCards') {
        buttons.push({
          label: '预览图片',
          action: () => {
            const url = props.attachmentMap[found.attachmentId]
            if (url) emit('showImage', url)
          },
        })
      }
      buttons.push({
        label: currentMode === 'stackedCards' ? '切换为横向图集' : '切换为堆叠图集',
        action: () => changeAlbumMode(
          albumId,
          currentMode === 'stackedCards' ? 'horizontalList' : 'stackedCards',
        ),
      })
      buttons.push(
        {
          label: '拆分整个图集',
          action: () => applyAlbumSplit(albumId, found.attachmentId, { type: 'all' }),
        },
        {
          label: '仅拆分当前图片',
          action: () => requestSingleImageSplit(albumId, found.attachmentId),
        },
        {
          label: '拆分当前及前后图片',
          action: () => requestRangeSplit(albumId, found.attachmentId),
        },
      )
    }
  }

  if (found.type === 'file') {
    buttons.push({
      label: '重命名附件',
      action: () => emit('renameAttachment', found.attachmentId, att.filename, (newFilename: string) => {
        if (!editor.value) return
        editor.value.commands.command(({ tr }) => {
          tr.doc.descendants((node, pos) => {
            if (node.attrs.id === found.attachmentId) {
              tr.setNodeMarkup(pos, undefined, { ...node.attrs, filename: newFilename })
            }
          })
          return true
        })
      }),
    })
  }

  if (currentPlatform === 'android') {
    let selectedAction: MenuAction | undefined
    $q.bottomSheet({
      actions: buttons.map(b => ({ label: b.label, id: b.label })),
    }).onOk((action: { id: string }) => {
      selectedAction = buttons.find(b => b.label === action.id)
    }).onDismiss(() => {
      selectedAction?.action()
    })
  } else {
    try {
      const items = await Promise.all(
        buttons.map(b => MenuItem.new({ text: b.label, action: b.action })),
      )
      const menu = await Menu.new({ items })
      await menu.popup()
    } catch (err) {
      console.error('上下文菜单失败:', err)
    }
  }
}

// --- Lifecycle ---

let disposeImageLoading: (() => void) | null = null

onMounted(async () => {
  await nextTick()
  if (editorElement.value) {
    disposeImageLoading = setupEditorImageLoading(editorElement.value)
  }
  if (storageY.value > 0 && editorElement.value) {
    editorElement.value.scrollTop = storageY.value
  }
})

onBeforeUnmount(() => {
  disposeImageLoading?.()
  editor.value?.destroy()
})

function insertAttachments(insertions: readonly AttachmentInsertion[], atEnd: boolean) {
  const currentEditor = editor.value
  if (!currentEditor || !insertions.length) return Boolean(currentEditor)

  const chain = currentEditor.chain()
  if (atEnd) {
    chain.setTextSelection(currentEditor.state.doc.content.size)
  } else {
    chain.focus()
  }
  return chain.insertContent(attachmentInsertionsToEditorContent(insertions)).run()
}

defineExpose({
  editor,
  focusEnd: () => editor.value?.commands.focus('end'),
  insertImage: (id: string) => (editor.value?.chain().focus() as any).insertImage({ id, src: props.attachmentMap[id] || '' }).run(),
  insertAlbum: (id: string, images: string[], urls: string[]) => (editor.value?.chain().focus() as any).insertAlbum({
    id,
    images,
    urls,
    displayMode: 'horizontalList',
    hasCycled: false,
  }).run(),
  insertVideo: (id: string) => (editor.value?.chain().focus() as any).insertVideo({ id, src: props.attachmentMap[id] || '' }).run(),
  insertAudio: (id: string) => (editor.value?.chain().focus() as any).insertAudio({ id, src: props.attachmentMap[id] || '' }).run(),
  insertFile: (id: string, filename: string) => (editor.value?.chain().focus() as any)
    .insertFile({ id, filename }).run(),
  insertAttachments,
  updateSrc(attachmentId: string, newUrl: string) {
    if (!editor.value) return false
    editor.value.commands.command(({ tr }) => {
      tr.doc.descendants((node, pos) => {
        if (node.attrs.id === attachmentId) {
          tr.setNodeMarkup(pos, undefined, { ...node.attrs, src: newUrl })
        } else if (node.type.name === 'albumNode') {
          const imageIndex = (node.attrs.images as string[]).indexOf(attachmentId)
          if (imageIndex >= 0) {
            const urls = [...node.attrs.urls]
            urls[imageIndex] = newUrl
            tr.setNodeMarkup(pos, undefined, { ...node.attrs, urls })
          }
        }
      })
      return true
    })
    return true
  },
  undo: () => editor.value?.chain().undo().run(),
  redo: () => editor.value?.chain().redo().run(),
})
</script>

<template>
  <div
    ref="editorElement"
    class="tiptap-wrapper"
    @pointerdown.capture="handleWrapperPointerDown"
    @click="handleWrapperClick"
    @contextmenu="handleContextMenu"
  >
    <EditorContent :editor="editor" />
    <AlbumImageInsertDialog
      v-model="showAlbumInsertDialog"
      :albums="albumInsertTargets"
      @insert="insertImageIntoAlbum"
    />
    <div v-if="albumAnchor" class="album-selection-bar" @click.stop>
      <span>已选择 {{ albumSelection.length }} 张图片</span>
      <q-btn flat dense label="取消" @click="cancelAlbumSelection" />
      <q-btn
        flat
        dense
        label="横向图集"
        :disable="albumSelection.length < 2"
        @click="confirmAlbum('horizontalList')"
      />
      <q-btn
        flat
        dense
        label="堆叠图集"
        :disable="albumSelection.length < 2"
        @click="confirmAlbum('stackedCards')"
      />
    </div>
  </div>
</template>

<style scoped lang="scss">
.tiptap-wrapper {
  width: 100%;
  box-sizing: border-box;
  flex: 1;
  overflow-y: auto;
  height: 0;
  position: relative;
}

.album-selection-bar {
  position: sticky;
  bottom: 8px;
  z-index: 5;
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 4px;
  margin: 8px;
  padding: 6px 8px;
  border: 1px solid var(--pad-border-color);
  border-radius: 8px;
  background: var(--pad-bg-color);
  box-shadow: 0 2px 10px var(--pad-shadow-color-200);
}

.tiptap-wrapper :deep(.ProseMirror) {
  outline: none;
  min-height: 100%;
  padding: 0;
  text-align: left;

  > * + * {
    margin-top: 0.75em;
  }

  p.is-editor-empty:first-child::before {
    color: #adb5bd;
    content: attr(data-placeholder);
    float: left;
    height: 0;
    pointer-events: none;
  }
}
</style>

<style lang="scss">
.tiptap-wrapper {
  img[data-id] {
    cursor: pointer;
    min-height: 50px;
    transition: width 0.3s ease, opacity 0.25s ease;
    width: auto;
  }

  .ProseMirror > img[data-id].editor-image-loading {
    display: block;
    box-sizing: border-box;
    width: 100%;
    min-height: 180px;
    aspect-ratio: 16 / 9;
    opacity: 0.65;
    background: linear-gradient(
      90deg,
      var(--pad-bg-color-200) 25%,
      var(--pad-bg-color) 50%,
      var(--pad-bg-color-200) 75%
    );
    background-size: 200% 100%;
    animation: editor-image-skeleton 1.25s ease-in-out infinite;
  }

  .ProseMirror > img[data-id].editor-image-loaded:not([data-size="small"]) {
    display: block;
    width: var(--image-natural-width, auto);
    height: auto;
    opacity: 1;
  }

  .ProseMirror > img[data-id].editor-image-error {
    display: block;
    width: 100%;
    min-height: 100px;
    background: color-mix(in srgb, var(--q-negative) 12%, transparent);
  }

  img[data-id]:hover {
    box-shadow: 0 0 0 3px rgba(64, 158, 255, 0.5);
  }

  .ProseMirror > img.album-image-selected {
    outline: 4px solid var(--pad-primary-color, #1976d2);
    outline-offset: 2px;
  }

  .editor-image-album {
    display: flex;
    gap: 8px;
    width: 100%;
    padding: 8px;
    overflow-x: auto;
    border: 1px solid var(--pad-border-color);
    border-radius: 10px;
    background: var(--pad-bg-color-200);

    img[data-id] {
      flex: 0 0 min(72vw, 360px);
      width: min(72vw, 360px);
      height: min(72vw, 360px);
      object-fit: cover;
      border-radius: 8px;
    }

    img[data-id].editor-image-loading {
      opacity: 0.65;
      background: linear-gradient(
        90deg,
        var(--pad-bg-color-200) 25%,
        var(--pad-bg-color) 50%,
        var(--pad-bg-color-200) 75%
      );
      background-size: 200% 100%;
      animation: editor-image-skeleton 1.25s ease-in-out infinite;
    }

    &[data-display-mode="stackedCards"] {
      position: relative;
      display: block;
      min-height: min(72vw, 360px);
      overflow: visible;

      img[data-id] {
        position: absolute;
        inset: 8px auto auto 50%;
        transform: translateX(-50%);
        box-shadow: 0 4px 14px rgba(0, 0, 0, 0.25);
        transition:
          transform 300ms cubic-bezier(0.22, 1, 0.36, 1),
          opacity 300ms ease,
          filter 300ms ease;
      }

      img:nth-child(1) {
        z-index: 3;
        transform: translateX(-50%) rotate(0deg);
        opacity: 1;
        filter: none;
      }

      img:nth-child(2) {
        z-index: 2;
        transform: translateX(-34%) rotate(5deg) scale(0.94);
        opacity: 0.58;
        filter: saturate(0.72) brightness(0.82);
      }

      img:nth-child(n+3) {
        visibility: hidden;
      }

      &[data-has-cycled="true"] img:last-child:not(:nth-child(2)) {
        z-index: 1;
        visibility: visible;
        transform: translateX(-66%) rotate(-5deg) scale(0.94);
        opacity: 0.46;
        filter: saturate(0.68) brightness(0.78);
      }

      &.album-cycling {
        img:nth-child(1) {
          z-index: 4;
          transform: translateX(-66%) rotate(-5deg) scale(0.94);
          opacity: 0.46;
          filter: saturate(0.68) brightness(0.78);
        }

        img:nth-child(2) {
          z-index: 3;
          transform: translateX(-50%) rotate(0deg) scale(1);
          opacity: 1;
          filter: none;
        }
      }
    }
  }

  img[data-size="small"] {
    width: 32% !important;
    aspect-ratio: 1 / 1;
    object-fit: cover;
    display: inline-block;
  }

  .ProseMirror > img[data-size="small"].editor-image-loading {
    width: 32% !important;
    min-height: 0;
    aspect-ratio: 1 / 1;
  }

  audio[data-id] {
    width: 90%;
    margin: 10px auto;
  }

  video[data-id] {
    border-radius: 8px;
    margin: 10px 0;
    background: #000;
  }

  img, video, audio {
    padding: 5px;
    max-width: calc(100% - 10px);
    -webkit-touch-callout: none;
    user-select: none;
    -webkit-user-select: none;
    -webkit-user-drag: none;
  }

  .editor-file-attachment {
    display: inline-flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 12px;
    background-color: var(--pad-bg-color);
    border: 1px solid var(--pad-border-color);
    border-radius: 6px;
    cursor: pointer;
    -webkit-user-select: none;
    transition: all 0.2s ease;
    width: 100%;

    &:hover {
      background-color: var(--pad-bg-color-300);
      border-color: var(--pad-border-color-300);
      color: var(--pad-text-color-300);
    }

    .file-title {
      display: flex;
      align-items: center;

      .file-icon {
        font-size: 1.2em;
        margin-right: 8px;
      }

      .file-name {
        font-size: 14px;
        color: var(--pad-text-color);
        word-break: break-all;
        overflow: hidden;
        text-overflow: ellipsis;
        display: -webkit-box;
        -webkit-line-clamp: 2;
        -webkit-box-orient: vertical;
      }
    }
  }

  .ProseMirror-selectednode {
    outline: 3px solid rgba(64, 158, 255, 0.5);
    outline-offset: 2px;
  }
}

@keyframes editor-image-skeleton {
  0% { background-position: 200% 0; }
  100% { background-position: -200% 0; }
}
</style>
