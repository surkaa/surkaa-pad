<script setup lang="ts">
import {onActivated, onDeactivated, onUnmounted, ref} from "vue";
import DiarySummaryCard from "../../components/DiarySummaryCard.vue";
import {DiarySummary, SearchDiariesEvent} from "../../bindings.ts";
import {Channel} from "@tauri-apps/api/core";
import {useQuasar} from "quasar";
import {useOpenDiaryDetail} from "../../composables/useOpenDiaryDetail.ts";
import api from "../../utils/api.ts";
import {formatError} from "../../utils/formatError.ts";

const $q = useQuasar();
const {openDiary} = useOpenDiaryDetail();
const keyword = ref('');

const diarySummaries = ref<DiarySummary[]>([]);
const or = ref(false);

// 用于记录滚动位置，保持在列表页和详情页切换时的滚动状态
const savedScrollTop = ref(0);
const scrollContainer = ref<HTMLElement | null>(null);
const cancelToken = ref<string>();
const searchTotal = ref(0);

// 激活状态
const isActivating = ref(true);

async function searchHandle() {
  if (cancelToken.value) {
    await api.cmdCancelTask(cancelToken.value);
    return;
  }
  // 清空
  diarySummaries.value = [];
  if (!keyword.value || !keyword.value.trim()) {
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
    cancelToken.value = await api.cmdSearchDiaries(event, keyword.value, or.value);
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
          <q-btn dense round flat icon="search" size="sm" @click="searchHandle"/>
          <q-toggle dense v-model="or" size="sm" class="q-ml-xs" :icon="or ? 'alt_route' : 'reorder'"/>
        </template>
      </q-input>
    </Teleport>

    <section id="list" class="scroll-container" ref="scrollContainer" @scroll="handleScroll">
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
  }
}
</style>