// @vitest-environment happy-dom
import {describe, expect, it} from 'vitest';
import {Editor} from '@tiptap/vue-3';
import StarterKit from '@tiptap/starter-kit';
import {ImageNode} from '../tiptap-extensions/ImageNode';
import {
  createBlockOrderTransaction,
  describeDiaryBlocks,
  isValidBlockOrder,
  moveDiaryBlock,
  topLevelBlockIdentities,
} from '../blockOrder';

describe('diary block ordering', () => {
  it('describes visual top-level blocks without exposing attachment URLs', () => {
    const document = {
      type: 'doc',
      content: [
        {type: 'heading', attrs: {level: 2}, content: [{type: 'text', text: '旅行记录'}]},
        {type: 'paragraph'},
        {type: 'imageNode', attrs: {id: 'att-image', src: 'http://secret-url'}},
        {
          type: 'albumNode',
          attrs: {id: 'album-1', images: ['att-a', 'att-b'], urls: ['a', 'b']},
        },
        {
          type: 'locationNode',
          attrs: {location: {placeName: '西湖', latitude: 30.2, longitude: 120.1}},
        },
      ],
    };

    expect(describeDiaryBlocks(document, [
      {id: 'att-image', filename: '照片.jpg'},
      {id: 'att-a', filename: '第一张.jpg'},
    ])).toEqual([
      expect.objectContaining({sourceIndex: 0, title: '标题 H2', preview: '旅行记录'}),
      expect.objectContaining({sourceIndex: 1, title: '段落', preview: '空白段落'}),
      expect.objectContaining({sourceIndex: 2, title: '图片', preview: '照片.jpg'}),
      expect.objectContaining({sourceIndex: 3, title: '图集', preview: '2 张图片 · 第一张.jpg'}),
      expect.objectContaining({sourceIndex: 4, title: '位置', preview: '西湖'}),
    ]);
  });

  it('moves blocks across arbitrary distances without mutating the source', () => {
    const source = ['a', 'b', 'c', 'd'];

    expect(moveDiaryBlock(source, 3, 1)).toEqual(['a', 'd', 'b', 'c']);
    expect(moveDiaryBlock(source, 0, 3)).toEqual(['b', 'c', 'd', 'a']);
    expect(moveDiaryBlock(source, -1, 2)).toEqual(source);
    expect(source).toEqual(['a', 'b', 'c', 'd']);
  });

  it('accepts only a complete permutation', () => {
    expect(isValidBlockOrder([2, 0, 1], 3)).toBe(true);
    expect(isValidBlockOrder([0, 0, 1], 3)).toBe(false);
    expect(isValidBlockOrder([0, 1], 3)).toBe(false);
    expect(isValidBlockOrder([0, 1, 3], 3)).toBe(false);
  });

  it('applies the final order as one undoable editor transaction', () => {
    const editor = new Editor({
      extensions: [StarterKit, ImageNode],
      content: {
        type: 'doc',
        content: [
          {type: 'paragraph', content: [{type: 'text', text: '第一段'}]},
          {type: 'imageNode', attrs: {id: 'att-1', src: 'url-1'}},
          {type: 'paragraph', content: [{type: 'text', text: '最后一段'}]},
        ],
      },
    });
    const original = editor.getJSON();
    const transaction = createBlockOrderTransaction(editor.state, [2, 0, 1]);

    expect(transaction).not.toBeNull();
    editor.view.dispatch(transaction!);
    expect(editor.getJSON().content?.map(node => node.type)).toEqual([
      'paragraph',
      'paragraph',
      'imageNode',
      'paragraph',
    ]);
    const reordered = editor.getJSON().content;
    expect(reordered?.[0]).toMatchObject({content: [{text: '最后一段'}]});
    expect(reordered?.[1]).toMatchObject({content: [{text: '第一段'}]});
    expect(reordered?.[2]?.attrs).toMatchObject({id: 'att-1', src: 'url-1'});

    expect(editor.commands.undo()).toBe(true);
    expect(editor.getJSON()).toEqual(original);
    editor.destroy();
  });

  it('rejects unchanged and invalid orders and keeps volatile attachment attrs out of identity', () => {
    const editor = new Editor({
      extensions: [StarterKit],
      content: '<p>第一段</p><p>第二段</p>',
    });
    expect(createBlockOrderTransaction(editor.state, [0, 1])).toBeNull();
    expect(createBlockOrderTransaction(editor.state, [1, 1])).toBeNull();

    expect(topLevelBlockIdentities({
      type: 'doc',
      content: [{type: 'imageNode', attrs: {id: 'att-1', src: 'old'}}],
    })).toEqual(topLevelBlockIdentities({
      type: 'doc',
      content: [{type: 'imageNode', attrs: {id: 'att-1', src: 'new'}}],
    }));
    editor.destroy();
  });
});
