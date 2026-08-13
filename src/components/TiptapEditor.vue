<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch, nextTick } from 'vue'
import { useEditor, EditorContent } from '@tiptap/vue-3'
import StarterKit from '@tiptap/starter-kit'
import Placeholder from '@tiptap/extension-placeholder'
import TaskItem from '@tiptap/extension-task-item'
import TaskList from '@tiptap/extension-task-list'
import { useScroll, useStorage } from '@vueuse/core'
import { platform } from '@tauri-apps/plugin-os'
import type { AttachmentMeta, DiaryContent, DiarySummary } from '../bindings'
import { diaryContentToHtml, htmlToDiaryContent } from './editor/markdownConverter'
import {
  shouldFocusEditorEnd,
  shouldPreventEditorFocus,
} from './editor/editorClick'
import { animateStackedAlbumCycle } from './editor/albumAnimation'
import { setupEditorImageLoading } from './editor/imageLoading'
import { findAttachmentNode } from './editor/attachmentNode'
import { ImageNode, VideoNode, AudioNode, FileNode, AlbumNode } from './editor/tiptap-extensions'
import AlbumImageInsertDialog from './AlbumImageInsertDialog.vue'
import {
  attachmentInsertionsToEditorContent,
  type AttachmentInsertion,
} from '../utils/attachmentInsertion'
import { Decoration, DecorationSet } from '@tiptap/pm/view'
import { findSearchHighlightRanges } from '../utils/searchHighlight'
import { useEditorAlbumActions } from '../composables/useEditorAlbumActions'
import { useAttachmentContextMenu } from '../composables/useAttachmentContextMenu'

const props = defineProps<{
  modelValue: DiaryContent
  diarySummary?: DiarySummary
  attachments: AttachmentMeta[]
  attachmentMap: Record<string, string>
  searchTerms?: string[]
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

const editorElement = ref<HTMLDivElement>()
const currentPlatform = platform()

const storageY = useStorage(`scroll-y-${props.diarySummary?.id}`, 0, sessionStorage)
const { y } = useScroll(editorElement, {
  behavior: 'smooth',
  onScroll() {
    storageY.value = y.value
  },
})

function getAttachmentMeta(attachmentId: string) {
  return props.attachments.find(attachment => attachment.id === attachmentId) || null
}

const attachmentFilenames = computed<Record<string, string>>(() => Object.fromEntries(
  props.attachments.map(attachment => [attachment.id, attachment.filename]),
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
    TaskList.configure({
      HTMLAttributes: {
        class: 'editor-task-list',
      },
    }),
    TaskItem.configure({
      HTMLAttributes: {
        class: 'editor-task-item',
      },
      a11y: {
        checkboxLabel: (node, checked) => `${checked ? '取消完成' : '标记完成'}：${node.textContent || '待办事项'}`,
      },
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
    decorations: state => {
      if (!props.searchTerms?.length) return DecorationSet.empty
      const decorations: Decoration[] = []
      state.doc.descendants((node, pos) => {
        if (!node.isText || !node.text) return
        for (const range of findSearchHighlightRanges(node.text, props.searchTerms!)) {
          decorations.push(Decoration.inline(
            pos + range.from,
            pos + range.to,
            { class: 'search-highlight' },
          ))
        }
      })
      return DecorationSet.create(state.doc, decorations)
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

const albumActions = useEditorAlbumActions({
  editor,
  editorElement,
  currentPlatform,
  attachmentMap: () => props.attachmentMap,
})
const {
  albumSelection,
  albumAnchor,
  showAlbumInsertDialog,
  albumInsertTargets,
  cycleStackedAlbum,
  toggleAlbumImage,
  cancelAlbumSelection,
  confirmAlbum,
  insertImageIntoAlbum,
} = albumActions

const { handleContextMenu } = useAttachmentContextMenu({
  editor,
  editorElement,
  currentPlatform,
  getAttachment: getAttachmentMeta,
  attachmentUrl: attachmentId => props.attachmentMap[attachmentId],
  albumActions,
  toggleEncryption: attachmentId => emit('toggleAttachmentEncryption', attachmentId),
  rotate: (attachmentId, rotation) => emit('rotateAttachment', attachmentId, rotation),
  rename: (attachmentId, filename, callback) => {
    emit('renameAttachment', attachmentId, filename, callback)
  },
  saveDecrypted: attachmentId => emit('saveDecryptAttachment', attachmentId),
  showImage: url => emit('showImage', url),
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

watch(() => props.searchTerms, () => {
  if (editor.value) editor.value.view.dispatch(editor.value.state.tr)
}, { deep: true })

// --- Click handler (image preview) ---

function handleWrapperClick(e: MouseEvent) {
  // 点击附件节点时处理图片预览
  const found = findAttachmentNode(e.target as HTMLElement, editorElement.value)
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
  if (albumAnchor.value) return
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
  if (!shouldPreventEditorFocus(e.target, currentPlatform, Boolean(albumAnchor.value))) return
  e.preventDefault()
  e.stopPropagation()
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
    color: var(--pad-text-color-400);
    content: attr(data-placeholder);
    float: left;
    height: 0;
    pointer-events: none;
  }

  .search-highlight {
    border-radius: 2px;
    background-color: color-mix(in srgb, var(--pad-warning-color) 45%, transparent);
    box-shadow: 0 0 0 1px color-mix(in srgb, var(--pad-warning-color) 22%, transparent);
  }

}
</style>

<style lang="scss">
.tiptap-wrapper {
  .ProseMirror .editor-task-list {
    margin: 0.65em 0;
    padding: 0;
    list-style: none;

    .editor-task-item {
      display: flex;
      align-items: flex-start;
      gap: 10px;
      min-height: 28px;
      margin: 3px 0;

      > label {
        position: relative;
        flex: none;
        display: grid;
        place-items: center;
        width: 20px;
        height: 24px;
        cursor: pointer;
        user-select: none;

        input[type='checkbox'] {
          position: absolute;
          width: 20px;
          height: 20px;
          margin: 0;
          opacity: 0;
          cursor: pointer;
        }

        span {
          display: grid;
          place-items: center;
          box-sizing: border-box;
          width: 19px;
          height: 19px;
          border: 1.5px solid var(--pad-border-color-300);
          border-radius: 6px;
          background: var(--pad-bg-color-100);
          transition: border-color 0.15s ease, background-color 0.15s ease, transform 0.15s ease;
        }

        input[type='checkbox']:checked + span {
          border-color: var(--pad-primary-color);
          background: var(--pad-primary-color);
        }

        input[type='checkbox']:checked + span::after {
          content: '';
          width: 9px;
          height: 5px;
          margin-top: -2px;
          border-bottom: 2px solid var(--pad-on-primary-color);
          border-left: 2px solid var(--pad-on-primary-color);
          transform: rotate(-45deg);
        }

        input[type='checkbox']:focus-visible + span {
          outline: 2px solid color-mix(in srgb, var(--pad-primary-color) 45%, transparent);
          outline-offset: 2px;
        }

        &:active span {
          transform: scale(0.9);
        }
      }

      > div {
        flex: 1;
        min-width: 0;
        padding: 1px 0 3px;
        line-height: 1.45;

        > p {
          margin: 0;
        }
      }

      &[data-checked='true'] > div {
        color: var(--pad-text-color-400);
        text-decoration: line-through;
        text-decoration-color: var(--pad-text-color-400);
      }
    }
  }

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
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--pad-primary-color) 50%, transparent);
  }

  .ProseMirror > img.album-image-selected {
    outline: 4px solid var(--pad-primary-color);
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
