<script setup lang="ts">
import {computed, onMounted, ref} from "vue";
import {commands, DiarySummary} from "../../bindings.ts";
import {useRoute} from "vue-router";
import DiaryHeader from "./DiaryHeader.vue";
import LiveRichEditor from "../../components/LiveRichEditor.vue";
import EditToolbar from "../../components/EditToolbar.vue";

const route = useRoute();

const diaryId = ref<string>("");
const diary = ref<DiarySummary>();
const diaryContent = ref<string>("");

const isNew = computed(() => diaryId.value.trim() === "");

async function loadDiaryInfo(id: string) {
  // 获取日记摘要
  const res = await commands.cmdGetDiarySummary(id);
  if (res.status === 'error') {
    console.error(`加载日记 ${id} 摘要失败:`, res.error);
    return;
  }
  diary.value = res.data;

  // 获取日记内容
  const contentRes = await commands.cmdGetDiaryContent(id);
  if (contentRes.status === 'error') {
    console.error(`加载日记 ${id} 内容失败:`, contentRes.error);
    return;
  }
  diaryContent.value = contentRes.data;
}

function operate() {
  console.log('操作按钮被点击');
}

onMounted(async () => {
  diaryId.value = route.params.id as string || "";
  if (!isNew.value) {
    await loadDiaryInfo(diaryId.value);
  }
});
</script>

<template>
  <main>
    <DiaryHeader
        class="header"
        :title="diary?.title"
        @back="$router.back()"
        @operate="operate"
    />
    <LiveRichEditor class="editor"/>
    <EditToolbar class="toolbar"/>
  </main>
</template>

<style scoped lang="scss">
main {
  display: flex;
  flex-direction: column;
  width: 100%;
  height: 100%;

  .header {
    width: 100%;
    flex-shrink: 0;
  }

  .editor {
    width: 100%;
    flex: 1;
    overflow: auto;
  }

  .toolbar {
    width: 100%;
    flex-shrink: 0;
  }
}
</style>
