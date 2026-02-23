import {ref, watch} from 'vue';
import {platform} from "@tauri-apps/plugin-os";
import {useKeyboardShow} from "./useKeyboardShow.ts";
import {open} from "@tauri-apps/plugin-dialog"
import {AddAttachmentEvent, commands} from "../bindings.ts";
import {Channel} from "@tauri-apps/api/core";

export function useEditorUI(initialId: string) {
    const showMenu = ref(false);
    const showToolbar = ref(false);
    const showToolbarPanel = ref(false);
    const showToolbarAfterMenu = ref(false);
    const cancelTokens = ref<string[]>([]);

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

    const uploadAttachment = async (accessStr: string, mimetype: string, encrypted: boolean) => {
        const event = new Channel<AddAttachmentEvent>();
        let cancelToken = "";
        event.onmessage = (msg) => {
            switch (msg.event) {
                case "started":
                    console.log("开始上传");
                    break;
                case "progress":
                    console.log("百分制整数进度", msg.data);
                    break;
                case "completed":
                    console.log("上传完成，附件Meta", msg.data);
                    if (cancelToken) {
                        // 去掉cancelToken
                        const index = cancelTokens.value.indexOf(cancelToken);
                        if (index !== -1) {
                            cancelTokens.value.splice(index, 1);
                        }
                    }
                    break;
                case "error":
                    console.error("上传失败，错误信息", msg.data);
                    break;
            }
        };
        const res = await commands.cmdAddAttachment(event, initialId, accessStr, mimetype, encrypted);
        if (res.status == "error") {
            console.log("调用 Rust 后端失败", res.error);
            return;
        }
        cancelToken = res.data;
        cancelTokens.value.push(res.data);
    }

    // 占位功能
    const mediaActions = {
        insertPhoto: async () => {
            const accessStrArr = await open({multiple: true, pickerMode: 'image'});
            if (!accessStrArr) return;
            for (const accessStr in accessStrArr) {
                await uploadAttachment(accessStr, "image/*", true);
            }
        },
        takePhoto: async () => {
            // TODO
        },
        audioRecording: () => {
        },
        insertVideo: async () => {
            const accessStrArr = await open({multiple: true, pickerMode: 'video'});
            if (!accessStrArr) return;
            for (const accessStr in accessStrArr) {
                await uploadAttachment(accessStr, "video/*", true);
            }
        },
        takeVideo: () => {
            // TODO
        },
        insertFile: async () => {
            const accessStrArr = await open({multiple: true, pickerMode: 'document'});
            if (!accessStrArr) return;
            for (const accessStr in accessStrArr) {
                await uploadAttachment(accessStr, "document/*", true);
            }
        }
    };

    return {
        showMenu,
        showToolbar,
        cancelTokens,
        showToolbarPanel,
        setupToolbar,
        mediaActions
    };
}
