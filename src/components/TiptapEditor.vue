<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, watch, nextTick } from 'vue'
import { useEditor, EditorContent } from '@tiptap/vue-3'
import StarterKit from '@tiptap/starter-kit'
import Placeholder from '@tiptap/extension-placeholder'
import { useScroll, useStorage } from '@vueuse/core'
import { useQuasar } from 'quasar'
import { platform } from '@tauri-apps/plugin-os'
import { Menu, MenuItem } from '@tauri-apps/api/menu'
import type { DiarySummary } from '../bindings'
import { htmlToMarkdown, markdownToHtml } from './editor/markdownConverter'
import { ImageNode, VideoNode, AudioNode, FileNode } from './editor/tiptap-extensions'

const props = defineProps<{
  modelValue: string
  diarySummary?: DiarySummary
  attachmentMap: Record<string, string>
}>()

const emit = defineEmits<{
  (e: 'update:modelValue', value: string): void
  (e: 'pasteAttachments', files: File[]): void
  (e: 'showImage', src: string): void
  (e: 'toggleAttachmentEncryption', filename: string): void
  (e: 'rotateAttachment', filename: string, rotation: number): void
  (e: 'renameAttachment', filename: string, cb: (newFilename: string) => void): void
  (e: 'saveDecryptAttachment', filename: string): void
}>()

const $q = useQuasar()
const editorElement = ref<HTMLDivElement>()
const currentPlatform = platform()

const storageY = useStorage(`scroll-y-${props.diarySummary?.id}`, 0, sessionStorage)
const { y } = useScroll(editorElement, {
  behavior: 'smooth',
  onScroll() {
    storageY.value = y.value
  },
})

function getAttachmentMeta(filename: string) {
  return props.diarySummary?.attachments.find(a => a.filename === filename) || null
}

const editor = useEditor({
  content: markdownToHtml(props.modelValue, props.attachmentMap),
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
    const md = htmlToMarkdown(html)
    emit('update:modelValue', md)
  },
})

watch(() => props.modelValue, (newVal) => {
  if (!editor.value || newVal === undefined) return
  const currentMd = htmlToMarkdown(editor.value.getHTML())
  if (newVal !== currentMd) {
    editor.value.commands.setContent(markdownToHtml(newVal, props.attachmentMap))
  }
})

// --- Click handler (image preview) ---

function findAttachmentNode(el: HTMLElement | null): { type: string; filename: string; el: HTMLElement } | null {
  while (el && el !== editorElement.value) {
    const tag = el.tagName.toUpperCase()
    if (tag === 'IMG' && el.dataset.id) return { type: 'image', filename: el.dataset.id, el }
    if (tag === 'VIDEO' && el.dataset.id) return { type: 'video', filename: el.dataset.id, el }
    if (tag === 'AUDIO' && el.dataset.id) return { type: 'audio', filename: el.dataset.id, el }
    if (el.classList.contains('editor-file-attachment') && (el as HTMLElement).dataset.id) {
      return { type: 'file', filename: (el as HTMLElement).dataset.id!, el }
    }
    el = el.parentElement
  }
  return null
}

function handleWrapperClick(e: MouseEvent) {
  // 点击附件节点时处理图片预览
  const found = findAttachmentNode(e.target as HTMLElement)
  if (found?.type === 'image') {
    const url = props.attachmentMap[found.filename]
    if (url) emit('showImage', url)
    return
  }
  // 点击编辑器空白区域（如底部）时聚焦到末尾
  const proseMirror = editorElement.value?.querySelector('.ProseMirror') as HTMLElement | null
  if (e.target === editorElement.value || (proseMirror && e.target === proseMirror)) {
    editor.value?.chain().focus('end').run()
  }
}

// --- Context menu ---

async function handleContextMenu(e: MouseEvent) {
  const found = findAttachmentNode(e.target as HTMLElement)
  if (!found) return
  e.preventDefault()

  const att = getAttachmentMeta(found.filename)
  if (!att) return

  interface MenuAction {
    label: string
    action: () => void
  }

  const buttons: MenuAction[] = [
    {
      label: `转成${att.encrypted ? '普通' : '加密'}附件`,
      action: () => emit('toggleAttachmentEncryption', found.filename),
    },
    {
      label: '保存到本地',
      action: () => emit('saveDecryptAttachment', found.filename),
    },
  ]

  if (found.type === 'image') {
    const isSmall = found.el.getAttribute('data-size') === 'small'
    buttons.push({
      label: isSmall ? '大图模式' : '小图模式',
      action: () => {
        editor.value?.commands.command(({ tr }) => {
          tr.doc.descendants((node, pos) => {
            if (node.attrs.id === found.filename) {
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
    buttons.push(
      { label: '顺时针旋转90°', action: () => emit('rotateAttachment', found.filename, 90) },
      { label: '逆时针旋转90°', action: () => emit('rotateAttachment', found.filename, -90) },
      { label: '旋转180°', action: () => emit('rotateAttachment', found.filename, 180) },
    )
  }

  if (found.type === 'file') {
    buttons.push({
      label: '重命名附件',
      action: () => emit('renameAttachment', found.filename, (newFilename: string) => {
        if (!editor.value) return
        editor.value.commands.command(({ tr }) => {
          tr.doc.descendants((node, pos) => {
            if (node.attrs.id === found.filename) {
              tr.setNodeMarkup(pos, undefined, { ...node.attrs, id: newFilename })
            }
          })
          return true
        })
      }),
    })
  }

  if (currentPlatform === 'android') {
    $q.bottomSheet({
      actions: buttons.map(b => ({ label: b.label, id: b.label })),
    }).onOk((action: { id: string }) => {
      buttons.find(b => b.label === action.id)?.action()
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

onMounted(async () => {
  await nextTick()
  if (storageY.value > 0 && editorElement.value) {
    editorElement.value.scrollTop = storageY.value
  }
})

onBeforeUnmount(() => {
  editor.value?.destroy()
})

defineExpose({
  editor,
  focusEnd: () => editor.value?.commands.focus('end'),
  insertImage: (id: string) => (editor.value?.chain().focus() as any).insertImage({ id, src: props.attachmentMap[id] || '' }).run(),
  insertVideo: (id: string) => (editor.value?.chain().focus() as any).insertVideo({ id, src: props.attachmentMap[id] || '' }).run(),
  insertAudio: (id: string) => (editor.value?.chain().focus() as any).insertAudio({ id, src: props.attachmentMap[id] || '' }).run(),
  insertFile: (id: string) => (editor.value?.chain().focus() as any).insertFile({ id }).run(),
  updateSrc(filename: string, newUrl: string) {
    if (!editor.value) return false
    editor.value.commands.command(({ tr }) => {
      tr.doc.descendants((node, pos) => {
        if (node.attrs.id === filename) {
          tr.setNodeMarkup(pos, undefined, { ...node.attrs, src: newUrl })
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
  <div ref="editorElement" class="tiptap-wrapper" @click="handleWrapperClick" @contextmenu="handleContextMenu">
    <EditorContent :editor="editor" />
  </div>
</template>

<style scoped lang="scss">
.tiptap-wrapper {
  width: 100%;
  box-sizing: border-box;
  flex: 1;
  overflow-y: auto;
  height: 0;
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
    transition: width 0.3s ease;
    width: auto;
  }

  img[data-id]:hover {
    box-shadow: 0 0 0 3px rgba(64, 158, 255, 0.5);
  }

  img[data-size="small"] {
    width: 32% !important;
    aspect-ratio: 1 / 1;
    object-fit: cover;
    display: inline-block;
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
</style>
