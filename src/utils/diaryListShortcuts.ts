export type DiaryListShortcutAction = 'search' | 'settings';

export type DiaryListShortcutKeyboardEvent = Pick<
  KeyboardEvent,
  'altKey' | 'code' | 'ctrlKey' | 'metaKey' | 'shiftKey'
>;

export function findDiaryListShortcutAction(
  event: DiaryListShortcutKeyboardEvent,
): DiaryListShortcutAction | null {
  if (!event.ctrlKey || event.metaKey || event.altKey || event.shiftKey) return null;
  if (event.code === 'KeyF') return 'search';
  if (event.code === 'Comma') return 'settings';
  return null;
}

export function isEditableShortcutTarget(target: EventTarget | null): boolean {
  const element = target instanceof Element ? target : null;
  return Boolean(element?.closest('input, textarea, select, [contenteditable="true"]'));
}
