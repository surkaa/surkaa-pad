export type DiaryLeaveBlocker = 'active-uploads' | 'unused-attachments' | null;

export interface DiaryLeaveState {
  isDeletingDiary: boolean;
  hasActiveUploads: boolean;
  unusedAttachmentCount: number;
}

export function getDiaryLeaveBlocker(state: DiaryLeaveState): DiaryLeaveBlocker {
  if (state.isDeletingDiary) return null;
  if (state.hasActiveUploads) return 'active-uploads';
  if (state.unusedAttachmentCount > 0) return 'unused-attachments';
  return null;
}
