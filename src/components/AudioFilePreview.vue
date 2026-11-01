<template><div ref="host" class="attachment-audio-preview"/></template>
<script setup lang="ts">
import {onBeforeUnmount, onMounted, ref} from 'vue';
import {Schema} from '@tiptap/pm/model';
import {createAudioNodeView} from './editor/tiptap-extensions/AudioNode';
const props = defineProps<{url: string; size: number}>();
const host = ref<HTMLElement>();
// 复用日记的音频 NodeView，但不创建编辑器、不伪造附件元数据、不触发日记静默更新。
const schema = new Schema({nodes: {doc: {content: 'audio'}, text: {}, audio: {attrs: {id: {default: ''}, src: {default: ''}}}}});
let player: ReturnType<typeof createAudioNodeView> | undefined;
onMounted(() => {
  player = createAudioNodeView(schema.node('audio', {id: 'archive-preview', src: props.url}), {
    getAttachment: () => ({size: props.size, contentInfo: null}), onAudioInfoGenerated: () => undefined,
  });
  host.value?.append(player.dom);
});
onBeforeUnmount(() => player?.destroy?.());
</script>
<style scoped>
.attachment-audio-preview {display: flex; flex: 1; min-height: 0; align-items: center; justify-content: center; padding: 24px; overflow: auto; background: var(--pad-bg-color-100);}
</style>
