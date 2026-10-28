<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch, nextTick } from 'vue'
import { useEditor, EditorContent } from '@tiptap/vue-3'
import {useQuasar} from 'quasar'
import StarterKit from '@tiptap/starter-kit'
import {DiaryDocument} from './editor/editorSchema'
import Placeholder from '@tiptap/extension-placeholder'
import TaskList from '@tiptap/extension-task-list'
import { useScroll, useStorage } from '@vueuse/core'
import { platform } from '@tauri-apps/plugin-os'
import type {
  AttachmentMeta,
  AudioWaveform,
  DiaryContent,
  DiaryLocation,
  DiarySummary,
} from '../bindings'
import { diaryContentToHtml, htmlToDiaryContent } from './editor/markdownConverter'
import {
  disableMobileImageDragging,
  shouldFocusEditorEnd,
  shouldPreventEditorFocus,
} from './editor/editorClick'
import { animateStackedAlbumCycle } from './editor/albumAnimation'
import { setupEditorImageLoading } from './editor/imageLoading'
import { findAttachmentNode } from './editor/attachmentNode'
import {
  ImageNode,
  VideoNode,
  AudioNode,
  FileNode,
  AlbumNode,
  SummaryNode,
  LocationNode,
} from './editor/tiptap-extensions'
import AlbumImageInsertDialog from './AlbumImageInsertDialog.vue'
import AttachmentBatchDownloadDialog from './editor/AttachmentBatchDownloadDialog.vue'
import DiaryBlockOrderDialog from './editor/DiaryBlockOrderDialog.vue'
import {
  attachmentInsertionsToEditorContent,
  type AttachmentInsertion,
} from '../utils/attachmentInsertion'
import { Decoration, DecorationSet } from '@tiptap/pm/view'
import { findSearchHighlightRanges } from '../utils/searchHighlight'
import { useEditorAlbumActions } from '../composables/useEditorAlbumActions'
import { useAttachmentContextMenu } from '../composables/useAttachmentContextMenu'
import {useEditorContextMenu, type EditorMenuAction} from '../composables/useEditorContextMenu'

import type {EditorJsonNode} from './editor/albumEditor'
import {
  createBlockOrderTransaction,
  describeDiaryBlocks,
  isValidBlockOrder,
  topLevelBlockIdentities,
  type DiaryBlockDescriptor,
} from './editor/blockOrder'
import {TaskCompletionOrder} from './editor/taskCompletionOrder'
import {createTaskItemExtension} from './editor/taskItem'
import {ListIndentation} from './editor/listEditing'
import {createDiaryTableExtensions} from './editor/tableEditing'
import {pasteEditorMarkdown} from './editor/markdownPaste'
import {isMobileEditorPlatform} from './editor/editorClick'
import {isHtmlAttachment} from '../utils/attachmentOpen'
import {attachmentPreviewKind} from '../utils/attachmentPreview'
import {insertImageAndMoveCursorAfter} from './editor/imageInsertion'
import {useConfigStore} from '../stores/config'
import {editorFontSizeCssVariables} from '../utils/editorFontSizes'
import TableInsertDialog from './editor/TableInsertDialog.vue'
import {
  TABLE_ACTION_GROUPS, canRunTableAction, runTableAction, tableTargetFromElement, validTableSize,
  type TableAction, type TableTarget,
} from './editor/tableCommands'
import {installEditorLongPress} from './editor/editorLongPress'
import type {Node as ProseMirrorNode} from '@tiptap/pm/model'
import type {Selection} from '@tiptap/pm/state'
import {closeHistory} from '@tiptap/pm/history'
import {copyTextToClipboard} from '../utils/clipboard'
import {summaryRange} from './editor/tiptap-extensions/SummaryNode'

const props = defineProps<{
  modelValue: DiaryContent
  diarySummary?: DiarySummary
  attachments: AttachmentMeta[]
  attachmentMap: Record<string, string>
  searchTerms?: string[]
  readOnly?: boolean
}>()

