import {watch, type Ref} from 'vue';
import {useQuasar} from 'quasar';
import {useEditorContextMenu} from './useEditorContextMenu';
import {installEditorLongPress} from '../components/editor/editorLongPress';
import {copyTextToClipboard} from '../utils/clipboard';

interface DiaryListMenuOptions {
  currentPlatform: string;
  pinnedDiaryIds: Ref<string[]>;
  enabled: () => boolean;
  isAvailable: (id: string) => boolean;
}

/** 委托给可见卡片容器，避免为每张卡片或隐藏的高度测量副本安装全局监听。 */
export function useDiaryListContextMenu(container: Ref<HTMLElement | null>, options: DiaryListMenuOptions) {
  const $q = useQuasar();
  const {showMenu, closeMenu} = useEditorContextMenu(options.currentPlatform);
  const mobile = options.currentPlatform === 'android' || options.currentPlatform === 'ios';
  const available = (id: string) => options.enabled() && options.isAvailable(id);

  function resolve(target: EventTarget | null): string | null {
    const card = target instanceof Element ? target.closest<HTMLElement>('[data-diary-id]') : null;
    const id = card?.dataset.diaryId;
    return card && container.value?.contains(card) && id && available(id) ? id : null;
  }

  function open(id: string) {
    if (!available(id)) return;
    const pin = !options.pinnedDiaryIds.value.includes(id);
    void showMenu([[
      {
        label: pin ? '置顶' : '取消置顶',
        action: () => {
          if (!available(id)) return;
          // 按菜单上显示的意图设置，而不是再次 toggle；保留期间从设置/云端收到的其他置顶。
          const latest = options.pinnedDiaryIds.value;
          if (latest.includes(id) === pin) return;
          options.pinnedDiaryIds.value = pin ? [...latest, id] : latest.filter(value => value !== id);
          $q.notify({type: 'positive', message: pin ? '已置顶该日记' : '已取消置顶该日记'});
        },
      },
      {
        label: '复制日记 ID',
        action: async () => {
          if (!available(id)) return;
          try {
            await copyTextToClipboard(id);
            $q.notify({type: 'positive', message: '已复制日记 ID'});
          } catch (error) {
            console.error('复制日记 ID 失败:', error);
            $q.notify({type: 'negative', message: '复制日记 ID 失败，请重试'});
          }
        },
      },
    ]]);
  }

  watch([container, options.enabled], ([element, enabled], _previous, onCleanup) => {
    if (!element || !enabled) { closeMenu(); return; }
    const context = (event: MouseEvent) => {
      const id = resolve(event.target);
      if (!id) return;
      event.preventDefault();
      event.stopPropagation();
      if (!mobile) open(id); // 移动端由长按处理，避免 contextmenu 重复弹出。
    };
    element.addEventListener('contextmenu', context);
    const disposeLongPress = mobile ? installEditorLongPress(element, resolve, open) : undefined;
    onCleanup(() => {
      disposeLongPress?.();
      element.removeEventListener('contextmenu', context);
      closeMenu();
    });
  }, {immediate: true, flush: 'post'});
}
