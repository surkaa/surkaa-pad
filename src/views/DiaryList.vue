<script setup lang="ts">
import {computed, onMounted, ref, watch} from "vue";
import {DiaryManifest} from "../types";
import {useAppStore} from "../stores/app.ts";

const appStore = useAppStore();
const searchTerm = ref('');
const diaries = ref<DiaryManifest[]>([]);
const matchIds = ref<Set<string>>(new Set());

const filteredDiaries = computed<DiaryManifest[]>(() => {
  if (matchIds.value.size == 0) return diaries.value;
  return diaries.value.filter(diary => matchIds.value.has(diary.id));
});

function loadLocalDiaries() {
  appStore.loadLocalDiaries().then((remoteDiaries) => {
    diaries.value = remoteDiaries;
  });
}

onMounted(() => {
  loadLocalDiaries();
  watch(searchTerm, async (term) => {
    console.log(`Searching for term: ${term}`);
    const matchIdArr = await appStore.searchWithKeyword(term);
    matchIds.value = new Set(matchIdArr);
  })
});
</script>

<template>
  <main id="diary-list">
    <section id="search">
      <input type="text" v-model="searchTerm" placeholder="关键词搜索">
    </section>
    <section id="list">
      <transition-group name="list" tag="ul">
        <li v-for="diary in filteredDiaries" :key="diary.id">
          <span>{{ new Date(diary.id).toLocaleString() }}</span>
        </li>
        <span v-if="filteredDiaries.length === 0" key="empty">
          无
        </span>
      </transition-group>
    </section>
    <!--悬浮的新增按钮-->
  </main>
</template>

<style scoped lang="scss">
#diary-list {
  position: relative;

  #search {
    margin: 20px;


    input {
      width: 100%;
      padding: 10px;
      box-sizing: border-box;
    }
  }

  #list {
    .list-enter-active {
      transition: all 0.5s ease;
    }

    .list-leave-active {
      transition: all 0.5s ease;
      position: absolute;
    }

    .list-enter-from,
    .list-leave-to {
      opacity: 0;
      transform: translateX(30px);
    }

    .list-move {
      transition: transform 0.5s ease;
    }

    .list-leave-active {
      position: absolute;
      width: 100%;
    }
  }
}
</style>