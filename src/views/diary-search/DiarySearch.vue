<script setup lang="ts">
import {computed, nextTick, onActivated, onDeactivated, onUnmounted, ref} from "vue";
import {useScroll} from "@vueuse/core";
import DiarySummaryCard from "../../components/DiarySummaryCard.vue";
import {AttachmentTypeFilter, DiarySummary, SearchDiariesEvent} from "../../bindings.ts";
import {Channel} from "@tauri-apps/api/core";
import {useQuasar} from "quasar";
import {useOpenDiaryDetail} from "../../composables/useOpenDiaryDetail.ts";
import api from "../../utils/api.ts";
import {formatError} from "../../utils/formatError.ts";
import {
  attachmentTypeOptions,
  type AttachmentFilterSelection,
  hasDiarySearchCriteria,
  NO_ATTACHMENT_FILTER,
  selectedAttachmentTypes,
  toggleAttachmentFilterSelection,
} from "../../utils/diarySearchFilters.ts";

const $q = useQuasar();
const {openDiary} = useOpenDiaryDetail();
const keyword = ref('');
const activeKeyword = ref('');
const shouldAutoFocus = ref(true);

const diarySummaries = ref<DiarySummary[]>([]);
const or = ref(false);
const attachmentOr = ref(true);
const attachmentFilterSelection = ref<AttachmentFilterSelection[]>([NO_ATTACHMENT_FILTER]);
const attachmentTypes = computed<AttachmentTypeFilter[]>(() =>
    selectedAttachmentTypes(attachmentFilterSelection.value)
);
const canSearch = computed(() => hasDiarySearchCriteria(keyword.value, attachmentTypes.value));

const scrollContainer = ref<HTMLElement | null>(null);
const {y} = useScroll(scrollContainer, {behavior: 'smooth'});
const cancelToken = ref<string>();
const searchTotal = ref(0);
let searchSequence = 0;
let filterRevision = 0;

// 激活状态
const isActivating = ref(true);

async function toggleAttachmentFilter(value: AttachmentFilterSelection) {
  attachmentFilterSelection.value = toggleAttachmentFilterSelection(
      attachmentFilterSelection.value,
      value,
  );
  const revision = ++filterRevision;
  await cancelCurrentSearch();
  if (revision === filterRevision) {
    await startSearch();
  }
}

async function setKeywordMatchMode(matchAny: boolean) {
  if (or.value === matchAny) return;
  or.value = matchAny;
  const revision = ++filterRevision;
  await cancelCurrentSearch();
  if (revision === filterRevision) {
    await startSearch();
  }
}

async function setAttachmentMatchMode(matchAny: boolean) {
  if (attachmentOr.value === matchAny) return;
  attachmentOr.value = matchAny;
  const revision = ++filterRevision;
  await cancelCurrentSearch();
  if (revision === filterRevision) {
    await startSearch();
  }
}

async function cancelCurrentSearch() {
  searchSequence += 1;
  const token = cancelToken.value;
  cancelToken.value = undefined;
  if (token) {
    await api.cmdCancelTask(token);
  }
}

async function startSearch() {
  const currentSearch = ++searchSequence;
  const searchKeyword = keyword.value;
  activeKeyword.value = searchKeyword;
  // 清空
  diarySummaries.value = [];
  searchTotal.value = 0;
  if (!canSearch.value) {
    return;
  }

  let finished = false;
  const event = new Channel<SearchDiariesEvent>();
  event.onmessage = msg => {
    if (currentSearch !== searchSequence) return;
    switch (msg.event) {
      case "match":
        searchTotal.value = searchTotal.value + 1;
        diarySummaries.value.push(msg.data);
        break;
      case "unmatch":
        searchTotal.value = searchTotal.value + 1;
        console.log('收到unmatch事件，id：', msg.data);
        break;
      case "finished":
        finished = true;
        $q.notify({type: 'positive', message: '搜索完成'});
        cancelToken.value = undefined;
        break;
      case "error":
        finished = true;
        $q.notify({type: 'negative', message: msg.data || '搜索失败'});
        cancelToken.value = undefined;
        break;
    }
  };
  try {
    const token = await api.cmdSearchDiaries(
        event,
        searchKeyword,
        or.value,
        attachmentTypes.value,
        attachmentOr.value,
    );
    if (currentSearch !== searchSequence) {
      await api.cmdCancelTask(token);
    } else if (!finished) {
      cancelToken.value = token;
      console.log('搜索中，取消令牌：', token);
    }
  } catch (e) {
    if (currentSearch === searchSequence) {
      $q.notify({type: 'negative', message: formatError(e)});
    }
  }
}

