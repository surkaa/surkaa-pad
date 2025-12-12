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

function clickBtn() {
  if (recording.value) {
    stopRecording();
  } else {
    startRecording();
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
        <button class="btn" @click="clickBtn" :class="{'recording': recording}"/>
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
    backdrop-filter: blur(2px);
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
    background: linear-gradient(135deg, #f8f9fa 0%, #e9ecef 100%);
    padding: 40px 20px;
    box-shadow: 0 -10px 40px rgba(0, 0, 0, 0.15);
    border-radius: 24px 24px 0 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;

    .btn {
      width: 80px;
      height: 80px;
      border-radius: 50%;
      background: linear-gradient(135deg, #ff6b6b 0%, #ff4757 100%);
      border: none;
      cursor: pointer;
      position: relative;
      transition: all 0.3s cubic-bezier(0.4, 0, 0.2, 1);
      box-shadow:
          0 4px 20px rgba(255, 71, 87, 0.4),
          inset 0 2px 4px rgba(255, 255, 255, 0.3),
          inset 0 -2px 4px rgba(0, 0, 0, 0.1);

      &::before {
        content: '';
        position: absolute;
        top: 50%;
        left: 50%;
        transform: translate(-50%, -50%);
        width: 32px;
        height: 32px;
        background-color: white;
        border-radius: 8px;
        transition: all 0.3s ease;
      }

      &::after {
        content: '';
        position: absolute;
        top: 50%;
        left: 50%;
        transform: translate(-50%, -50%) scale(1);
        width: 40px;
        height: 40px;
        border-radius: 50%;
        background: rgba(255, 255, 255, 0.2);
        opacity: 0;
        transition: opacity 0.3s ease;
      }

      &:hover {
        transform: scale(1.05);
        box-shadow:
            0 6px 25px rgba(255, 71, 87, 0.5),
            inset 0 2px 4px rgba(255, 255, 255, 0.3),
            inset 0 -2px 4px rgba(0, 0, 0, 0.1);
      }

      &:active {
        transform: scale(0.98);
        box-shadow:
            0 2px 15px rgba(255, 71, 87, 0.3),
            inset 0 2px 4px rgba(255, 255, 255, 0.2),
            inset 0 -2px 4px rgba(0, 0, 0, 0.15);
      }

      &.recording {
        background: linear-gradient(135deg, #ff4757 0%, #ff3838 100%);
        animation: pulse 1.5s infinite;

        &::before {
          width: 20px;
          height: 20px;
          border-radius: 4px;
          background-color: white;
        }

        &::after {
          animation: ripple 1.5s infinite;
        }
      }
    }
  }

  @keyframes pulse {
    0%, 100% {
      box-shadow:
          0 4px 20px rgba(255, 71, 87, 0.4),
          inset 0 2px 4px rgba(255, 255, 255, 0.3),
          inset 0 -2px 4px rgba(0, 0, 0, 0.1);
    }
    50% {
      box-shadow:
          0 4px 30px rgba(255, 71, 87, 0.6),
          inset 0 2px 4px rgba(255, 255, 255, 0.3),
          inset 0 -2px 4px rgba(0, 0, 0, 0.1);
    }
  }

  @keyframes ripple {
    0% {
      transform: translate(-50%, -50%) scale(1);
      opacity: 0.8;
    }
    100% {
      transform: translate(-50%, -50%) scale(1.5);
      opacity: 0;
    }
  }

  // 添加说明文字
  .drawer::before {
    content: '点击开始录音，再次点击结束';
    position: absolute;
    top: 20px;
    color: #666;
    font-size: 14px;
    font-weight: 500;
    text-align: center;
    width: 100%;
    padding: 0 20px;
  }

  // 添加录音状态指示器
  .drawer::after {
    content: '';
    position: absolute;
    bottom: 120px;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: #ddd;
    transition: background-color 0.3s ease;
  }

  .btn.recording ~ .drawer::after {
    background: #ff4757;
    animation: blink 1s infinite;
  }

  @keyframes blink {
    0%, 100% {
      opacity: 1;
      transform: scale(1);
    }
    50% {
      opacity: 0.5;
      transform: scale(0.9);
    }
  }
}
</style>