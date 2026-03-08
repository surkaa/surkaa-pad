import {Extension} from "./extension.ts";

export const BaseExtension: Extension = {
    name: "base",
    toSource: html => html
        // 空段落应映射为双换行符，补偿 Inline 和 Block 混排时的视觉间隔
        .replace(/<div><br\s*\/?>\s*<\/div>/gi, '\n\n')
        // 正常块级元素起手计为一个换行
        .replace(/<div>/gi, '\n')
        .replace(/<\/div>/gi, '')
        // 独立的行内换行
        .replace(/<br\s*\/?>/gi, '\n')
        .replace(/&nbsp;/g, ' ')
        // 剔除由于正则替换可能导致的首部冗余换行
        .replace(/^\n+/, ''),
    toHtml: source => source.replace(/\n/g, '<br/>') // 将换行符转换为<br/>
}
