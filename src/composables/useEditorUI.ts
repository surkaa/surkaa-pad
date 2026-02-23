import {ref, watch} from 'vue';
import {platform} from "@tauri-apps/plugin-os";
import {useKeyboardShow} from "./useKeyboardShow.ts";

export function useEditorUI() {
    const showMenu = ref(false);
    const showToolbar = ref(false);
    const showToolbarPanel = ref(false);
    const showToolbarAfterMenu = ref(false);

    const setupToolbar = () => {
        const p = platform();
        if (p === 'android') {
            // 目前这个键盘只测试了安卓手机
            useKeyboardShow(showToolbar);
        } else {
            // 其他平台默认显示工具栏
            showToolbar.value = true;
        }
    };

    watch(showMenu, (newVal) => {
        if (newVal) {
            // 打开菜单时隐藏工具栏
            showToolbarAfterMenu.value = showToolbar.value;
            showToolbar.value = false;
            showToolbarPanel.value = false;
        } else {
            // 关闭菜单时恢复工具栏状态
            showToolbar.value = showToolbarAfterMenu.value;
        }
    });
    return {
        showMenu,
        showToolbar,
        showToolbarPanel,
        setupToolbar
    };
}
