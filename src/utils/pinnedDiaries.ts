/** index 是移除被拖动项后，在剩余置顶日记中的插入位置。 */
export function reorderPinnedDiaryIds(ids: readonly string[], diaryId: string, index: number): string[] {
  const next = [...ids];
  const current = next.indexOf(diaryId);
  if (current < 0 || !Number.isFinite(index)) return next;
  const target = Math.max(0, Math.min(next.length - 1, Math.trunc(index)));
  if (target === current) return next;
  next.splice(current, 1);
  next.splice(target, 0, diaryId);
  return next;
}
