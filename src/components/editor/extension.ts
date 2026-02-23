import {AttachmentMeta} from "../../bindings.ts";
import {SupportType} from "../../utils/resolveMediaAttachmentUrl.ts";

// 定义菜单按钮的数据结构
export interface MenuButton {
    label: string;
    icon?: string;
    action: (target: HTMLElement | null) => void;
}

export interface ExtensionContext {
    getDiaryId(): string;
    getAttachment(filename: string): AttachmentMeta | null;
    gotoPreview(type: SupportType, diaryId: string, filename: string): void;
}

export interface Extension {
    name: string;

    // 样式类名，插件会自动添加到节点上
    style?: string;

    // 转换规则：HTML -> Source
    toSource?: (html: string) => string;

    // 转换规则：Source -> HTML
    toHtml?: (md: string, ctx: ExtensionContext) => string;

    // 交互钩子: 判断一个节点是否属于该插件
    match?: (node: Node) => boolean;

    // 单击回调
    onClick?: (e: MouseEvent, node: HTMLElement, ctx: ExtensionContext) => void;

    // TODO 解决 ctrl+z ctrl+y 删除又恢复时，附件可能会被删除但是无法恢复导致渲染失败
    // 删除回调
    onDeleted?: (node: Node, ctx: ExtensionContext) => void;

    // 上下文菜单 (windows右键/android长按) -> 返回要显示的菜单按钮列表
    onContextmenu?: (e: MouseEvent, node: HTMLElement, ctx: ExtensionContext) => MenuButton[];
}
