import {ref} from "vue";
import {MenuButton} from "../../types";
import {ExtensionContext, EXTENSIONS} from "./extension.ts";

export function useContextMenu(extensionCtx: ExtensionContext, handleInput: () => void) {
    const contextMenuState = ref({
        visible: false,
        x: 0,
        y: 0,
        buttons: [] as MenuButton[],
        targetNode: null as HTMLElement | null
    });

    // 处理右键菜单事件
    function handleEditorContextMenu(e: MouseEvent) {
        const target = e.target as HTMLElement;
        const handler = EXTENSIONS.find(ext => ext.match && ext.match(target));

        if (handler && handler.onContextmenu) {
            const buttons = handler.onContextmenu(e, target, extensionCtx);
            if (buttons && buttons.length > 0) {
                e.preventDefault(); // 拦截浏览器默认右键菜单
                contextMenuState.value = {
                    visible: true,
                    x: e.clientX, // 使用 clientX/Y 配合 fixed 定位
                    y: e.clientY,
                    buttons,
                    targetNode: target
                };
            }
        } else {
            // 点击到非扩展节点，隐藏已有菜单并允许浏览器默认行为
            closeContextMenu();
        }
    }

    // 执行菜单动作
    function executeMenuAction(btn: MenuButton) {
        if (contextMenuState.value.targetNode) {
            btn.action(contextMenuState.value.targetNode);
            // 强制触发同步：因为 action 修改了 DOM dataset 属性，不会触发浏览器的 input 事件
            handleInput();
        }
        closeContextMenu();
    }

    function closeContextMenu() {
        contextMenuState.value.visible = false;
    }

    return {
        contextMenuState,
        handleEditorContextMenu,
        executeMenuAction,
        closeContextMenu
    }
}