import type {Node as ProseMirrorNode} from '@tiptap/pm/model';
import {TextSelection, type EditorState, type Transaction} from '@tiptap/pm/state';
import type {SummaryAttributes} from './tiptap-extensions/SummaryNode';

export interface PlainTextSelection {
  from: number;
  to: number;
  text: string;
}

export interface SummaryTarget extends SummaryAttributes {
  position: number;
}

function readPlainTextRange(
  document: ProseMirrorNode,
  from: number,
  to: number,
): PlainTextSelection | null {
  if (from >= to || from < 0 || to > document.content.size) return null;

  let containsNonTextLeaf = false;
  document.nodesBetween(from, to, node => {
    if (node.isLeaf && !node.isText && node.type.name !== 'hardBreak') {
      containsNonTextLeaf = true;
      return false;
    }
    return !containsNonTextLeaf;
  });
  if (containsNonTextLeaf) return null;

  const text = document.textBetween(from, to, '\n', '\n');
  if (!text.trim()) return null;
  return {from, to, text};
}

export function readPlainTextSelection(state: EditorState): PlainTextSelection | null {
  if (!(state.selection instanceof TextSelection) || state.selection.empty) return null;
  return readPlainTextRange(state.doc, state.selection.from, state.selection.to);
}

export function listSummaryTargets(document: ProseMirrorNode): SummaryTarget[] {
  const targets: SummaryTarget[] = [];
  document.descendants((node, position) => {
    if (node.type.name !== 'summaryNode') return;
    targets.push({
      position,
      summary: node.attrs.summary,
      content: node.attrs.content,
    });
  });
  return targets;
}

function restoreSelection(
  state: EditorState,
  source: PlainTextSelection,
): Transaction | null {
  const current = readPlainTextRange(state.doc, source.from, source.to);
  if (!current || current.text !== source.text) return null;
  return state.tr.setSelection(TextSelection.create(state.doc, source.from, source.to));
}

export function replaceSelectionWithSummary(
  state: EditorState,
  source: PlainTextSelection,
  attrs: SummaryAttributes,
): Transaction | null {
  const transaction = restoreSelection(state, source);
  const summaryNode = state.schema.nodes.summaryNode;
  if (!transaction || !summaryNode) return null;
  return transaction.replaceSelectionWith(summaryNode.create(attrs));
}

export function appendSelectionToSummary(
  state: EditorState,
  source: PlainTextSelection,
  targetPosition: number,
): Transaction | null {
  const transaction = restoreSelection(state, source);
  const target = state.doc.nodeAt(targetPosition);
  if (!transaction || target?.type.name !== 'summaryNode') return null;

  const previousContent = String(target.attrs.content ?? '').trimEnd();
  const content = previousContent ? `${previousContent}\n${source.text}` : source.text;
  transaction.setNodeMarkup(targetPosition, undefined, {...target.attrs, content});
  return transaction.deleteSelection();
}
