export function replaceAttachmentMark(content: string, oldFilename: string, newFilename: string): string | null {
    // 新文件名不能包含 [[ 或者 ]]
    if (newFilename.indexOf('[[') !== -1 || newFilename.indexOf(']]') !== -1) {
        console.warn(`新文件名 "${newFilename}" 包含非法字符 [[ 或 ]]`);
        return null;
    }
    // 使用严格的正则匹配，避免误伤普通文本
    // 匹配: [[任意大写字母:oldFilename]] 或 [[任意大写字母:oldFilename|其他配置]]
    const regex = new RegExp(`\\[\\[([A-Z]+):${oldFilename}(?:\\|[^\\]]*)?\]\\]`, 'g');

    return content.replace(regex, (match, type) => {
        // 保持原有的 type (如 IMG, AUDIO) 和其他配置参数，只替换文件名
        console.log(`更新类型${type}的文件名: ${oldFilename} -> ${newFilename}`);
        return match.replace(oldFilename, newFilename);
    });
}
