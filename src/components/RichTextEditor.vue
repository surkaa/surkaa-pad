<script setup lang="ts">
import {convertFilename2URL} from "../utils/convertFilename2URL.ts";
import {DiaryManifest} from "../types";
const {diary} = defineProps<{ diary: DiaryManifest }>();
</script>

<template>
  <div class="rich-text-editor">
  </div>
</template>

<style scoped lang="scss">
.rich-text-editor {
  width: 100%;
  height: 100%;
}
</style>