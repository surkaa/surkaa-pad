// @vitest-environment happy-dom
import {afterEach, describe, expect, it} from 'vitest';
import {Editor} from '@tiptap/vue-3';
import TaskList from '@tiptap/extension-task-list';
import StarterKit from '@tiptap/starter-kit';
import {DiaryDocument} from '../editorSchema';
import {createTaskItemExtension} from '../taskItem';
import {createDiaryTableExtensions} from '../tableEditing';
import {ImageNode, VideoNode, AudioNode, FileNode, AlbumNode, SummaryNode, LocationNode} from '../tiptap-extensions';
import {diaryContentToHtml, htmlToDiaryContent, markdownToHtml} from '../markdownConverter';
import {insertImageAndMoveCursorAfter} from '../imageInsertion';

let editor: Editor | undefined;
afterEach(() => { editor?.destroy(); editor = undefined; });
function createEditor(content: string) {
  editor = new Editor({extensions: [
    StarterKit.configure({document: false}), DiaryDocument,
    TaskList, createTaskItemExtension(true).configure({nested: true}),
    ...createDiaryTableExtensions(),
    ...[ImageNode, VideoNode, AudioNode, FileNode, AlbumNode, SummaryNode, LocationNode]
      .map(node => node.extend({group: 'diaryBlock'})),
  ], content});
  return editor;
}

describe('structured diary block boundaries', () => {
  it.each(['listItem', 'taskItem', 'tableCell', 'tableHeader'])('does not allow attachment blocks inside %s', name => {
    const current = createEditor('<p></p>');
    const paragraph = current.schema.nodes.paragraph.create();
    const image = current.schema.nodes.imageNode.create({id: 'image'});
    expect(current.schema.nodes[name].createAndFill({}, [paragraph, image])).toBeNull();
    expect(current.schema.nodes.doc.createAndFill({}, [paragraph, image])).not.toBeNull();
  });

  it.each(['- 第一项\n- 第二项', '- [ ] 第一项\n- [ ] 第二项'])
    ('inserting a photo inside a list preserves it as a top-level attachment: %s', source => {
      const current = createEditor(markdownToHtml(source) + '<p></p>');
      let position = 0;
      current.state.doc.descendants((node, pos) => { if (node.isText && node.text === '第一项') position = pos + 1; });
      current.commands.setTextSelection(position);
      expect(current.commands.insertContent({type: 'imageNode', attrs: {id: 'image', src: '', size: 'normal'}})).toBe(true);
      let images = 0;
      current.state.doc.descendants((node, _pos, parent) => {
        if (node.type.name === 'imageNode') { images++; expect(parent?.type.name).toBe('doc'); }
      });
      expect(images).toBe(1);
      expect(current.state.doc.textContent).toContain('第二项');
      const saved = htmlToDiaryContent(current.getHTML());
      expect(saved.nodes).toContainEqual({type: 'image', attachmentId: 'image', size: 'normal'});
      current.commands.setContent(diaryContentToHtml(saved, {}));
      expect(htmlToDiaryContent(current.getHTML())).toEqual(saved);
    });

  it('keeps ordinary tables available inside nested text lists', () => {
    const current = createEditor(markdownToHtml('- [ ] 内容\n\n  | A | B |\n  | --- | --- |\n  | 1 | 2 |') + '<p></p>');
    expect(current.state.doc.child(0).child(0).child(1).type.name).toBe('table');
    const before = current.getJSON();
    const saved = htmlToDiaryContent(current.getHTML());
    current.commands.setContent(diaryContentToHtml(saved, {}));
    expect(current.getJSON()).toEqual(before);
  });

  it('keeps consecutive camera insertions separate when starting inside a list', () => {
    const current = createEditor(markdownToHtml('- 第一项\n- 第二项') + '<p></p>');
    current.commands.setTextSelection(4);
    expect(insertImageAndMoveCursorAfter(current, {id: 'photo-1', src: ''})).toBe(true);
    expect(insertImageAndMoveCursorAfter(current, {id: 'photo-2', src: ''})).toBe(true);
    const saved = htmlToDiaryContent(current.getHTML());
    expect(saved.nodes.filter(node => node.type === 'image')).toEqual([
      {type: 'image', attachmentId: 'photo-1', size: 'normal'},
      {type: 'image', attachmentId: 'photo-2', size: 'normal'},
    ]);
  });
});
