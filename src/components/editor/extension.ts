import {AttachmentMeta} from "../../bindings.ts";
import {ImageExtension} from "./imageExtension.ts";
import {AudioExtension} from "./audioExtension.ts";
import {VideoExtension} from "./videoExtension.ts";
import {BaseExtension} from "./baseExtension.ts";
import {FileExtension} from "./fileExtension.ts";

// 定义菜单按钮的数据结构
export interface MenuButton {
    label: string;
    icon?: string;
    action: (target: HTMLElement | null) => void;
}

export interface ExtensionContext {
    getDiaryId(): string;
    getAttachment(filename: string): AttachmentMeta | null;
    getAttachmentUrl(filename: string): string | null;
    gotoPreview(src: string, rotation?: string): void;
    emit: {
        rotateAttachment(filename: string, rotation: number): void;
    }
}

export interface Extension {
    name: string;

    // 转换规则：HTML -> Source
    toSource?: (html: string) => string;

    // 安全 DOM 节点级反解析
    serialize?: (node: HTMLElement) => string;

    // 转换规则：Source -> HTML
    toHtml?: (md: string, ctx: ExtensionContext) => string;

    // 交互钩子: 判断一个节点是否属于该插件
    match?: (node: Node) => boolean;

    // 获取标记
    getMark?: (filename: string) => string;

    // 用于带配置项标记的安全校验
    hasMark?: (source: string, filename: string) => boolean;

    // 单击回调
    onClick?: (e: MouseEvent, node: HTMLElement, ctx: ExtensionContext) => void;

    // 上下文菜单 (windows右键/android长按) -> 返回要显示的菜单按钮列表
    onContextmenu?: (e: MouseEvent, node: HTMLElement, ctx: ExtensionContext) => MenuButton[];

    // 是否是加密了的附件
    isEncrypted?: (node: HTMLElement, ctx: ExtensionContext) => boolean;

    // 获取附件的文件名
    getFilename?: (node: HTMLElement) => string | undefined;
}

export const EXTENSIONS: Extension[] = [
    ImageExtension,
    AudioExtension,
    VideoExtension,
    FileExtension,
    BaseExtension
] as const;
