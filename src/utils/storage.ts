import {BaseDirectory, create, writeFile} from "@tauri-apps/plugin-fs";
import {appDataDir, join} from "@tauri-apps/api/path";

export async function joinAndCreateLocalRecordingDir(diaryId: string, stream: ReadableStream<Uint8Array>) {
    const filename = `audio_cache/${diaryId}_${new Date().getTime()}.tmp`;
    await create("audio_cache", {baseDir: BaseDirectory.AppData});
    await writeFile(filename, stream, {
        baseDir: BaseDirectory.AppData
    });
    const appDataPath = await appDataDir();
    return join(appDataPath, filename);
}