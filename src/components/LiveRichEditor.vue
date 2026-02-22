<script setup lang="ts">
import {ref} from "vue";
import {BaseExtension} from "./editor/baseExtension.ts";

const editor = ref<HTMLDivElement>();
const extensions = [
    BaseExtension
];
const styles = extensions.map(ext => ext.style || '').join("\n");

// 声明不自动继承属性
defineOptions({
  inheritAttrs: false
});
</script>

<template>
    <component is="style">{{ styles }}</component>
    <div
        v-bind="$attrs"
        class="live-rich-editor"
        ref="editor"
        contenteditable="true"
    ></div>
</template>

<style scoped lang="scss">
.live-rich-editor {
  width: 100%;
  height: 100%;
  padding: 16px;
  box-sizing: border-box;
  outline: none;
  text-align: left;
}
</style>
