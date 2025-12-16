<script setup lang="ts">
import { formatTimestamp, getCurEmoji } from "../../utils";
import {DownloadAttachmentEvent} from "../../types";

defineProps<{
  contentLen: number;
  statusMsg: string;
  updated: number;
  created: number;
  downType: DownloadAttachmentEvent['event'] | null;
}>();
</script>

<template>
  <section id="diary-detail-footer">
    <section id="diary-detail-footer-left">
      <div class="footer-item" :title="`${contentLen} 字`">
        <svg viewBox="0 0 24 24" width="14" height="14">
          <path d="M14 2H6c-1.1 0-1.99.9-1.99 2L4 20c0 1.1.89 2 1.99 2H18c1.1 0 2-.9 2-2V8l-6-6zm2 16H8v-2h8v2zm0-4H8v-2h8v2zm-3-5V3.5L18.5 9H13z"/>
        </svg>
        <span class="footer-text">{{ contentLen }}字</span>
      </div>
      <div class="footer-item" v-if="statusMsg" :title="statusMsg">
        <svg viewBox="0 0 24 24" width="14" height="14" v-if="downType == null">
          <path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm-2 15l-5-5 1.41-1.41L10 14.17l7.59-7.59L19 8l-9 9z"/>
        </svg>
        <div class="rotating" v-if="downType == 'downloadProgress' || downType == 'decrypting'">
          ⏳
        </div>
        <span class="footer-text">{{ statusMsg }}</span>
      </div>
    </section>

    <section id="diary-detail-footer-right">
      <div class="footer-item" :title="formatTimestamp(updated)">
        <span class="footer-emoji">{{ getCurEmoji() }}</span>
        <span class="footer-text">{{ formatTimestamp(updated) }}</span>
      </div>
      <div class="footer-item" :title="formatTimestamp(created)">
        <svg viewBox="0 0 24 24" width="14" height="14">
          <path d="M20 3h-1V1h-2v2H7V1H5v2H4c-1.1 0-2 .9-2 2v16c0 1.1.9 2 2 2h16c1.1 0 2-.9 2-2V5c0-1.1-.9-2-2-2zm0 18H4V8h16v13z"/>
        </svg>
        <span class="footer-text">{{ formatTimestamp(created) }}</span>
      </div>
    </section>
  </section>
</template>

<style scoped lang="scss">
#diary-detail-footer {
  height: 48px;
  min-height: 48px;
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 0 8px;
  background-color: var(--pad-bg-color-200);
  border-top: 1px solid var(--pad-border-color-200);
  font-size: 13px;
  color: var(--pad-text-color-300);
  flex-shrink: 0;
  gap: 20px;

  #diary-detail-footer-left,
  #diary-detail-footer-right {
    display: flex;
    align-items: center;
    gap: 24px;
  }

  .footer-item {
    display: flex;
    align-items: center;
    gap: 8px;
    cursor: default;

    svg {
      fill: currentColor;
      opacity: 0.7;
    }

    .footer-emoji {
      font-size: 14px;
    }

    .footer-text {
      font-size: 12px;
      font-weight: 400;
      letter-spacing: 0.2px;
    }

    &:hover {
      color: var(--pad-text-color-200);
    }
  }

  // 状态信息样式
  #diary-detail-footer-left .footer-item:nth-child(2) {
    color: var(--pad-success-color);
    font-weight: 500;

    svg {
      fill: currentColor;
    }
  }
}

// 响应式设计
@media (max-width: 768px) {
  #diary-detail-footer {
    height: 44px;
    font-size: 12px;
    gap: 12px;

    #diary-detail-footer-left,
    #diary-detail-footer-right {
      gap: 16px;
    }

    .footer-item {
      gap: 3px;
    }
  }
}

@media (max-width: 480px) {
  #diary-detail-footer {
    #diary-detail-footer-left,
    #diary-detail-footer-right {
      gap: 12px;
    }
  }
}
</style>
