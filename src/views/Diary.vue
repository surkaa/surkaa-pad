<script setup lang="ts">
import {onMounted, ref} from "vue";
import {DiaryManifest} from "../types";


const diary = ref<DiaryManifest>({
  id: "",
  content: "",
  created: 0,
  updated: 0,
  algorithm: "",
  attachments: []
});

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