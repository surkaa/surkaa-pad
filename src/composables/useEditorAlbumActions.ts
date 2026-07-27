import type { Editor } from '@tiptap/vue-3'
import { nextTick, ref, type Ref, type ShallowRef } from 'vue'
import { useQuasar } from 'quasar'
import {
  addImageToAlbumDocument,
  changeAlbumDisplayMode,
  createAlbumDocument,
  listAlbums,
  splitAlbumDocument,
  type AlbumSplitOperation,
  type AlbumSummary,
} from '../components/editor/albumEditor'
import { isMobileEditorPlatform } from '../components/editor/editorClick'

interface EditorAlbumActionsOptions {
  editor: ShallowRef<Editor | undefined>
  editorElement: Ref<HTMLDivElement | undefined>
  currentPlatform: string
  attachmentMap: () => Record<string, string>
}

export function useEditorAlbumActions(options: EditorAlbumActionsOptions) {
  const $q = useQuasar()
  const albumSelection = ref<string[]>([])
  const albumAnchor = ref('')
  const showAlbumInsertDialog = ref(false)
  const albumInsertSource = ref('')
  const albumInsertTargets = ref<AlbumSummary[]>([])

  function cycleStackedAlbum(albumId: string) {
    if (!options.editor.value || !albumId) return
    options.editor.value.commands.command(({ tr }) => {
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
    if (!options.editor.value || !albumId) return
    options.editor.value.commands.setContent(
      changeAlbumDisplayMode(options.editor.value.getJSON(), albumId, displayMode),
    )
  }

  function startAlbumSelection(filename: string) {
    albumAnchor.value = filename
    albumSelection.value = [filename]
    if (isMobileEditorPlatform(options.currentPlatform)) {
      options.editor.value?.commands.blur()
      nextTick(() => options.editor.value?.commands.blur())
    }
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
      options.editorElement.value
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
    if (!options.editor.value || albumSelection.value.length < 2) return
    const nextDocument = createAlbumDocument(
      options.editor.value.getJSON(),
      albumSelection.value,
      albumAnchor.value,
      crypto.randomUUID(),
      displayMode,
      options.attachmentMap(),
    )
    cancelAlbumSelection()
    options.editor.value.commands.setContent(nextDocument)
  }

  function openAlbumInsertDialog(filename: string) {
    if (!options.editor.value) return
    const albums = listAlbums(options.editor.value.getJSON())
    if (albums.length === 0) {
      $q.notify({ type: 'info', message: '当前日记中没有可加入的图集' })
      return
    }
    albumInsertSource.value = filename
    albumInsertTargets.value = albums
    showAlbumInsertDialog.value = true
  }

  function insertImageIntoAlbum(albumId: string, insertionIndex: number) {
    if (!options.editor.value || !albumInsertSource.value) return
    options.editor.value.commands.setContent(addImageToAlbumDocument(
      options.editor.value.getJSON(),
      albumInsertSource.value,
      albumId,
      insertionIndex,
      options.attachmentMap(),
    ))
    albumInsertSource.value = ''
    albumInsertTargets.value = []
  }

  function applyAlbumSplit(
    albumId: string,
    filename: string,
    operation: AlbumSplitOperation,
  ) {
    if (!options.editor.value) return
    options.editor.value.commands.setContent(splitAlbumDocument(
      options.editor.value.getJSON(),
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

  return {
    albumSelection,
    albumAnchor,
    showAlbumInsertDialog,
    albumInsertTargets,
    cycleStackedAlbum,
    changeAlbumMode,
    startAlbumSelection,
    toggleAlbumImage,
    cancelAlbumSelection,
    confirmAlbum,
    openAlbumInsertDialog,
    insertImageIntoAlbum,
    applyAlbumSplit,
    requestSingleImageSplit,
    requestRangeSplit,
  }
}