async function searchHandle() {
  if (cancelToken.value) {
    await cancelCurrentSearch();
    return;
  }
  await startSearch();
}

onActivated(async () => {
  isActivating.value = true;
  // 等待 KeepAlive 中的页面重新显示并完成布局后再恢复滚动位置。
  await nextTick();
  if (scrollContainer.value) {
    scrollContainer.value.scrollTop = y.value;
  }
});

onDeactivated(() => {
  isActivating.value = false;
  shouldAutoFocus.value = false;
});

onUnmounted(() => {
  // 组件销毁时取消搜索任务
  void cancelCurrentSearch();
  keyword.value = '';
})
</script>

<template>
  <div id="diary-search">
    <Teleport v-if="isActivating" defer to="#header-actions">
      <q-input dense :autofocus="shouldAutoFocus" v-model="keyword" placeholder="输入关键词搜索" @keyup.enter="searchHandle" class="full-width">
        <template #prepend v-if="keyword.indexOf(' ') != -1">
          <q-badge transparent :label="or ? 'OR' : 'AND'"/>
        </template>

        <template #append>
          <q-btn
              dense
              round
              flat
              icon="search"
              size="sm"
              :disable="!canSearch && !cancelToken"
              @click="searchHandle"
          />
        </template>
      </q-input>
    </Teleport>

    <section id="list" class="scroll-container" ref="scrollContainer">
      <div class="attachment-filter">
        <div class="attachment-filter-heading">
          <div class="attachment-filter-label">附件类型</div>
          <div class="attachment-filter-hint">可多选</div>
        </div>
        <div class="attachment-filter-options" role="group" aria-label="按附件类型筛选">
          <q-btn
            v-for="option in attachmentTypeOptions"
            :key="option.value"
            :label="option.label"
            :aria-pressed="attachmentFilterSelection.includes(option.value)"
            :class="{
              'attachment-filter-option': true,
              'is-selected': attachmentFilterSelection.includes(option.value),
            }"
            no-caps
            unelevated
            dense
            @click="toggleAttachmentFilter(option.value)"
          />
        </div>

        <div class="relation-filters">
          <div class="relation-filter">
            <div class="keyword-filter-heading">
              <div class="attachment-filter-label">附件关系</div>
              <div class="attachment-filter-hint">选择多种类型时生效</div>
            </div>
            <div class="keyword-filter-row">
              <div class="keyword-filter-options" role="group" aria-label="多个附件类型的匹配关系">
                <q-btn
                    label="全部"
                    :aria-pressed="!attachmentOr"
                    :class="{'keyword-filter-option': true, 'is-selected': !attachmentOr}"
                    no-caps
                    unelevated
                    dense
                    @click="setAttachmentMatchMode(false)"
                />
                <q-btn
                    label="任一"
                    :aria-pressed="attachmentOr"
                    :class="{'keyword-filter-option': true, 'is-selected': attachmentOr}"
                    no-caps
                    unelevated
                    dense
                    @click="setAttachmentMatchMode(true)"
                />
              </div>
              <div class="keyword-mode-hint">
                {{ attachmentOr ? '包含任一所选类型即可' : '需同时包含所有所选类型' }}
              </div>
            </div>
          </div>

          <div class="relation-filter">
            <div class="keyword-filter-heading">
              <div class="attachment-filter-label">关键词关系</div>
              <div class="attachment-filter-hint">多个关键词以空格分隔</div>
            </div>
            <div class="keyword-filter-row">
              <div class="keyword-filter-options" role="group" aria-label="多个关键词的匹配关系">
                <q-btn
                    label="全部"
                    :aria-pressed="!or"
                    :class="{'keyword-filter-option': true, 'is-selected': !or}"
                    no-caps
                    unelevated
                    dense
                    @click="setKeywordMatchMode(false)"
                />
                <q-btn
                    label="任一"
                    :aria-pressed="or"
                    :class="{'keyword-filter-option': true, 'is-selected': or}"
                    no-caps
                    unelevated
                    dense
                    @click="setKeywordMatchMode(true)"
                />
              </div>
              <div class="keyword-mode-hint">
                {{ or ? '包含任一关键词即可' : '需同时包含所有关键词' }}
              </div>
            </div>
          </div>
        </div>
      </div>

      <DiarySummaryCard
          v-for="d in diarySummaries"
          :key="d.id"
          :diary="d"
          @click="openDiary(d.id, activeKeyword)"
      />
      <div v-if="!diarySummaries.length">
        <p class="text-center text-gray-500">无日记</p>
      </div>
    </section>

    <Teleport v-if="isActivating && searchTotal" defer to="#footer-content">
      <span>匹配 {{ diarySummaries.length }} · 已检索 {{ searchTotal }}</span>
    </Teleport>
  </div>
