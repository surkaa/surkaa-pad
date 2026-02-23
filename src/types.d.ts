export type OssConfigType = {
    akid: string;
    aks: string;
    bucket: string;
    endpoint: string;
}

export type ThemeType = 'light' | 'dark' | 'system';

// 定义菜单按钮的数据结构
export interface MenuButton {
    label: string;
    icon?: string;
    action: (target: HTMLElement | null) => void;
}

// 定义编辑器插件的接口
export interface EditorExtension {
    name: string;

    // 插件自带 CSS 字符串
    style?: string;

    // 工具栏配置
    icon?: string;
    title?: string;
    action?: (editor: HTMLDivElement) => void;

    // 转换规则：HTML -> Markdown
    toMarkdown?: (html: string) => string;

    // 转换规则：Markdown -> HTML
    toHtml?: (md: string) => string;

    // 交互钩子: 判断一个节点是否属于该插件 (例如 IMG 标签属于 MediaExtensions)
    match?: (node: Node) => boolean;

    // 单击回调
    onClick?: (e: MouseEvent, node: HTMLElement) => void;

    // 删除回调
    onDelete?: (node: Node) => void;

    // 上下文菜单 (长按/右键) -> 返回要显示的菜单按钮列表
    onContextmenu?: (e: MouseEvent, node: HTMLElement) => MenuButton[];
}
