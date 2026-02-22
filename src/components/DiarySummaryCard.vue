<script setup lang="ts">
import {formatBytes, formatTimestamp, getCurEmoji} from "../utils";
import {AttachmentMeta, DiarySummary} from "../bindings.ts";

const {diary} = defineProps<{
  diary: DiarySummary | null;
}>();

// 格式化附件信息
function getAttachmentInfo(attachments?: AttachmentMeta[]) {
  if (!attachments || attachments.length === 0) return null;

  const totalSize = attachments.reduce((sum, att) => sum + (att.size || 0), 0);
  const imageCount = attachments.filter(att => att.mimetype.includes('image')).length;
  const otherCount = attachments.length - imageCount;

  return {
    count: attachments.length,
    totalSize,
    imageCount,
    otherCount
  };
}
</script>

<template>
  <div class="diary-card">
    <div class="card-header">
      <div class="date-group">
        <span class="date-primary">
          <svg viewBox="0 0 24 24" width="14" height="14">
            <path
                d="M20 3h-1V1h-2v2H7V1H5v2H4c-1.1 0-2 .9-2 2v16c0 1.1.9 2 2 2h16c1.1 0 2-.9 2-2V5c0-1.1-.9-2-2-2zm0 18H4V8h16v13z"/>
          </svg>
          {{ formatTimestamp(diary?.created) }}
        </span>
        <span
            class="date-updated"
            v-if="diary?.updated && diary?.created && (diary.updated > diary.created)"
            title="最后更新"
        >
          <span class="update-icon">{{ getCurEmoji(diary?.updated) }}</span>
          {{ formatTimestamp(diary?.updated) }}
        </span>
      </div>

      <div class="card-actions">
        <span
            class="attachment-badge"
            v-if="diary?.attachments.length"
            :title="`${diary?.attachments.length} 个附件`"
        >
          <span class="badge-icon">📎</span>
          <span class="badge-count">{{ diary?.attachments.length }}</span>
        </span>
      </div>
    </div>

    <div class="card-content">
      <p class="preview-content">
        {{ diary?.title || '（无内容预览）' }}
      </p>
    </div>

    <div class="card-footer">
      <div class="meta-info">
        <span class="meta-item diary-id" :title="diary?.id">
          <span class="meta-icon">🆔</span>
          <span class="meta-text">{{ diary?.id.substring(0, 8) }}</span>
        </span>
        <span class="meta-item" v-if="getAttachmentInfo(diary?.attachments)">
          <span class="meta-icon">📦</span>
          <span class="meta-text">
            {{ getAttachmentInfo(diary?.attachments)!.count }} 个附件
            <span class="meta-detail">{{ formatBytes(getAttachmentInfo(diary?.attachments)?.totalSize) }}</span>
            <span class="meta-detail" v-if="getAttachmentInfo(diary?.attachments)!.imageCount > 0">
              ( {{ getAttachmentInfo(diary?.attachments)!.imageCount }} 张图片)
            </span>
          </span>
        </span>
      </div>

      <span class="open-indicator">
        <svg class="arrow-icon" viewBox="0 0 24 24" width="16" height="16">
          <path d="M8.59 16.59L13.17 12 8.59 7.41 10 6l6 6-6 6-1.41-1.41z"/>
        </svg>
      </span>
    </div>
  </div>
</template>

<style scoped lang="scss">
.diary-card {
  background-color: var(--pad-bg-color-200);
  border: 1px solid var(--pad-border-color-100);
  border-radius: var(--pad-radius-lg);
  margin-bottom: 16px;
  padding: 20px;
  margin-top: 2px;
  cursor: pointer;
  transition: all var(--pad-transition-base);
  box-shadow: var(--pad-shadow-md);

  &:hover {
    transform: translateY(-2px);
    box-shadow: var(--pad-shadow-md);
    border-color: var(--pad-border-color-300);
    background-color: var(--pad-bg-color-100);
  }

  &:active {
    transform: translateY(0);
  }

  .card-header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    margin-bottom: 16px;

    .date-group {
      .date-primary {
        display: block;
        font-size: 16px;
        font-weight: 600;
        color: var(--pad-text-color-100);
        margin-bottom: 4px;
      }

      .date-updated {
        display: flex;
        align-items: center;
        gap: 4px;
        font-size: 12px;
        color: var(--pad-text-color-400);

        .update-icon {
          font-size: 10px;
        }
      }
    }

    .card-actions {
      .attachment-badge {
        display: flex;
        align-items: center;
        gap: 4px;
        padding: 4px 8px;
        background-color: var(--pad-bg-color-300);
        border-radius: var(--pad-radius-full);
        font-size: 12px;
        color: var(--pad-text-color-300);
        transition: all var(--pad-transition-fast);

        &:hover {
          background-color: var(--pad-primary-light);
          color: var(--pad-text-color-light);
        }

        .badge-icon {
          font-size: 12px;
        }

        .badge-count {
          font-weight: 600;
        }
      }
    }
  }

  .card-content {
    margin-bottom: 16px;

    .preview-content {
      font-size: 15px;
      line-height: 1.6;
      color: var(--pad-text-color-200);
      margin: 0;
      display: -webkit-box;
      -webkit-box-orient: vertical;
      overflow: hidden;
      text-overflow: ellipsis;
      max-height: 1.6rem;
      white-space: pre-wrap;
    }
  }

  .card-footer {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding-top: 12px;
    border-top: 1px solid var(--pad-border-color-100);

    .meta-info {
      display: flex;
      flex-wrap: wrap;
      gap: 12px;

      .meta-item {
        display: flex;
        align-items: center;
        gap: 4px;
        font-size: 12px;
        color: var(--pad-text-color-400);

        &.diary-id {
          cursor: help;
          overflow: hidden;
          text-overflow: ellipsis;
          white-space: nowrap;
        }

        .meta-icon {
          font-size: 12px;
          opacity: 0.7;
        }

        .meta-text {
          line-height: 1.3;
        }

        .meta-detail {
          font-size: 11px;
          opacity: 0.8;
        }
      }
    }

    .open-indicator {
      .arrow-icon {
        fill: var(--pad-text-color-400);
        transition: transform var(--pad-transition-fast);
      }
    }
  }

  &:hover .arrow-icon {
    transform: translateX(2px);
  }
}

@media (max-width: 512px) {
  .diary-card {
    padding: 16px;

    .card-footer {
      .meta-info {
        gap: 8px;

        .meta-item {
          .meta-text {
            overflow: hidden;
            text-overflow: ellipsis;
            white-space: nowrap;
          }
        }
      }
    }
  }
}
</style>
