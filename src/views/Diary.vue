<script setup lang="ts">
import {computed, onMounted, ref} from "vue";
import {DiaryManifest} from "../types";
import {invoke} from "@tauri-apps/api/core";
import {useRouter} from "vue-router";

const router = useRouter();
const diary = ref<DiaryManifest>({
  id: "",         // 不可修改
  content: "",    // 可修改
  created: 0,     // 不可修改
  updated: 0,     // 不用动,后端会自动更新
  algorithm: "",  // 不可修改不可选择,只用于显示
  attachments: [] // 调用其他方法修改
});
const saveLoading = ref(false);
const delLoading = ref(false);

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
    return;
  }
  try {
    if (diary.value.id && diary.value.id.length > 0) {
      // 更新日记
      console.log("更新日记", diary.value);
      await invoke("update_diary_content_only", {
        uuid: diary.value.id,
        new_content: diary.value.content
      });
      console.log("日记更新成功");
    } else {
      // 新建日记
      console.log("新建日记", diary.value);
      const id = await invoke<string>("save_diary", {
        content: diary.value.content
      });
      diary.value.id = id;
      console.log("日记保存成功, ID:", id);
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
  delLoading.value = true;
  if (!diary.value.id || diary.value.id.length === 0) {
    // 未保存的
    if (!diary.value.content || diary.value.content.length === 0) {
      // 内容也为空, 直接返回
      back();
    } else {
      // 内容不为空, 确认是否放弃
      const confirmDelete = confirm("当前日记未保存, 确认放弃并删除吗?");
      if (confirmDelete) {
        back();
      }
    }
    return;
  }
  try {
    await invoke("delete_diary", {uuid: diary.value.id});
    console.log("日记删除成功");
    // 返回上一级页面
    back();
  } catch (e) {
    console.error("删除日记失败", e);
    alert("删除日记失败: " + e);
  } finally {
    delLoading.value = false;
  }
}

onMounted(() => {
  console.log('Diary OnMounted');
  if (history.state.diary) {
    diary.value = history.state.diary;
  }
});
</script>

<template>
  <main id="diary">
    diary: {{ diary }}
  </main>
</template>

<style scoped>

</style>