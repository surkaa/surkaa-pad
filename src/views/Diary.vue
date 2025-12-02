<script setup lang="ts">
import {computed, onMounted, ref} from "vue";
import {DiaryManifest} from "../types";
import {invoke} from "@tauri-apps/api/core";
import {useRouter} from "vue-router";
import {formatTimestamp} from "../utils/time.ts";

const router = useRouter();

// 默认值用于新建日记
const DEFAULT_DIARY: DiaryManifest = {
  id: "",
  content: "",
  created: Date.now(),
  updated: Date.now(),
  algorithm: "AES-256-GCM", // 默认加密算法
  attachments: []
} as const;

const diary = ref<DiaryManifest>(DEFAULT_DIARY);
const saveLoading = ref(false);
const delLoading = ref(false);
const isNew = computed(() => !diary.value.id); // 判断是否为新建日记

const contentLen = computed(() => {
  return diary.value.content ? diary.value.content.length : 0;
});

// 返回上一级页面
function back() {
  router.back();
}

// 保存或者更新日记
async function saveDiary() {
  saveLoading.value = true;
  if (!diary.value.content || diary.value.content.length === 0) {
    alert("日记内容不能为空");
    saveLoading.value = false;
    return;
  }
  try {
    if (isNew.value) {
      // 新建日记
      console.log("新建日记", diary.value);
      const d = await invoke<DiaryManifest>("save_diary", {
        content: diary.value.content
      });
      diary.value = d;
      console.log("日记保存成功, Diary: ", d);
      alert("日记保存成功");
    } else {
      // 更新日记
      console.log("更新日记, Old Diary: ", diary.value);
      const d = await invoke<DiaryManifest>("update_diary_content_only", {
        uuid: diary.value.id,
        newContent: diary.value.content
      });
      diary.value = d;
      console.log("日记更新成功, Diary: ", d);
      alert("日记更新成功");
    }
  } catch (e) {
    console.error("保存日记失败", e);
    alert("保存日记失败: " + e);
  } finally {
    saveLoading.value = false;
  }
}

// 删除当前日记
async function deleteDiary() {
  if (isNew.value) {
    const confirmAbandon = confirm("当前日记未保存, 确认放弃并返回吗?");
    if (confirmAbandon) back();
    return;
  }

  const confirmDelete = confirm("⚠️ 确认永久删除这篇日记吗?");
  if (!confirmDelete) return;

  delLoading.value = true;
  try {
    await invoke("delete_diary", {uuid: diary.value.id});
    console.log("日记删除成功");
    alert("日记删除成功");
    back();
  } catch (e) {
    console.error("删除日记失败", e);
    alert("删除日记失败: " + e);
  } finally {
    delLoading.value = false;
  }
}

onMounted(() => {
  if (history.state.diary) {
    diary.value = history.state.diary;
  }
});
</script>

<template>
  <main id="diary-detail">
    <section id="diary-detail-header">
      <button id="diary-detail-header-back-btn" @click="back">返回</button>
      <button id="diary-detail-header-save-btn" @click="saveDiary" :disabled="saveLoading">
        {{ saveLoading ? "保存中..." : (isNew ? "保存日记" : "更新日记") }}
      </button>
      <button id="diary-detail-header-delete-btn" @click="deleteDiary" :disabled="delLoading">
        {{ delLoading ? "删除中..." : "删除日记" }}
      </button>
    </section>
    <section id="diary-detail-main">
      <textarea id="diary-detail-content" v-model="diary.content"></textarea>
    </section>
    <section id="diary-detail-footer">
      <section id="diary-detail-footer-left">
        <span>字数: {{ contentLen }}</span>
      </section>
      <section id="diary-detail-footer-right">
        <span>最后更新: {{ formatTimestamp(diary.updated) }}</span>
        <span>创建时间: {{ formatTimestamp(diary.created) }}</span>
      </section>
    </section>
  </main>
</template>

<style scoped lang="scss">
</style>
