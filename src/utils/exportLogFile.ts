import {appLogDir, join} from "@tauri-apps/api/path";
import {save} from "@tauri-apps/plugin-dialog";
import {copyFile} from "@tauri-apps/plugin-fs";
import {getName} from "@tauri-apps/api/app";

export async function exportLogFile() {
    try {
        const logDir = await appLogDir();
        const name = await getName();
        const sourceFile = await join(logDir, `${name}.log`);

        const savePath = await save({
            title: "导出系统日志",
            defaultPath: `${name}-export.log`,
            filters: [{name: "Log", extensions: ["log"]}]
        });

        if (savePath) {
            await copyFile(sourceFile, savePath);
            console.log("日志导出成功:", savePath);
        }
    } catch (e) {
        console.error("导出失败:", e);
    }
}