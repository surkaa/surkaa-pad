import {Extension, MenuButton} from "./extension.ts";

// TODO 考虑使用泛型避免频繁的类型断言
export const ImageExtension: Extension = {
    name: "image",

    match: (node) => node.nodeName === 'IMG',

    getMark: (filename) => `[[IMG:${filename}]]`,

    hasMark: (source, filename) => new RegExp(`\\[\\[IMG:${filename}(?:\\|[^\\]]*)?\]\\]`).test(source),

    toHtml: (source, ctx) => source.replace(/\[\[IMG:([^\]|]+)(?:\|([^\]]+))?]]/g, (_, filename, configStr) => {
        const url = ctx.getAttachmentUrl(filename);
        if (!url) {
            console.error(`无法解析图片URL，因为没有找到附件 ${filename}`);
            return '';
        }
        const img = document.createElement('img');
        img.src = url;
        img.dataset.id = filename;
        // 解析配置项
        if (configStr) {
            const params = new URLSearchParams(configStr);
            if (params.get('size') === 'small') {
                img.dataset.size = 'small'; // 使用 data-size 属性标记小图模式
            }
        }

        return img.outerHTML;
    }),

    serialize: (n: HTMLElement) => {
        const node = n as HTMLImageElement;
        const filename = node.dataset.id;
        if (!filename) return '';

        const params = new URLSearchParams();
        if (node.dataset.size === 'small') {
            params.append('size', 'small');
        }

        const configStr = params.toString();
        return configStr ? `[[IMG:${filename}|${configStr}]]` : `[[IMG:${filename}]]`;
    },

    onClick: (_e, node, ctx) => {
        const filename = (node as HTMLImageElement).dataset.id;
        if (!filename) {
            console.error(`无法打开附件，因为没有找到 data-id 属性`);
            return;
        }
        const src = ctx.getAttachmentUrl(filename);
        if (!src) {
            console.error(`没有找到附件src ${filename}`);
            return;
        }
        ctx.gotoPreview(src);
    },

    // 右键菜单实现
    onContextmenu: (_e, node, _ctx): MenuButton[] => {
        const imgNode = node as HTMLImageElement;
        const isSmall = imgNode.dataset.size === 'small';

        // 辅助函数：更新旋转并同步样式
        const setRotation = (target: HTMLImageElement, dRotation: number) => {
            const currentRotation = parseInt(target.dataset.rotation || '0', 10);
            const newRot = currentRotation + dRotation;
            target.dataset.rotation = newRot.toString();
            target.style.transform = `rotate(${newRot}deg)`;
        };

        return [{
            label: isSmall ? '大图模式' : '小图模式',
            action: (targetEl) => {
                const target = targetEl as HTMLImageElement;
                if (isSmall) {
                    delete target.dataset.size; // 移除属性，恢复默认
                } else {
                    target.dataset.size = 'small'; // 设置属性
                }
            }
        }, {
            label: '顺时针旋转90°',
            icon: 'rotate_90_degrees_cw',
            action: (targetEl) => {
                const target = targetEl as HTMLImageElement;
                setRotation(target, 90);
            }
        }, {
            label: '逆时针旋转90°',
            icon: 'rotate_90_degrees_ccw',
            action: (targetEl) => {
                const target = targetEl as HTMLImageElement;
                setRotation(target, -90);
            }
        }, {
            label: '旋转180°',
            icon: 'replay_180',
            action: (targetEl) => {
                const target = targetEl as HTMLImageElement;
                setRotation(target, 180);
            }
        }];
    },

    isEncrypted: (node, ctx) => {
        const filename = (node as HTMLImageElement).dataset.id;
        if (!filename) return false;
        const attachment = ctx.getAttachment(filename);
        return attachment ? attachment.encrypted : false;
    },

    getFilename: (node) => (node as HTMLImageElement).dataset.id
}
