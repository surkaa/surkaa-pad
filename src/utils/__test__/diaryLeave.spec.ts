import {describe, expect, it} from 'vitest';
import {getDiaryLeaveBlocker} from '../diaryLeave';

describe('getDiaryLeaveBlocker', () => {
  it('allows leaving after deleting the diary', () => {
    expect(getDiaryLeaveBlocker({
      isDeletingDiary: true,
      hasActiveUploads: true,
      unusedAttachmentCount: 2,
    })).toBeNull();
  });

  it('handles active uploads before unused attachments', () => {
    expect(getDiaryLeaveBlocker({
      isDeletingDiary: false,
      hasActiveUploads: true,
      unusedAttachmentCount: 2,
    })).toBe('active-uploads');
  });

  it('checks unused attachments after uploads settle', () => {
    expect(getDiaryLeaveBlocker({
      isDeletingDiary: false,
      hasActiveUploads: false,
      unusedAttachmentCount: 2,
    })).toBe('unused-attachments');
  });
});
