import type { Editor } from '@tiptap/vue-3'
import type { Ref, ShallowRef } from 'vue'
import { Menu, MenuItem } from '@tauri-apps/api/menu'
import { useQuasar } from 'quasar'
import type { AttachmentMeta } from '../bindings'
import {
  findAttachmentNode,
  type AttachmentNodeMatch,
} from '../components/editor/attachmentNode'
import { listAlbums, type AlbumSplitOperation } from '../components/editor/albumEditor'

interface EditorAlbumMenuActions {
  startAlbumSelection: (attachmentId: string) => void
  openAlbumInsertDialog: (attachmentId: string) => void
  changeAlbumMode: (albumId: string, mode: 'horizontalList' | 'stackedCards') => void
  applyAlbumSplit: (albumId: string, attachmentId: string, operation: AlbumSplitOperation) => void
  requestSingleImageSplit: (albumId: string, attachmentId: string) => void
  requestRangeSplit: (albumId: string, attachmentId: string) => void
}

interface AttachmentContextMenuOptions {
  editor: ShallowRef<Editor | undefined>
  editorElement: Ref<HTMLDivElement | undefined>
  currentPlatform: string
  getAttachment: (attachmentId: string) => AttachmentMeta | null
  attachmentUrl: (attachmentId: string) => string | undefined
  albumActions: EditorAlbumMenuActions
  toggleEncryption: (attachmentId: string) => void
  rotate: (attachmentId: string, rotation: number) => void
  rename: (
    attachmentId: string,
    filename: string,
    callback: (newFilename: string) => void,
  ) => void
  saveDecrypted: (attachmentId: string) => void
  showImage: (url: string) => void
}

interface MenuAction {
  label: string
  action: () => void
}

export function useAttachmentContextMenu(options: AttachmentContextMenuOptions) {
  const $q = useQuasar()

  async function handleContextMenu(event: MouseEvent) {
    const found = findAttachmentNode(event.target as HTMLElement, options.editorElement.value)
    if (!found) return
    event.preventDefault()

    const attachment = options.getAttachment(found.attachmentId)
    if (!attachment) return

    const buttons: MenuAction[] = [
      {
        label: `转成${attachment.encrypted ? '普通' : '加密'}附件`,
        action: () => options.toggleEncryption(found.attachmentId),
      },
      {
        label: '保存到本地',
        action: () => options.saveDecrypted(found.attachmentId),
      },
    ]

    if (found.type === 'image') {
      addImageActions(buttons, found)
    }
    if (found.type === 'file') {
      addRenameAction(buttons, found.attachmentId, attachment.filename)
    }

    await showMenu(buttons)
  }

  function addImageActions(
    buttons: MenuAction[],
    found: AttachmentNodeMatch,
  ) {
    const album = found.el.closest('.editor-image-album') as HTMLElement | null
    const isAlbumImage = Boolean(album)
    const isSmall = found.el.getAttribute('data-size') === 'small'
    if (!isAlbumImage) {
      buttons.push({
        label: isSmall ? '大图模式' : '小图模式',
        action: () => resizeImage(found.attachmentId, isSmall),
      })
    }
    buttons.push(
      { label: '顺时针旋转90°', action: () => options.rotate(found.attachmentId, 90) },
      { label: '逆时针旋转90°', action: () => options.rotate(found.attachmentId, -90) },
      { label: '旋转180°', action: () => options.rotate(found.attachmentId, 180) },
    )

    if (!isAlbumImage) {
      buttons.push({
        label: '创建图集',
        action: () => options.albumActions.startAlbumSelection(found.attachmentId),
      })
      if (options.editor.value && listAlbums(options.editor.value.getJSON()).length > 0) {
        buttons.push({
          label: '加入已有图集',
          action: () => options.albumActions.openAlbumInsertDialog(found.attachmentId),
        })
      }
      return
    }

    const albumId = album?.dataset.id || ''
    const currentMode = album?.dataset.displayMode
    if (currentMode === 'stackedCards') {
      buttons.push({
        label: '预览图片',
        action: () => {
          const url = options.attachmentUrl(found.attachmentId)
          if (url) options.showImage(url)
        },
      })
    }
    buttons.push({
      label: currentMode === 'stackedCards' ? '切换为横向图集' : '切换为堆叠图集',
      action: () => options.albumActions.changeAlbumMode(
        albumId,
        currentMode === 'stackedCards' ? 'horizontalList' : 'stackedCards',
      ),
    })
    buttons.push(
      {
        label: '拆分整个图集',
        action: () => options.albumActions.applyAlbumSplit(
          albumId,
          found.attachmentId,
          { type: 'all' },
        ),
      },
      {
        label: '仅拆分当前图片',
        action: () => options.albumActions.requestSingleImageSplit(albumId, found.attachmentId),
      },
      {
        label: '拆分当前及前后图片',
        action: () => options.albumActions.requestRangeSplit(albumId, found.attachmentId),
      },
    )
  }

  function resizeImage(attachmentId: string, isSmall: boolean) {
    options.editor.value?.commands.command(({ tr }) => {
      tr.doc.descendants((node, pos) => {
        if (node.attrs.id === attachmentId) {
          tr.setNodeMarkup(pos, undefined, {
            ...node.attrs,
            size: isSmall ? null : 'small',
          })
        }
      })
      return true
    })
  }

  function addRenameAction(buttons: MenuAction[], attachmentId: string, filename: string) {
    buttons.push({
      label: '重命名附件',
      action: () => options.rename(attachmentId, filename, (newFilename: string) => {
        options.editor.value?.commands.command(({ tr }) => {
          tr.doc.descendants((node, pos) => {
            if (node.attrs.id === attachmentId) {
              tr.setNodeMarkup(pos, undefined, { ...node.attrs, filename: newFilename })
            }
          })
          return true
        })
      }),
    })
  }

  async function showMenu(buttons: MenuAction[]) {
    if (options.currentPlatform === 'android') {
      let selectedAction: MenuAction | undefined
      $q.bottomSheet({
        actions: buttons.map(button => ({ label: button.label, id: button.label })),
      }).onOk((action: { id: string }) => {
        selectedAction = buttons.find(button => button.label === action.id)
      }).onDismiss(() => {
        selectedAction?.action()
      })
      return
    }

    try {
      const items = await Promise.all(
        buttons.map(button => MenuItem.new({ text: button.label, action: button.action })),
      )
      const menu = await Menu.new({ items })
      await menu.popup()
    } catch (error) {
      console.error('上下文菜单失败:', error)
    }
  }

  return { handleContextMenu }
}
