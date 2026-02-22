<script setup lang="ts">
import {onMounted, ref} from "vue";
import {commands, DiarySummary} from "../../bindings.ts";

const diary = ref<DiarySummary>();
const diaryContent = ref<string>("");

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

onMounted(async () => {
  // 获取 route 参数
  const diaryId = history.state.diaryId;
  if (!diaryId) {
    console.error("未提供日记 ID");
    return;
  }
  console.log("Diary ID:", diaryId);
  await loadDiaryInfo(diaryId);
});
</script>

<template>
  <main>
    <h1>{{ diary?.title }}</h1>
    <span v-text="diaryContent"></span>
  </main>
</template>

<style scoped lang="scss">
</style>