const emit = defineEmits<{
  (e: 'update:modelValue', value: DiaryContent): void
  (e: 'pasteAttachments', files: File[]): void
  (e: 'showImage', src: string): void
  (e: 'toggleAttachmentEncryption', attachmentId: string): void
  (e: 'rotateAttachment', attachmentId: string, rotation: number): void
  (e: 'renameAttachment', attachmentId: string, filename: string, cb: (newFilename: string) => void): void
  (e: 'saveDecryptAttachment', attachmentId: string): void
  (e: 'saveDecryptAttachments', attachmentIds: string[]): void
  (e: 'openHtmlAttachment', attachmentId: string): void
  (e: 'previewAttachment', attachmentId: string): void
  (e: 'openLocation', location: DiaryLocation): void
  (e: 'audioInfoGenerated', attachmentId: string, durationMs: number, waveform: AudioWaveform): void
  (e: 'editorFocused'): void
}>()

const editorElement = ref<HTMLDivElement>()
const showBatchDownloadDialog = ref(false)
const batchDownloadInitialId = ref('')
const currentPlatform = platform()
const $q = useQuasar()
const {showMenu: showEditorMenu} = useEditorContextMenu(currentPlatform)
const configStore = useConfigStore()
const editorFontSizes = configStore.useTauriConfig('editor_font_sizes')
const editorFontSizeStyle = computed(() => editorFontSizeCssVariables(editorFontSizes.value))

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

const showBlockOrderDialog = ref(false)
const blockOrderBlocks = ref<DiaryBlockDescriptor[]>([])
const blockOrderSnapshot = ref<string[]>([])
let blockOrderContainerPosition: number | undefined
let blockOrderDocument: ProseMirrorNode | undefined
const showTableDialog = ref(false)
let tableInsertion: {document: ProseMirrorNode; selection: Selection} | null = null

function openTableDialog() {
  const current = editor.value
  if (!current?.isEditable || current.isActive('table')) return
  tableInsertion = {document: current.state.doc, selection: current.state.selection}
  showTableDialog.value = true
}

function insertTable(rows: number, columns: number) {
  const current = editor.value
  const snapshot = tableInsertion
  if (!current?.isEditable || !snapshot || !validTableSize(rows, columns)) return
  if (snapshot.document !== current.state.doc) {
    showTableDialog.value = false
    $q.notify({type: 'warning', message: '日记内容已经发生变化，请重新插入表格'})
    return
  }
  // 恢复打开弹窗前的选区，不把插入表格和之前的文字输入合成一次撤销。
  current.chain().command(({tr}) => {
    closeHistory(tr).setSelection(snapshot.selection)
    return true
  }).insertTable({rows, cols: columns, withHeaderRow: true}).run()
  showTableDialog.value = false
  tableInsertion = null
}

function openTableMenu(target: TableTarget, cell: HTMLElement) {
  const current = editor.value
  if (!current || target.document !== current.state.doc) return
  if (isMobileEditorPlatform(currentPlatform)) {
    current.commands.blur()
    // 在 BottomSheet 记录回焦目标前同步收起编辑焦点，关闭菜单时不再唤起键盘。
    current.view.dom.blur()
  }
  // blur 也会提交事务，可能触发自动补尾段落；以事务完成后的真实单元格重新取位置。
  const refreshed = tableTargetFromElement(current, cell)
  if (!refreshed) return
  const groups: EditorMenuAction[][] = TABLE_ACTION_GROUPS.map(group => group.map(([action, label]) => ({
    label,
    enabled: canRunTableAction(current, refreshed, action),
    danger: action.startsWith('delete'),
    action: () => selectTableAction(refreshed, action),
  })))
  groups[0].unshift({label: '复制单元格内容', action: () => selectTableAction(refreshed, 'copy')})
  void showEditorMenu(groups)
}

