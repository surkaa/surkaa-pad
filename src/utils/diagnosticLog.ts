import {getName} from '@tauri-apps/api/app';
import {BaseDirectory} from '@tauri-apps/api/path';
import {exists, readFile} from '@tauri-apps/plugin-fs';

export interface DiagnosticLogSnapshot {
    fileName: string;
    content: string;
}

export interface DiagnosticLogSource {
    getAppName: () => Promise<string>;
    exists: (fileName: string) => Promise<boolean>;
    read: (fileName: string) => Promise<Uint8Array>;
}

const defaultSource: DiagnosticLogSource = {
    getAppName: getName,
    exists: fileName => exists(fileName, {baseDir: BaseDirectory.AppLog}),
    read: fileName => readFile(fileName, {baseDir: BaseDirectory.AppLog}),
};

export async function loadDiagnosticLog(
    source: DiagnosticLogSource = defaultSource,
): Promise<DiagnosticLogSnapshot | null> {
    const appName = await source.getAppName();
    const fileName = `${appName}.log`;
    if (!await source.exists(fileName)) {
        return null;
    }

    const contents = await source.read(fileName);
    return {
        fileName,
        content: new TextDecoder().decode(contents),
    };
}