</template>

<style scoped lang="scss">
#diary-search {
  width: 100%;
  height: 100%;
  display: flex;
  flex-direction: column;

  #list {
    flex: 1;
    overflow-y: auto;
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 12px;

    .attachment-filter {
      flex: none;
      padding: 10px;
      border: 1px solid var(--pad-border-color-100);
      border-radius: 8px;
      background-color: var(--pad-bg-color-200);

      .attachment-filter-heading {
        display: flex;
        align-items: baseline;
        flex-wrap: wrap;
        gap: 4px 10px;
        margin-bottom: 8px;
      }

      .attachment-filter-label {
        color: var(--pad-text-color);
        font-size: 13px;
        font-weight: 500;
      }

      .attachment-filter-hint {
        color: var(--pad-text-color-200);
        font-size: 12px;
      }

      .attachment-filter-options {
        display: grid;
        grid-template-columns: repeat(5, minmax(0, 1fr));
        border: 1px solid var(--pad-border-color-100);
        border-radius: 6px;
        overflow: hidden;
        background-color: var(--pad-bg-color-300);

        .attachment-filter-option {
          min-height: 34px;
          padding: 0 6px;
          background-color: transparent !important;
          color: var(--pad-text-color-200) !important;

          & + .attachment-filter-option {
            border-left: 1px solid var(--pad-border-color-100);
          }

          &.is-selected {
            background-color: var(--q-primary) !important;
            color: white !important;
          }
        }
      }

      .keyword-filter-heading {
        display: flex;
        align-items: baseline;
        flex-wrap: wrap;
        gap: 4px 10px;
        margin: 12px 0 8px;
      }

      .relation-filters {
        display: grid;
        grid-template-columns: minmax(0, 1fr);
        gap: 0 24px;
      }

      .keyword-filter-row {
        display: flex;
        align-items: center;
        flex-wrap: wrap;
        gap: 8px 12px;

        .keyword-filter-options {
          display: flex;
          overflow: hidden;
          border: 1px solid var(--pad-border-color-100);
          border-radius: 6px;
          background-color: var(--pad-bg-color-300);

          .keyword-filter-option {
            min-height: 32px;
            padding: 0 10px;
            background-color: transparent !important;
            color: var(--pad-text-color-200) !important;

            & + .keyword-filter-option {
              border-left: 1px solid var(--pad-border-color-100);
            }

            &.is-selected {
              background-color: var(--q-primary) !important;
              color: white !important;
            }
          }
        }

        .keyword-mode-hint {
          color: var(--pad-text-color-200);
          font-size: 12px;
        }
      }

      @media (min-width: 720px) {
        .relation-filters {
          grid-template-columns: repeat(2, minmax(0, 1fr));
        }
      }

    }
  }
}
</style>
