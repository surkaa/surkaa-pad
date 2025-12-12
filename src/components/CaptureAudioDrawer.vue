<script setup lang="ts">
import {showToast} from "../utils/toast.ts";
import {onUnmounted, ref} from "vue";

let mediaRecorder: MediaRecorder | null = null;
let stream: MediaStream | null = null;
let flushInterval: number | null = null;
let audioChunks: Blob[] = [];
const PERIODIC_FLUSH_MS = 100;
const MINE_TYPE = 'audio/webm';
const recording = ref(false);

const {
  visible
} = defineProps<{
  visible: boolean;
}>();
const emit = defineEmits<{
  (e: 'close'): void;
  (e: 'recorded', minetype: string, stream: ReadableStream<Uint8Array>): void;
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
  // TODO 记录录音时长
  emit('close');
  emit('recorded', MINE_TYPE, audioBlob.stream());
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
  <div class="capture-audio-drawer">
    <transition name="overlay">
      <div v-if="visible" class="overlay" @click="emit('close')"></div>
    </transition>

    <transition name="drawer">
      <div v-if="visible" class="drawer">
        <span>
          {{ recording ? '正在录音...' : '未录音' }}
        </span>
        <button @click="startRecording">开始录音</button>
        <button @click="stopRecording">停止录音</button>
      </div>
    </transition>
  </div>
</template>

<style scoped lang="scss">
.capture-audio-drawer {
  .overlay-enter-active, .overlay-leave-active {
    transition: opacity 0.3s ease;
  }

  .overlay-enter-from, .overlay-leave-to {
    opacity: 0;
  }

  .overlay {
    position: fixed;
    top: 0;
    left: 0;
    width: 100%;
    height: 100%;
    background: rgba(0, 0, 0, 0.5);
  }

  .drawer-enter-active, .drawer-leave-active {
    transition: transform 0.3s ease;
  }

  .drawer-enter-from, .drawer-leave-to {
    transform: translateY(100%);
  }

  .drawer {
    position: fixed;
    bottom: 0;
    left: 0;
    width: 100%;
    height: 300px;
    background: white;
    padding: 20px;
    box-shadow: 0 -2px 10px rgba(0, 0, 0, 0.1);
  }
}
</style>
