<script setup lang="ts">
import {setWebmDuration} from "../utils";
import {computed, onUnmounted, ref} from "vue";
import {useQuasar} from "quasar";

let mediaRecorder: MediaRecorder | null = null;
let stream: MediaStream | null = null;
let flushInterval: number | null = null;
let audioChunks: Blob[] = [];
const PERIODIC_FLUSH_MS = 100;
const recording = ref(false);
const recordingDuration = ref(0); // 录音时长（秒）
let startTime: number = 0; // 录音开始时间戳

const $q = useQuasar();

const {
  visible
} = defineProps<{
  visible: boolean;
}>();

const emit = defineEmits<{
  (e: 'close'): void;
  (e: 'recorded', mimetype: string, stream: ReadableStream<Uint8Array>): void;
}>();

// 格式化时长显示（MM:SS）
const formattedDuration = computed(() => {
  const minutes = Math.floor(recordingDuration.value / 60);
  const seconds = Math.floor(recordingDuration.value % 60);
  return `${minutes.toString().padStart(2, '0')}:${seconds.toString().padStart(2, '0')}`;
});

const SUPPORTED_MIME_TYPES = [
  'audio/webm;codecs=opus',
  'audio/webm',
  'audio/ogg;codecs=opus',
  'audio/mp4',
];

function getSupportedMimeType() {
  for (const type of SUPPORTED_MIME_TYPES) {
    if (MediaRecorder.isTypeSupported(type)) {
      return type;
    }
  }
  return '';
}

