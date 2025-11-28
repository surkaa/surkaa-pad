<script setup lang="ts">
import {ref} from "vue";
import {DiaryEntry} from "../types";

const diaries = ref<DiaryEntry[]>([{
  id: 1696118400000, nonce: []
}, {
  id: 1696204800000, nonce: []
}, {
  id: 1696291200000, nonce: []
}]);
</script>

<template>
  <main id="diary-list">
    <section id="search"></section>
    <section id="list">
      <transition-group name="list" tag="ul">
        <li v-for="diary in diaries" :key="diary.id">
          {{ new Date(diary.id).toLocaleString() }}
        </li>
      </transition-group>
    </section>
    <!--悬浮的新增按钮-->
  </main>
</template>

<style scoped>
#diary-list {
  /* 1. 列表进入 (插入) 时的状态 */

  .list-enter-active {
    /* 确保进入动画有持续时间 */
    transition: all 0.5s ease;
  }

  /* 2. 列表离开 (移除) 时的状态 */

  .list-leave-active {
    /* 确保离开动画有持续时间，且定位为 absolute 以便元素可以自由过渡和消失 */
    transition: all 0.5s ease;
    position: absolute; /* **重要：让移除的元素脱离文档流** */
  }

  /* 3. 元素进入前的初始状态 / 元素离开后的最终状态 */

  .list-enter-from,
  .list-leave-to {
    opacity: 0; /* 透明度为0 */
    transform: translateX(30px); /* 稍微从右侧移入 */
  }

  /* 4. 列表移动动画 (Move) */
  /* **关键：用于位置变化的动画** */

  .list-move {
    /* 当元素在列表中的位置发生变化时，应用平滑过渡 */
    transition: transform 0.5s ease;
  }

  /* 阻止列表项被移除时闪烁或占据空间 */

  .list-leave-active {
    /* 必须确保离开的元素是定位的 (如 absolute)，否则 Move 动画会出错 */
    position: absolute;
  }
}
</style>