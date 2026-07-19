import {ref, watch} from 'vue';
import {platform} from "@tauri-apps/plugin-os";

export function useEditorUI() {
    const showMenu = ref(false);
    const showToolbar = ref(false);
    const showToolbarPanel = ref(false);
    const showToolbarAfterMenu = ref(false);

    const setupToolbar = () => {
        const p = platform();
        if (p !== 'android') {
            // 其他平台默认显示工具栏
            showToolbar.value = true;
        }
    };

    // Android 上首次聚焦编辑器后显示工具栏，不再随键盘收起而隐藏。
    const showToolbarAfterEditorFocus = () => {
        if (platform() === 'android') {
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
        setupToolbar,
        showToolbarAfterEditorFocus,
    };
}
