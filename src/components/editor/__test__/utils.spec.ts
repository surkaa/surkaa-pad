import {replaceAttachmentMark} from "../utils.ts";
import {it, expect} from "vitest";

it('更改标记的文件名', () => {
    const original = '这是一个文件 [[FILE:oldName.txt]]，还有一个图片 [[IMG:oldName.txt|size=large]]。';
    const expected = '这是一个文件 [[FILE:newName.txt]]，还有一个图片 [[IMG:newName.txt|size=large]]。';
    const result = replaceAttachmentMark(original, 'oldName.txt', 'newName.txt');
    expect(result).toBe(expected);
});

it('更改标记的文件名包含特殊字符应该更新失败', () => {
    const original = '这是一个文件 [[FILE:oldName.txt]]，还有一个图片 [[IMG:oldName.txt|size=large]]。';
    const result = replaceAttachmentMark(original, 'oldName.txt', 'newName.txt]]');
    expect(result).toBe(null);
});