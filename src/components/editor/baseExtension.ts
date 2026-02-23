import {Extension} from "./extension.ts";

export const BaseExtension: Extension = {
    name: "base",
    toSource: html => html
        .replace(/<div><br><\/div>/g, '\n') // 处理空行
        .replace(/<div>/g, '\n') // 将<div>转换为换行符
        .replace(/<\/div>/g, '') // 移除</div>标签
        .replace(/<br\s*\/?>/g, '\n').replace(/&nbsp;/g, ' '), // 将<br>转换为换行符，并将&nbsp;转换为空格
    toHtml: source => source.replace(/\n/g, '<br/>') // 将换行符转换为<br/>
}
