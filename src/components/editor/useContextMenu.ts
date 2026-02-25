import { Menu, MenuItem } from '@tauri-apps/api/menu';
import { EXTENSIONS, ExtensionContext } from "./extension.ts";

export function useContextMenu(extensionCtx: ExtensionContext, handleInput: () => void) {
    async function handleEditorContextMenu(e: MouseEvent) {
        const target = e.target as HTMLElement;
        const handler = EXTENSIONS.find(ext => ext.match && ext.match(target));

        if (handler && handler.onContextmenu) {
            const buttons = handler.onContextmenu(e, target, extensionCtx);
            if (buttons && buttons.length > 0) {
                e.preventDefault(); // 拦截 Webview 默认菜单

                try {
                    // 动态映射生成 Tauri 原生 MenuItem 实例
                    const items = await Promise.all(
                        buttons.map(btn => MenuItem.new({
                            text: btn.label,
                            action: () => {
                                btn.action(target);
                                // 强制触发同步
                                handleInput();
                            }
                        }))
                    );

                    // 构建原生上下文菜单
                    const menu = await Menu.new({ items });

                    // 在当前系统鼠标绝对坐标弹出
                    await menu.popup();
                } catch (error) {
                    console.error('唤起系统原生菜单失败，请检查 Tauri Capabilities:', error);
                }
            }
        }
    }

    return {
        handleEditorContextMenu
    }
}
