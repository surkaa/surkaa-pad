import {Extension, MenuButton} from "./extension.ts";

export const ImageExtension: Extension<HTMLImageElement> = {
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

        let sizeAttr = '';
        if (configStr) {
            // 解析配置项
            const params = new URLSearchParams(configStr);
            if (params.get('size') === 'small') sizeAttr = ' data-size="small"';
        }

        return `<img loading="lazy" alt="${filename}" src="${url}" data-id="${filename}"${sizeAttr} />`;
    }),

    serialize: (node) => {
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
        const filename = node.dataset.id;
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
    onContextmenu: (_e, imgNode, ctx): MenuButton<HTMLImageElement>[] => {
        const isSmall = imgNode.dataset.size === 'small';
        const filename = imgNode.dataset.id;
        if (!filename) {
            console.error(`无法获取附件文件名，无法生成上下文菜单`);
            return [];
        }

        return [{
            label: isSmall ? '大图模式' : '小图模式',
            action: (target) => {
                if (isSmall) {
                    delete target.dataset.size; // 移除属性，恢复默认
                } else {
                    target.dataset.size = 'small'; // 设置属性
                }
            }
        }, {
            label: '顺时针旋转90°',
            icon: 'rotate_90_degrees_cw',
            action: () => ctx.emit("rotateAttachment", filename, 90),
        }, {
            label: '逆时针旋转90°',
            icon: 'rotate_90_degrees_ccw',
            action: () => ctx.emit("rotateAttachment", filename, -90),
        }, {
            label: '旋转180°',
            icon: 'cached',
            action: () => ctx.emit("rotateAttachment", filename, 180),
        }];
    },

    getFilename: (node) => node.dataset.id
}