async function startRecording() {
  if (recording.value) {
    console.warn('录音已在进行中');
    return;
  }
  try {
    stream = await navigator.mediaDevices.getUserMedia({ audio: true });
    const mimeType = getSupportedMimeType();
    mediaRecorder = new MediaRecorder(stream, { mimeType });
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
      stream.getTracks().forEach(track => track.stop());
    };

    // 重置时长并开始录音
    recordingDuration.value = 0;
    startTime = Date.now();
    mediaRecorder.start();
    recording.value = true;

    // 更新录音时长
    flushInterval = setInterval(() => {
      if (mediaRecorder && mediaRecorder.state === 'recording') {
        mediaRecorder.requestData();
        recordingDuration.value = Math.floor((Date.now() - startTime) / 1000);
      }
    }, PERIODIC_FLUSH_MS);

    console.log("录音开始...");
  } catch (err) {
    stopInterval();
    if (err instanceof DOMException) {
      if (err.name === 'NotFoundError') {
        $q.notify('未找到麦克风设备');
        return;
      } else if (err.name === 'NotAllowedError') {
        $q.notify(`请允许访问麦克风 ${err.message}`);
        return;
      } else {
        $q.notify(`无法访问麦克风 ${err.name}`);
        console.log('获取麦克风失败: ', err.name, err.stack);
        return;
      }
    }
    $q.notify(`无法访问麦克风: ${err}`);
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

  let audioBlob = new Blob(audioChunks, { type: mediaRecorder.mimeType });
  const newWebmBuffer = setWebmDuration(await audioBlob.arrayBuffer(), Date.now() - startTime);
  audioBlob = new Blob([newWebmBuffer], { type: mediaRecorder.mimeType });

  // 重置音频数据
  audioChunks = [];

  emit('close');
  emit('recorded', mediaRecorder.mimeType, audioBlob.stream());
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

function closeDrawer() {
  if (recording.value) {
    stopRecording();
  }
  emit('close');
}

onUnmounted(() => {
  stopInterval();
});
</script>

<template>
  <div class="capture-audio-drawer">
    <transition name="overlay">
      <div v-if="visible" class="overlay" @click="closeDrawer"></div>
    </transition>

    <transition name="drawer">
      <div v-if="visible" class="drawer">
        <div class="drawer-header">
          <button class="close-btn" @click="closeDrawer">
            <svg viewBox="0 0 24 24" width="20" height="20">
              <path d="M19 6.41L17.59 5 12 10.59 6.41 5 5 6.41 10.59 12 5 17.59 6.41 19 12 13.41 17.59 19 19 17.59 13.41 12z"/>
            </svg>
          </button>
          <h3 class="drawer-title">录音</h3>
        </div>

        <div class="recording-container">
          <div class="duration-display" :class="{ 'recording': recording }">
            <div class="duration-text">{{ recording ? formattedDuration : '00:00' }}</div>
            <div class="duration-label">{{ recording ? '录音中...' : '准备录音' }}</div>
          </div>

          <div class="button-container">
            <div
                class="record-btn"
                @click="clickBtn"
                :class="{ 'recording': recording }"
                aria-label="录音/停止录音"
            >
              <div class="btn-inner">
                <div class="btn-icon">
                  <svg v-if="!recording" class="mic-icon" viewBox="0 0 24 24">
                    <path d="M12 14c1.66 0 3-1.34 3-3V5c0-1.66-1.34-3-3-3S9 3.34 9 5v6c0 1.66 1.34 3 3 3z"/>
                    <path d="M17 11c0 2.76-2.24 5-5 5s-5-2.24-5-5H5c0 3.53 2.61 6.43 6 6.92V21h2v-3.08c3.39-.49 6-3.39 6-6.92h-2z"/>
                  </svg>
                  <div v-else class="stop-icon"></div>
                </div>
                <div class="btn-text">{{ recording ? '停止录音' : '开始录音' }}</div>
              </div>

              <!-- 录音时的脉动动画 -->
              <div v-if="recording" class="pulse-ring"></div>
              <div v-if="recording" class="pulse-ring delay-1"></div>
            </div>
          </div>

          <div class="hint-text">
            <p v-if="!recording">点击开始录音按钮开始录制音频</p>
            <p v-else>点击停止录音按钮或关闭窗口结束录音</p>
          </div>
        </div>
      </div>
    </transition>
  </div>
</template>

<style scoped lang="scss">
.capture-audio-drawer {
  font-family: var(--pad-font-family), serif;

  .overlay-enter-active, .overlay-leave-active {
    transition: opacity var(--pad-transition-base);
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
    background: var(--pad-shadow-color-400);
    backdrop-filter: blur(2px);
    z-index: 999;
  }

  .drawer-enter-active, .drawer-leave-active {
    transition: transform var(--pad-transition-base) cubic-bezier(0.25, 0.8, 0.25, 1);
  }

  .drawer-enter-from, .drawer-leave-to {
    transform: translateY(100%);
  }

  .drawer {
    position: fixed;
    bottom: 0;
    left: 0;
    width: 100%;
    height: auto;
    max-height: 80vh;
    background: var(--pad-bg-color-100);
    border-radius: var(--pad-radius-xl) var(--pad-radius-xl) 0 0;
    padding: 0;
    box-shadow: var(--pad-shadow-lg);
    z-index: 1000;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    border-top: 1px solid var(--pad-border-color-200);

    // 移动端适配
    @media (min-width: 768px) {
      left: 50%;
      bottom: 5%;
      width: min(90vw, 400px);
      transform: translateX(-50%);
      border-radius: var(--pad-radius-xl);
      max-height: min(80vh, 500px);
      border: 1px solid var(--pad-border-color-200);
    }

    .drawer-header {
      display: flex;
      align-items: center;
      padding: 16px 20px;
      border-bottom: 1px solid var(--pad-border-color-100);
      background: var(--pad-bg-color-200);

      .close-btn {
        background: transparent;
        border: none;
        padding: 8px;
        cursor: pointer;
        border-radius: var(--pad-radius-full);
        color: var(--pad-text-color-400);
        transition: all var(--pad-transition-fast);
        display: flex;
        align-items: center;
        justify-content: center;

        svg {
          fill: currentColor;
        }

        &:hover {
          background: var(--pad-bg-color-300);
          color: var(--pad-text-color-200);
        }

        &:active {
          transform: scale(0.95);
        }

        @media (max-width: 767px) {
          &:active {
            background: var(--pad-bg-color-300);
          }
        }
      }

      .drawer-title {
        margin: 0 auto;
        font-size: 16px;
        font-weight: 600;
        color: var(--pad-text-color-100);
        transform: translateX(-16px); // 平衡关闭按钮的位置
      }
    }

    .recording-container {
      padding: 32px 20px 40px;
      display: flex;
      flex-direction: column;
      align-items: center;
      flex: 1;
      background: var(--pad-bg-color-100);

      .duration-display {
        text-align: center;
        margin-bottom: 40px;
        transition: all var(--pad-transition-base);

        &.recording {
          .duration-text {
            color: var(--pad-record-primary);
            transform: scale(1.05);
          }
        }

        .duration-text {
          font-size: 48px;
          font-weight: 700;
          font-variant-numeric: tabular-nums;
          color: var(--pad-text-color-100);
          margin-bottom: 8px;
          transition: all var(--pad-transition-base);
          font-family: 'Segoe UI', system-ui, -apple-system, sans-serif;

          @media (min-width: 768px) {
            font-size: 56px;
          }
        }

        .duration-label {
          font-size: 14px;
          color: var(--pad-text-color-300);
          font-weight: 500;
          letter-spacing: 0.5px;
        }
      }

      .button-container {
        position: relative;
        margin-bottom: 32px;

        .record-btn {
          position: relative;
          width: 80px;
          height: 80px;
          border-radius: var(--pad-radius-full);
          border: none;
          background: var(--pad-primary-gradient);
          color: var(--pad-text-color-light);
          cursor: pointer;
          padding: 0;
          transition: all var(--pad-transition-base);
          box-shadow: var(--pad-shadow-md);

          // 移动端触摸优化
          @media (max-width: 767px) {
            &:active {
              transform: scale(0.95);
              box-shadow: var(--pad-shadow-sm);
            }
          }

          @media (min-width: 768px) {
            width: 100px;
            height: 100px;

            &:hover {
              transform: scale(1.05);
              box-shadow: var(--pad-shadow-lg);
            }
          }

          &.recording {
            background: var(--pad-record-gradient);
            box-shadow: 0 6px 20px var(--pad-shadow-color-300);

            @media (min-width: 768px) {
              &:hover {
                box-shadow: 0 8px 24px var(--pad-shadow-color-400);
              }
            }

            .btn-text {
              color: var(--pad-record-primary);
              font-weight: 600;
            }
          }

          .btn-inner {
            position: relative;
            z-index: 2;
            display: flex;
            flex-direction: column;
            align-items: center;
            justify-content: center;
            height: 100%;

            .btn-icon {
              display: flex;
              align-items: center;
              justify-content: center;
              margin-bottom: 6px;

              .mic-icon {
                width: 32px;
                height: 32px;
                fill: currentColor;
                filter: drop-shadow(0 1px 2px rgba(0, 0, 0, 0.1));

                @media (min-width: 768px) {
                  width: 36px;
                  height: 36px;
                }
              }

              .stop-icon {
                width: 24px;
                height: 24px;
                background: currentColor;
                border-radius: var(--pad-radius-sm);
                box-shadow: 0 1px 2px rgba(0, 0, 0, 0.1);

                @media (min-width: 768px) {
                  width: 28px;
                  height: 28px;
                }
              }
            }

            .btn-text {
              font-size: 12px;
              font-weight: 500;
              color: var(--pad-text-color-light);
              transition: color var(--pad-transition-fast);
              letter-spacing: 0.3px;

              @media (min-width: 768px) {
                font-size: 14px;
              }
            }
          }

          .pulse-ring {
            position: absolute;
            top: 0;
            left: 0;
            width: 100%;
            height: 100%;
            border-radius: var(--pad-radius-full);
            border: 2px solid var(--pad-record-pulse);
            animation: pulse 1.5s infinite;
            z-index: 1;

            &.delay-1 {
              animation-delay: 0.5s;
            }
          }
        }
      }

      .hint-text {
        text-align: center;
        font-size: 14px;
        color: var(--pad-text-color-400);
        line-height: 1.5;
        max-width: 280px;
        margin: 0 auto;
        padding: 12px 16px;
        background: var(--pad-bg-color-200);
        border-radius: var(--pad-radius-lg);
        border: 1px solid var(--pad-border-color-100);

        p {
          margin: 0;
        }
      }
    }
  }
}

@keyframes pulse {
  0% {
    transform: scale(1);
    opacity: 0.8;
  }
  70% {
    transform: scale(1.3);
    opacity: 0;
  }
  100% {
    transform: scale(1.3);
    opacity: 0;
  }
}

// 优化深色模式下的动画
@media (prefers-color-scheme: dark) {
  .capture-audio-drawer {
    .record-btn {
      &.recording {
        .pulse-ring {
          border-color: var(--pad-record-pulse);
        }
      }
    }
  }
}

// 打印样式优化
@media print {
  .capture-audio-drawer {
    display: none !important;
  }
}
</style>
