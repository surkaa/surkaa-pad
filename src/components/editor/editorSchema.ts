import {Node as TiptapNode} from '@tiptap/vue-3';

/** 结构化日记块单独分组，仅允许放在文档顶层，不进入 Markdown 列表和表格。 */
export const DiaryDocument = TiptapNode.create({
  name: 'doc',
  topNode: true,
  addOptions() { return {summary: false}; },
  content() {
    return this.options.summary
      ? '(block|diaryBlock|summaryBlock)+' : '(block|diaryBlock)+';
  },
});
