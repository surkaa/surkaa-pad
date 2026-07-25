<script setup lang="ts">
import {computed, onActivated, onDeactivated, onUnmounted, ref} from "vue";
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

const diarySummaries = ref<DiarySummary[]>([]);
const or = ref(false);
const attachmentFilterSelection = ref<AttachmentFilterSelection[]>([NO_ATTACHMENT_FILTER]);
const attachmentTypes = computed<AttachmentTypeFilter[]>(() =>
    selectedAttachmentTypes(attachmentFilterSelection.value)
);
const canSearch = computed(() => hasDiarySearchCriteria(keyword.value, attachmentTypes.value));

// 用于记录滚动位置，保持在列表页和详情页切换时的滚动状态
const savedScrollTop = ref(0);
const scrollContainer = ref<HTMLElement | null>(null);
const cancelToken = ref<string>();
const searchTotal = ref(0);

// 激活状态
const isActivating = ref(true);

function toggleAttachmentFilter(value: AttachmentFilterSelection) {
  attachmentFilterSelection.value = toggleAttachmentFilterSelection(
      attachmentFilterSelection.value,
      value,
  );
}

async function searchHandle() {
  if (cancelToken.value) {
    await api.cmdCancelTask(cancelToken.value);
    return;
  }
  // 清空
  diarySummaries.value = [];
  if (!canSearch.value) {
    return;
  }

  const event = new Channel<SearchDiariesEvent>();
  event.onmessage = msg => {
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
        $q.notify({type: 'positive', message: '搜索完成'});
        cancelToken.value = undefined;
        break;
      case "error":
        $q.notify({type: 'negative', message: msg.data || '搜索失败'});
        cancelToken.value = undefined;
        break;
    }
  };
  try {
    searchTotal.value = 0;
    cancelToken.value = await api.cmdSearchDiaries(
        event,
        keyword.value,
        or.value,
        attachmentTypes.value,
    );
    console.log('搜索中，取消令牌：', cancelToken.value);
  } catch (e) {
    $q.notify({type: 'negative', message: formatError(e)});
  }
}

function handleScroll(e: Event) {
  const target = e.target as HTMLElement;
  savedScrollTop.value = target.scrollTop;
}

onActivated(() => {
  isActivating.value = true;
  // 激活时恢复滚动位置
  if (scrollContainer.value) {
    scrollContainer.value.scrollTop = savedScrollTop.value;
  }
});

onDeactivated(() => {
  isActivating.value = false;
  // 离开时保存滚动位置
  if (scrollContainer.value) {
    savedScrollTop.value = scrollContainer.value.scrollTop;
  }
});

onUnmounted(() => {
  // 组件销毁时取消搜索任务
  if (cancelToken.value) {
    api.cmdCancelTask(cancelToken.value).then();
  }
  keyword.value = '';
})
</script>

<template>
  <div id="diary-search">
    <Teleport v-if="isActivating" defer to="#header-actions">
      <q-input dense autofocus v-model="keyword" placeholder="输入关键词搜索" @keyup.enter="searchHandle" class="full-width">
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
          <q-toggle dense v-model="or" size="sm" class="q-ml-xs" :icon="or ? 'alt_route' : 'reorder'"/>
        </template>
      </q-input>
    </Teleport>

    <section id="list" class="scroll-container" ref="scrollContainer" @scroll="handleScroll">
      <div class="attachment-filter">
        <div class="attachment-filter-label">附件类型</div>
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
        <div class="attachment-filter-hint">具体类型可多选，满足任一类型即可</div>
      </div>

      <DiarySummaryCard
          v-for="d in diarySummaries"
          :key="d.id"
          :diary="d"
          @click="openDiary(d.id)"
      />
      <div v-if="!diarySummaries.length">
        <p class="text-center text-gray-500">无日记</p>
      </div>
    </section>

    <Teleport v-if="isActivating && searchTotal" defer to="#footer-content">
      <span>共搜索到 {{ diarySummaries.length }} / {{ searchTotal }} 个日记</span>
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

      .attachment-filter-label {
        margin-bottom: 8px;
        color: var(--pad-text-color);
        font-size: 13px;
        font-weight: 500;
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

      .attachment-filter-hint {
        margin-top: 6px;
        color: var(--pad-text-color-200);
        font-size: 12px;
      }
    }
  }
}
</style>
