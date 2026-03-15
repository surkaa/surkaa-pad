<script setup lang="ts">
import {onActivated, onDeactivated, onUnmounted, ref} from "vue";
import DiarySummaryCard from "../../components/DiarySummaryCard.vue";
import {commands, DiarySummary, SearchDiariesEvent} from "../../bindings.ts";
import {Channel} from "@tauri-apps/api/core";
import {useQuasar} from "quasar";
import {useOpenDiaryDetail} from "../../composables/useOpenDiaryDetail.ts";

const $q = useQuasar();
const {openDiary} = useOpenDiaryDetail();
const keyword = ref('');

const diarySummaries = ref<DiarySummary[]>([]);
const or = ref(false);

// 用于记录滚动位置，保持在列表页和详情页切换时的滚动状态
const savedScrollTop = ref(0);
const scrollContainer = ref<HTMLElement | null>(null);
const cancelToken = ref<string>();

// 激活状态
const isActivating = ref(true);

async function searchHandle() {
  if (cancelToken.value) {
    await commands.cmdCancelTask(cancelToken.value);
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
        diarySummaries.value.push(msg.data);
        break;
      case "unmatch":
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
  const res = await commands.cmdSearchDiaries(event, keyword.value, or.value);
  if (res.status == 'error') {
    $q.notify({type: 'negative', message: res.error || '搜索失败'});
    return;
  }
  cancelToken.value = res.data;
  console.log('搜索中，取消令牌：', cancelToken.value);
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
    commands.cmdCancelTask(cancelToken.value);
  }
  keyword.value = '';
})
</script>

<template>
  <div id="diary-search">
    <Teleport v-if="isActivating" defer to="#header-actions">
      <q-input dense v-model="keyword" placeholder="输入关键词搜索" @keyup.enter="searchHandle">
        <template #append>
          <q-btn flat icon="search" @click="searchHandle"/>
          <q-toggle v-model="or"/>
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

    <Teleport v-if="isActivating" defer to="#footer-content">
      <span>共搜索到 {{diarySummaries.length}} 个日记</span>
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