async function selectTableAction(target: TableTarget, action: TableAction | 'copy') {
  const current = editor.value
  if (!current || current.isDestroyed) return
  if (target.document !== current.state.doc) {
    $q.notify({type: 'warning', message: '日记内容已经发生变化，请重新打开表格菜单'})
    return
  }
  if (action === 'copy') {
    const cell = current.state.doc.nodeAt(target.cellPosition)
    try { await copyTextToClipboard(cell?.textBetween(0, cell.content.size, '\n') ?? '') }
    catch { $q.notify({type: 'negative', message: '复制失败，请重试'}) }
  } else {
    runTableAction(current, target, action)
  }
}

const editor = useEditor({
  editable: !props.readOnly,
  content: diaryContentToHtml(props.modelValue, props.attachmentMap, attachmentFilenames.value),
  extensions: [
    StarterKit.configure({
      document: false,
      heading: { levels: [1, 2, 3, 4, 5] },
    }),
    DiaryDocument.configure({summary: true}),
    Placeholder.configure({
      placeholder: '开始记录...',
    }),
    TaskList.configure({
      HTMLAttributes: {
        class: 'editor-task-list',
      },
    }),
    createTaskItemExtension(isMobileEditorPlatform(currentPlatform)).configure({
      nested: true,
      HTMLAttributes: {
        class: 'editor-task-item',
      },
      a11y: {
        checkboxLabel: (node, checked) => `${checked ? '取消完成' : '标记完成'}：${node.textContent || '待办事项'}`,
      },
    }),
    TaskCompletionOrder,
    ListIndentation,
    ...createDiaryTableExtensions(),
    ImageNode.extend({group: 'diaryBlock'}),
    VideoNode.extend({group: 'diaryBlock'}),
    AudioNode.extend({group: 'diaryBlock'}).configure({
      getAttachment: getAttachmentMeta,
      onAudioInfoGenerated: (attachmentId, durationMs, waveform) => {
        if (!props.readOnly) emit('audioInfoGenerated', attachmentId, durationMs, waveform)
      },
    }),
    FileNode.extend({group: 'diaryBlock'}),
    AlbumNode.extend({group: 'diaryBlock'}),
    SummaryNode,
    LocationNode.extend({group: 'diaryBlock'}).configure({
      onOpen: location => emit('openLocation', location),
    }),
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
    handlePaste: (view, event) => {
      if (props.readOnly) return false
      const html = event.clipboardData?.getData('text/html')
      if (html) {
        const parsed = new DOMParser().parseFromString(html, 'text/html')
        if ((summaryRange(view.state) && parsed.querySelector('.editor-summary, details'))
          || parsed.querySelector('.editor-summary .editor-summary, details details')) {
          event.preventDefault()
          $q.notify({type: 'warning', message: '折叠内容不能嵌套，请先取消内层折叠'})
          return true
        }
      }
      const files = event.clipboardData?.files
      if (files && files.length > 0) {
        event.preventDefault()
        emit('pasteAttachments', Array.from(files))
        return true
      }
      if (view.state.selection.$from.parent.type.name === 'summaryTitle') {
        event.preventDefault()
        view.dispatch(view.state.tr.insertText((event.clipboardData?.getData('text/plain') ?? '').replace(/\r?\n/g, ' ')))
        return true
      }
      return pasteEditorMarkdown(view, event)
    },
  },
  onUpdate({ editor: ed }) {
    if (props.readOnly) return
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

const { handleContextMenu: handleAttachmentContextMenu } = useAttachmentContextMenu({
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
  startBatchDownload: attachmentId => {
    batchDownloadInitialId.value = attachmentId
    showBatchDownloadDialog.value = true
  },
  openHtml: attachmentId => emit('openHtmlAttachment', attachmentId),
  previewAttachment: attachmentId => emit('previewAttachment', attachmentId),
  showImage: url => emit('showImage', url),
})

function handleContextMenu(event: MouseEvent) {
  const current = editor.value
  const target = current ? tableTargetFromElement(current, event.target) : null
  if (target && current?.isEditable) {
    event.preventDefault()
    if (!isMobileEditorPlatform(currentPlatform)) {
      openTableMenu(target, (event.target as Element).closest<HTMLElement>('td, th')!)
    }
    return
  }
  void handleAttachmentContextMenu(event)
}

watch(() => props.modelValue, (newVal) => {
  if (!editor.value || newVal === undefined) return
  const currentContent = htmlToDiaryContent(editor.value.getHTML())
  if (JSON.stringify(newVal) !== JSON.stringify(currentContent)) {
    editor.value.commands.setContent(
      diaryContentToHtml(newVal, props.attachmentMap, attachmentFilenames.value),
    )
  }
})

watch(() => props.readOnly, value => {
  editor.value?.setEditable(!value)
  if (value) {
    showBlockOrderDialog.value = false
    showTableDialog.value = false
    editor.value?.commands.blur()
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
  if (found?.type === 'file') {
    const attachment = getAttachmentMeta(found.attachmentId)
    if (attachment && isHtmlAttachment(attachment)) {
      if (currentPlatform === 'android') editor.value?.commands.blur()
      emit('openHtmlAttachment', found.attachmentId)
      return
    }
    if (attachment && attachmentPreviewKind(attachment)) {
      if (currentPlatform === 'android') editor.value?.commands.blur()
      emit('previewAttachment', found.attachmentId)
      return
    }
  }
  if (albumAnchor.value) return
  // 点击编辑器空白区域（如底部）时聚焦到末尾
  const proseMirror = editorElement.value?.querySelector('.ProseMirror') as HTMLElement | null
  if (
    editorElement.value
    && proseMirror
    && editor.value?.isEditable
    && shouldFocusEditorEnd(e.target, editorElement.value, proseMirror, e.clientY)
  ) {
    editor.value?.chain().focus('end').run()
  }
}

function handleWrapperPointerDown(e: PointerEvent) {
  if (disableMobileImageDragging(e.target, currentPlatform)) {
    // 避免 ProseMirror 在按下可拖拽节点后重新开启 draggable；不阻止默认事件，保留点击和长按菜单。
    e.stopPropagation()
  }
  if (!shouldPreventEditorFocus(e.target, currentPlatform, Boolean(albumAnchor.value))) return
  e.preventDefault()
  e.stopPropagation()
}

function handleWrapperDragStart(e: DragEvent) {
  if (!disableMobileImageDragging(e.target, currentPlatform)) return
  e.preventDefault()
  e.stopPropagation()
}

// --- Lifecycle ---

let disposeImageLoading: (() => void) | null = null
let disposeTableLongPress: (() => void) | null = null

onMounted(async () => {
  await nextTick()
  if (editorElement.value) {
    disposeImageLoading = setupEditorImageLoading(editorElement.value)
    if (isMobileEditorPlatform(currentPlatform)) {
      disposeTableLongPress = installEditorLongPress(editorElement.value, target => {
        const current = editor.value
        const cell = target instanceof Element ? target.closest<HTMLElement>('td, th') : null
        const table = current?.isEditable ? tableTargetFromElement(current, target) : null
        return cell && table ? {cell, table} : null
      }, ({cell, table}) => openTableMenu(table, cell))
    }
  }
  if (storageY.value > 0 && editorElement.value) {
    editorElement.value.scrollTop = storageY.value
  }
})

onBeforeUnmount(() => {
  disposeImageLoading?.()
  disposeTableLongPress?.()
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
    const summary = summaryRange(currentEditor.state)
    if (summary && currentEditor.state.selection.$from.parent.type.name === 'summaryTitle') {
      chain.enterSummaryBody()
    }
  }
  return chain.insertContent(attachmentInsertionsToEditorContent(insertions)).run()
}

function openSelectedSummaryDialog() {
  if (props.readOnly) return
  const current = editor.value
  if (!current) return
  if (!current.state.selection.empty && !summaryRange(current.state)) {
    const document = current.state.doc
    const selection = current.state.selection
    const targets: {position: number; title: string}[] = []
    document.forEach((node, position) => {
      if (node.type.name === 'summaryNode') targets.push({position, title: node.firstChild?.textContent || '未命名折叠内容'})
    })
    if (targets.length) {
      const run = (position?: number) => {
        if (current.state.doc !== document) {
          $q.notify({type: 'warning', message: '正文已发生变化，请重新选择内容'})
          return
        }
        const chain = current.chain().command(({tr}) => {tr.setSelection(selection); return true})
        const ok = position === undefined ? chain.insertSummary().focus().run() : chain.appendToSummary(position).focus().run()
        if (!ok) $q.notify({type: 'warning', message: '当前选区不能放入折叠内容'})
      }
      void showEditorMenu([
        [{label: '创建新的折叠内容', action: () => run()}],
        targets.map(target => ({label: `添加到：${target.title}`, action: () => run(target.position)})),
      ])
      return
    }
  }
  if (!editor.value?.chain().focus().insertSummary().run()) {
    $q.notify({type: 'warning', message: '折叠内容不能嵌套，请选择折叠范围外的普通内容'})
  }
}

function openBlockOrderDialog() {
  const currentEditor = editor.value
  if (!currentEditor) return
  const summary = summaryRange(currentEditor.state)
  blockOrderContainerPosition = summary ? summary.pos + summary.node.firstChild!.nodeSize + 1 : undefined
  blockOrderDocument = currentEditor.state.doc
  const document = (summary ? summary.node.lastChild!.toJSON() : currentEditor.getJSON()) as EditorJsonNode
  const blocks = describeDiaryBlocks(document, props.attachments)
  if (blocks.length < 2) {
    $q.notify({type: 'info', message: '当前内容不足两个块，无需调整顺序'})
    return
  }

  // 这里只关闭输入焦点，不能调用 commands.blur()：后者会派发事务，可能让
  // TrailingNode 自动补空段落，从而在尚未调整顺序时就触发日记保存。
  currentEditor.view.dom.blur()
  blockOrderBlocks.value = blocks
  blockOrderSnapshot.value = topLevelBlockIdentities(document)
  showBlockOrderDialog.value = true
}

function applyBlockOrder(order: number[]) {
  const currentEditor = editor.value
  if (!currentEditor || !isValidBlockOrder(order, blockOrderBlocks.value.length)) {
    $q.notify({type: 'warning', message: '内容顺序无效，请重新调整'})
    return
  }

  const scope = blockOrderContainerPosition === undefined ? currentEditor.state.doc
    : currentEditor.state.doc.nodeAt(blockOrderContainerPosition)
  const currentDocument = (scope?.toJSON() ?? {type: 'doc'}) as EditorJsonNode
  const currentSnapshot = topLevelBlockIdentities(currentDocument)
  if (currentEditor.state.doc !== blockOrderDocument || !sameStrings(currentSnapshot, blockOrderSnapshot.value)) {
    showBlockOrderDialog.value = false
    $q.notify({type: 'warning', message: '日记内容已经发生变化，请重新打开排序'})
    return
  }

  const transaction = createBlockOrderTransaction(currentEditor.state, order, blockOrderContainerPosition)
  showBlockOrderDialog.value = false
  if (transaction) currentEditor.view.dispatch(transaction)
}

function sameStrings(left: readonly string[], right: readonly string[]) {
  return left.length === right.length && left.every((value, index) => value === right[index])
}

defineExpose({
  editor,
  focusEnd: () => editor.value?.commands.focus('end'),
  insertImage: (id: string) => {
    const currentEditor = editor.value
    if (!currentEditor) return false
    return insertImageAndMoveCursorAfter(currentEditor, {
      id,
      src: props.attachmentMap[id] || '',
    })
  },
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
  insertLocation: (location: DiaryLocation) => {
    if (editor.value?.state.selection.$from.parent.type.name === 'summaryTitle') editor.value.commands.enterSummaryBody()
    return editor.value?.chain().insertLocation(location).run()
  },
  insertAttachments,
  openSummaryDialog: openSelectedSummaryDialog,
  openTableDialog,
  openBlockOrderDialog,
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
    :class="{'is-read-only': readOnly}"
    :style="editorFontSizeStyle"
    @pointerdown.capture="handleWrapperPointerDown"
    @dragstart.capture="handleWrapperDragStart"
    @click="handleWrapperClick"
    @contextmenu="handleContextMenu"
  >
    <EditorContent :editor="editor" />
    <AttachmentBatchDownloadDialog v-model="showBatchDownloadDialog" :attachments="attachments"
                                   :initial-attachment-id="batchDownloadInitialId"
                                   @download="attachmentIds => emit('saveDecryptAttachments', attachmentIds)"/>
    <TableInsertDialog v-model="showTableDialog" @insert="insertTable"/>

    <DiaryBlockOrderDialog
      v-model="showBlockOrderDialog"
      :blocks="blockOrderBlocks"
      @confirm="applyBlockOrder"
    />
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
  &.is-read-only :deep(.editor-summary-edit) {
    display: none;
  }
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
  font-size: var(--diary-body-font-size, 16px);

  h1 {
    font-size: var(--diary-heading-1-font-size, 32px);
  }

  h2 {
    font-size: var(--diary-heading-2-font-size, 24px);
  }

  h3 {
    font-size: var(--diary-heading-3-font-size, 19px);
  }

  h4 {
    font-size: var(--diary-heading-4-font-size, 16px);
  }

  h5 {
    font-size: var(--diary-heading-5-font-size, 13px);
  }

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

  .tableWrapper {
    max-width: 100%;
    overflow-x: auto;
    overscroll-behavior-x: contain;
  }

  .editor-table {
    width: 100%;
    border-collapse: collapse;
    table-layout: auto;
    color: var(--pad-text-color);

    th, td {
      position: relative;
      box-sizing: border-box;
      min-width: 96px;
      padding: 8px 10px;
      border: 1px solid var(--pad-border-color-200);
      vertical-align: top;
      overflow-wrap: anywhere;
      background: var(--pad-bg-color-100);

      > p { margin: 0; }
      > p + p { margin-top: 0.4em; }
    }

    th {
      font-weight: 600;
      text-align: left;
      background: var(--pad-bg-color-200);
    }

    .selectedCell::after {
      content: '';
      position: absolute;
      inset: 0;
      pointer-events: none;
      background: color-mix(in srgb, var(--pad-primary-color) 18%, transparent);
    }
  }

}

</style>

<style lang="scss">
.tiptap-wrapper {
  .editor-summary {
    position: relative; margin: .65em 0; padding-left: 24px;
    color: var(--pad-text-color-200);
    .editor-summary-title { font-weight: 600; min-height: 1.5em; outline: none; }
    .editor-summary-title:empty::before { content: '折叠标题'; color: var(--pad-text-color-400); pointer-events: none; }
    .editor-summary-toggle { position: absolute; left: 0; top: 0; border: 0; background: transparent;
      color: var(--pad-text-color-300); font-size: 18px; width: 24px; padding: 0; cursor: pointer; }
    .editor-summary-content { min-height: 1.5em; padding-left: 8px; margin-top: .35em;
      border-left: 2px solid transparent; }
    &:focus-within .editor-summary-content { border-left-color: var(--pad-border-color-300); }
    .editor-summary-actions { display: none; gap: 12px; padding: 4px 8px; flex-wrap: wrap; }
    &:focus-within .editor-summary-actions { display: flex; }
    .editor-summary-actions button { font-size: 12px; background: transparent; border: 0;
      padding: 4px 0; color: var(--pad-text-color-400); cursor: pointer; }
    &.is-collapsed .editor-summary-content, &.is-collapsed .editor-summary-actions { display: none; }
  }
  &.is-read-only .editor-summary-actions { display: none; }

  .editor-location {
    display: flex;
    align-items: center;
    gap: 12px;
    box-sizing: border-box;
    width: min(100%, 640px);
    min-height: 68px;
    margin: 0.75em 0;
    padding: 12px 14px;
    border: 1px solid var(--pad-border-color-100);
    border-radius: 12px;
    color: var(--pad-text-color);
    background: var(--pad-bg-color-200);

    .editor-location-icon {
      display: grid;
      flex: none;
      place-items: center;
      width: 38px;
      height: 38px;
      border-radius: 10px;
      background: color-mix(in srgb, var(--pad-primary-color) 16%, transparent);
      font-size: 21px;
    }

    .editor-location-body {
      flex: 1;
      min-width: 0;
    }

    .editor-location-name {
      overflow: hidden;
      color: var(--pad-text-color-100);
      font-weight: 600;
      line-height: 1.45;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    .editor-location-details {
      margin-top: 3px;
      overflow-wrap: anywhere;
      color: var(--pad-text-color-400);
      font-size: 12px;
      line-height: 1.4;
    }

    .editor-location-open {
      display: grid;
      flex: none;
      place-items: center;
      width: 34px;
      height: 34px;
      padding: 0;
      border: 0;
      border-radius: 9px;
      color: var(--pad-primary-dark);
      background: transparent;
      font-size: 21px;
      cursor: pointer;

      &:hover,
      &:focus-visible {
        background: color-mix(in srgb, var(--pad-primary-color) 14%, transparent);
      }

      &:disabled {
        opacity: 0.4;
        cursor: default;
      }
    }
  }

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

  .editor-audio-attachment {
    position: relative;
    display: grid;
    grid-template-columns: 46px minmax(0, 1fr) auto;
    align-items: center;
    gap: 12px;
    box-sizing: border-box;
    width: min(100%, var(--editor-audio-width, 220px));
    min-height: 68px;
    padding: 10px 14px;
    border: 1px solid var(--pad-border-color-200);
    border-radius: var(--pad-radius-xl);
    background: linear-gradient(
      135deg,
      var(--pad-bg-color-200),
      color-mix(in srgb, var(--pad-primary-color) 7%, var(--pad-bg-color-100))
    );
    box-shadow: var(--pad-shadow-sm);
    -webkit-touch-callout: none;
    user-select: none;
    -webkit-user-select: none;

    .editor-audio-play {
      display: grid;
      place-items: center;
      width: 44px;
      height: 44px;
      padding: 0;
      border: 0;
      border-radius: var(--pad-radius-full);
      color: var(--pad-text-color-light);
      background: var(--pad-primary-gradient);
      box-shadow: var(--pad-shadow-sm);
      cursor: pointer;
      transition: transform var(--pad-transition-fast), opacity var(--pad-transition-fast);

      &:active {
        transform: scale(0.94);
      }

      &:disabled {
        cursor: default;
        opacity: 0.55;
      }
    }

    .editor-audio-play-icon {
      padding-left: 2px;
      font-size: 15px;
      line-height: 1;
    }

    .editor-audio-waveform {
      min-width: 0;
      height: 44px;
      overflow: hidden;
      border-radius: var(--pad-radius-md);

      &--native {
        cursor: pointer;
      }
    }

    .editor-audio-native-waveform {
      display: block;
      width: 100%;
      height: 44px;
    }

    .editor-audio-time {
      min-width: 38px;
      color: var(--pad-text-color-300);
      font-size: 12px;
      font-variant-numeric: tabular-nums;
      white-space: nowrap;
    }

    .editor-audio-status {
      position: absolute;
      right: 14px;
      bottom: 2px;
      color: var(--pad-text-color-400);
      font-size: 10px;
    }

    .editor-audio-native-backend {
      display: none;
    }
  }

  audio[data-id]:not(.editor-audio-native-backend) {
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

  .ProseMirror > img[data-id] {
    padding-inline: 10px;
    max-width: calc(100% - 20px);
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
