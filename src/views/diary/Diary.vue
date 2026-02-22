<script setup lang="ts">
import {computed, onMounted, ref} from "vue";
import {commands, DiarySummary} from "../../bindings.ts";
import {useRoute} from "vue-router";
import DiaryHeader from "./DiaryHeader.vue";
import LiveRichEditor from "../../components/LiveRichEditor.vue";
import EditToolbar from "../../components/EditToolbar.vue";
import {useQuasar} from "quasar";
import {formatTimestamp} from "../../utils";

const $q = useQuasar();
const route = useRoute();

const diaryId = ref<string>("");
const diary = ref<DiarySummary>();
const diaryContent = ref<string>("");

const showMenu = ref(false);

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
  showMenu.value = true;
}

function showDiaryDetail() {
  if (!diary.value) {
    $q.notify({
      message: '日记信息未加载'
    });
    return;
  }
  const {title, created, updated, attachments} = diary.value;
  $q.dialog({
    title,
    message: `创建时间：${formatTimestamp(created)}<br>更新时间：${formatTimestamp(updated)}<br>附件数量：${attachments.length}`,
    html: true,
    ok: {
      label: '关闭',
      color: 'primary',
      flat: true
    },
  });
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
        @info="showDiaryDetail"
        @operate="operate"
        style="width: 100%; flex-shrink: 0"
    />
    <LiveRichEditor v-model="diaryContent" style="width: 100%; flex: 1"/>
    <EditToolbar style="width: 100%; flex-shrink: 0"/>

    <q-dialog v-model="showMenu" position="bottom">
      <q-card class="action-sheet-card">
        <q-list padding class="text-center">
          <q-item clickable v-ripple @click="">
            <q-item-section>操作1</q-item-section>
          </q-item>
          <q-item clickable v-ripple @click="">
            <q-item-section>操作2</q-item-section>
          </q-item>
          <q-item clickable v-ripple @click="showMenu = false">
            <q-item-section>取消</q-item-section>
          </q-item>
        </q-list>
      </q-card>
    </q-dialog>
  </main>
</template>

<style scoped lang="scss">
main {
  display: flex;
  flex-direction: column;
  width: 100%;
  height: 100%;

  .action-sheet-card {
    width: 100%;
    overflow: hidden;
  }
}
</style>
