import {DiaryManifest} from "../types";
import {BaseDirectory, writeFile} from "@tauri-apps/plugin-fs";
import {invoke} from "@tauri-apps/api/core";

export async function saveAttachment(
    uuid: string,
    minetype: string,
    stream: ReadableStream
): Promise<DiaryManifest> {
    // 构造临时文件名/路径
    const filename = `${uuid}_${new Date().getTime()}.tmp`;

    console.log('临时文件名', filename);

    // 将文件写入应用数据目录或临时目录
    await writeFile(filename, stream, {
        baseDir: BaseDirectory.Temp
    });

    return await invoke<DiaryManifest>("add_attachment", {
        uuid, minetype, filename,
    });
}