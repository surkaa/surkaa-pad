import {Menu, MenuItem} from '@tauri-apps/api/menu';
import {platform} from '@tauri-apps/plugin-os';
import {useQuasar} from 'quasar';
import {ExtensionContext, EXTENSIONS, MenuButton} from "./extension.ts";

export function useContextMenu(extensionCtx: ExtensionContext, handleInput: () => void) {
    const $q = useQuasar();
    const currentPlatform = platform();

    async function handleEditorContextMenu(e: MouseEvent) {
        const target = e.target as HTMLElement;
        const handler = EXTENSIONS.find(ext => ext.match && ext.match(target));

        if (handler && handler.onContextmenu) {
            const buttons: MenuButton[] = [];
            const encrypted = handler.isEncrypted? handler.isEncrypted(target, extensionCtx) : false;
            buttons.push({
                label: `转成${encrypted ? '普通' : '加密'}附件`,
                action: t => {
                    // TODO 在后端编写转换逻辑
                    console.log('点击了转换附件按钮，目标元素：', t);
                }
            });
            buttons.push(...handler.onContextmenu(e, target, extensionCtx));

            if (!(buttons && buttons.length > 0)) {
                return;
            }

            e.preventDefault();
            if (currentPlatform === 'android') {
                $q.bottomSheet({
                    actions: buttons.map(btn => ({
                        label: btn.label,
                        icon: btn.icon,
                        id: btn.label // 透传标识符用于回调匹配
                    }))
                }).onOk(action => {
                    const btn = buttons.find(b => b.label === action.id);
                    if (btn) {
                        btn.action(target);
                        handleInput(); // 强制触发数据同步
                    }
                });
            } else if (currentPlatform === 'windows') {
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
                    const menu = await Menu.new({items});

                    // 在当前系统鼠标绝对坐标弹出
                    await menu.popup();
                } catch (error) {
                    console.error('唤起系统原生菜单失败:', error);
                }
            } else {
                console.error("当前平台不支持原生菜单");
            }
        }
    }

    return {
        handleEditorContextMenu
    }
}
