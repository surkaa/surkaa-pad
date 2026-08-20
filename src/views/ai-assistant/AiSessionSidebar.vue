<script setup lang="ts">
import type {AiSessionMeta} from '../../bindings';
import {formatTimestamp} from '../../utils/format';

defineProps<{
  sessions: AiSessionMeta[];
  activeSessionId: string | null;
  loading: boolean;
  error: string | null;
  disabled: boolean;
  deletingSessionId: string | null;
}>();

const emit = defineEmits<{
  new: [];
  select: [sessionId: string];
  delete: [session: AiSessionMeta];
  retry: [];
}>();

function sessionTitle(session: AiSessionMeta): string {
  return session.aiTitle?.trim() || session.title.trim() || '新对话';
}

function sessionSummary(session: AiSessionMeta): string {
  const rounds = Math.floor(session.committedMessageCount / 2);
  const time = formatTimestamp(session.updatedAt);
  return rounds > 0 ? `${time} · ${rounds} 轮对话` : time;
}
</script>

<template>
  <aside class="session-sidebar" aria-label="AI 会话列表">
    <div class="sidebar-header">
      <div>
        <div class="sidebar-title">对话</div>
        <div class="sidebar-subtitle">已加密并跟随数据存储</div>
      </div>
      <q-btn
        round
        flat
        dense
        icon="add"
        :disable="disabled"
        aria-label="新建 AI 对话"
        @click="emit('new')"
      >
        <q-tooltip>新建对话</q-tooltip>
      </q-btn>
    </div>

    <div v-if="loading" class="sidebar-state">
      <q-spinner-dots color="primary" size="28px"/>
      <span>正在读取对话</span>
    </div>
    <div v-else-if="error" class="sidebar-state is-error">
      <q-icon name="error_outline" size="24px"/>
      <span>{{ error }}</span>
      <q-btn flat dense no-caps color="primary" label="重试" @click="emit('retry')"/>
    </div>
    <div v-else-if="sessions.length === 0" class="sidebar-state">
      <q-icon name="forum" size="28px"/>
      <span>还没有保存的对话</span>
    </div>
    <div v-else class="session-list">
      <div
        v-for="session in sessions"
        :key="session.id"
        class="session-item"
        :class="{
          'is-active': session.id === activeSessionId,
          'is-disabled': disabled,
        }"
        role="button"
        :tabindex="disabled ? -1 : 0"
        :aria-disabled="disabled"
        @click="!disabled && emit('select', session.id)"
        @keydown.enter.prevent="!disabled && emit('select', session.id)"
        @keydown.space.prevent="!disabled && emit('select', session.id)"
      >
        <q-icon name="chat_bubble_outline" class="session-icon"/>
        <span class="session-content">
          <span class="session-title">{{ sessionTitle(session) }}</span>
          <span class="session-summary">{{ sessionSummary(session) }}</span>
        </span>
        <q-btn
          round
          flat
          dense
          size="sm"
          icon="delete_outline"
          class="delete-session"
          :loading="deletingSessionId === session.id"
          :disable="disabled || deletingSessionId !== null"
          :aria-label="`删除对话：${sessionTitle(session)}`"
          @click.stop="emit('delete', session)"
        >
          <q-tooltip>删除对话</q-tooltip>
        </q-btn>
      </div>
    </div>
  </aside>
</template>

<style scoped lang="scss">
.session-sidebar {
  width: 280px;
  min-width: 280px;
  height: 100%;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  color: var(--pad-text-color-200);
  background: var(--pad-bg-color-200);
  border-right: 1px solid var(--pad-border-color-100);
}

.sidebar-header {
  min-height: 68px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 12px 14px 10px 18px;
  border-bottom: 1px solid var(--pad-border-color-100);
}

.sidebar-title {
  color: var(--pad-text-color-100);
  font-size: 1rem;
  font-weight: 600;
}

.sidebar-subtitle,
.session-summary {
  color: var(--pad-text-color-400);
  font-size: 0.7rem;
}

.sidebar-state {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 8px;
  padding: 24px;
  color: var(--pad-text-color-400);
  text-align: center;
  font-size: 0.8rem;

  &.is-error {
    color: var(--pad-danger-color);
  }
}

.session-list {
  flex: 1;
  overflow-y: auto;
  padding: 8px;
}

.session-item {
  width: 100%;
  min-height: 60px;
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 9px 7px 9px 10px;
  border: 0;
  border-radius: var(--pad-radius-md);
  color: inherit;
  background: transparent;
  font: inherit;
  text-align: left;
  cursor: pointer;

  &:hover:not(.is-disabled) {
    background: var(--pad-bg-color-300);
  }

  &.is-active {
    background: color-mix(in srgb, var(--pad-primary-color) 18%, var(--pad-bg-color-300));
  }

  &.is-disabled {
    cursor: default;
  }
}

.session-icon {
  flex: none;
  color: var(--pad-primary-dark);
  font-size: 19px;
}

.session-content {
  min-width: 0;
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 3px;
}

.session-title,
.session-summary {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.session-title {
  color: var(--pad-text-color-200);
  font-size: 0.82rem;
  font-weight: 500;
}

.delete-session {
  flex: none;
  color: var(--pad-text-color-400);
  opacity: 0;
}

.session-item:hover .delete-session,
.session-item:focus-within .delete-session,
.session-item.is-active .delete-session {
  opacity: 1;
}

@media (max-width: 800px) {
  .session-sidebar {
    position: absolute;
    z-index: 12;
    inset: 0 auto 0 0;
    width: min(86vw, 320px);
    min-width: 0;
    transform: translateX(-100%);
    box-shadow: 8px 0 24px rgb(0 0 0 / 22%);
    transition: transform 0.2s ease;

    &.is-open {
      transform: translateX(0);
    }
  }

  .delete-session {
    opacity: 1;
  }
}

@media (prefers-reduced-motion: reduce) {
  .session-sidebar {
    transition: none;
  }
}
</style>
