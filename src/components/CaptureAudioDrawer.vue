<script setup lang="ts">
import {showToast} from "../utils/toast.ts";
import {onUnmounted, ref} from "vue";
import {saveAttachment} from "../utils/saveAttachment.ts";

let mediaRecorder: MediaRecorder | null = null;
let stream: MediaStream | null = null;
let flushInterval: number | null = null;
let audioChunks: Blob[] = [];
const PERIODIC_FLUSH_MS = 100;
const MINE_TYPE = 'audio/webm';
const recording = ref(false);

const props = defineProps<{
  uuid: string;
}>();

async function startRecording() {
  if (recording.value) {
    console.warn('录音已在进行中');
    return;
  }
  try {
    stream = await navigator.mediaDevices.getUserMedia({audio: true});
    mediaRecorder = new MediaRecorder(stream, {mimeType: MINE_TYPE});
    mediaRecorder.ondataavailable = (event: BlobEvent) => {
      if (event.data.size > 0) {
        audioChunks.push(event.data);
      }
    };
    mediaRecorder.onstop = () => {
      if (!stream) {
        console.error('MediaStream 丢失');
        return;
      }
      console.log('录音已停止，处理音频数据...');
      // 停止并释放麦克风轨道
      stream.getTracks().forEach(track => track.stop());
    };

    mediaRecorder.start();
    recording.value = true;
    flushInterval = setInterval(() => {
      if (mediaRecorder && mediaRecorder.state === 'recording') {
        mediaRecorder.requestData(); // 刷新Blob缓冲，触发dataavailable事件
      }
    }, PERIODIC_FLUSH_MS);
    console.log("录音开始...");
  } catch (err) {
    stopInterval();
    if (err instanceof DOMException) {
      if (err.name === 'NotFoundError') {
        showToast('未找到麦克风设备', 'error');
        return;
      }
    }
    showToast(`无法访问麦克风: ${err}`, 'error');
    console.error('获取麦克风失败: ', err);
  }
}

async function stopRecording() {
  recording.value = false;
  if (!mediaRecorder) {
    console.warn('没有正在进行的录音');
    return;
  }
  if (mediaRecorder.state === 'inactive') {
    console.warn('录音已经停止');
    return;
  }
  mediaRecorder.stop();
  // 整合音频数据
  const audioBlob = new Blob(audioChunks, {type: MINE_TYPE});
  await saveAttachment(props.uuid, MINE_TYPE, audioBlob.stream());
  console.log("录音停止...");
  stopInterval();
}

function stopInterval() {
  if (flushInterval) {
    clearInterval(flushInterval);
    flushInterval = null;
  }
}

onUnmounted(() => {
  stopInterval();
});
</script>

<template>
  <div class="capture-audio">
    <span>
      {{ recording ? '正在录音...' : '未录音' }}
    </span>
    <button @click="startRecording">开始录音</button>
    <button @click="stopRecording">停止录音</button>
  </div>
</template>

<style scoped lang="scss">
.capture-audio {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
</style>
