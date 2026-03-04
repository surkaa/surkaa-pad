import {BaseDirectory} from "@tauri-apps/api/path";
import {save} from "@tauri-apps/plugin-dialog";
import {exists, readFile, writeFile} from "@tauri-apps/plugin-fs";
import {getName} from "@tauri-apps/api/app";

export async function exportLogFile() {
    try {
        const name = await getName();
        const sourceFile = `${name}.log`;

        if (!await exists(sourceFile, {baseDir: BaseDirectory.AppLog})) {
            console.log('日志文件不存在:', sourceFile);
            return;
        }

        const savePath = await save({
            title: "导出系统日志",
            defaultPath: `${name}-export.log`,
            filters: [{name: "Log", extensions: ["log"]}]
        });

        if (savePath) {
            console.log('选择的保存路径:', savePath);
            const contents = await readFile(sourceFile, { baseDir: BaseDirectory.AppLog });
            await writeFile(savePath, contents);
            console.log("日志导出成功:", savePath);
        } else {
            console.log("用户取消了导出操作");
        }
    } catch (e) {
        console.error("导出失败:", e);
    }
}