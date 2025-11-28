<script setup lang="ts">
import {computed, ref} from "vue";
import {DiaryEntry} from "../types";

const searchTerm = ref('');
const diaries = ref<DiaryEntry[]>([
  {id: 1696118400000, nonce: []}, // 2023-10-01
  {id: 1701388800000, nonce: []}, // 2023-12-01
  {id: 1696204800000, nonce: []}, // 2023-10-02
  {id: 1704067200000, nonce: []}, // 2024-01-01
  {id: 1696291200000, nonce: []}, // 2023-10-03
  {id: 1709280000000, nonce: []}, // 2024-03-01
  {id: 1696377600000, nonce: []}, // 2023-10-04
  {id: 1711958400000, nonce: []}, // 2024-04-01
]);

const filteredDiaries = computed(() => {
  // 随机选取部分日记条目以模拟筛选效果
  const termLength = searchTerm.value.length;
  let result = diaries.value.filter(
      () => Math.random() < (termLength * 0.1)
  );
  // 按时间倒序排列
  result.sort((a, b) => b.id - a.id);
  return result;
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