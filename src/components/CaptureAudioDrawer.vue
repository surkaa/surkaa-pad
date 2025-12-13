<script setup lang="ts">
import { showToast } from "../utils/toast.ts";
import { onUnmounted, ref, computed } from "vue";

let mediaRecorder: MediaRecorder | null = null;
let stream: MediaStream | null = null;
let flushInterval: number | null = null;
let audioChunks: Blob[] = [];
const PERIODIC_FLUSH_MS = 100;
const MINE_TYPE = 'audio/webm';
const recording = ref(false);
const recordingDuration = ref(0); // 录音时长（秒）
let startTime: number = 0; // 录音开始时间戳

const {
  visible
} = defineProps<{
  visible: boolean;
}>();

const emit = defineEmits<{
  (e: 'close'): void;
  (e: 'recorded', minetype: string, stream: ReadableStream<Uint8Array>): void;
}>();

// 格式化时长显示（MM:SS）
const formattedDuration = computed(() => {
  const minutes = Math.floor(recordingDuration.value / 60);
  const seconds = Math.floor(recordingDuration.value % 60);
  return `${minutes.toString().padStart(2, '0')}:${seconds.toString().padStart(2, '0')}`;
});

async function startRecording() {
  if (recording.value) {
    console.warn('录音已在进行中');
    return;
  }
  try {
    stream = await navigator.mediaDevices.getUserMedia({ audio: true });
    mediaRecorder = new MediaRecorder(stream, { mimeType: MINE_TYPE });
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
        showToast('未找到麦克风设备', 'error');
        return;
      } else if (err.name === 'NotAllowedError') {
        showToast(`请允许访问麦克风 ${err.message}`, 'error');
        return;
      } else {
        showToast(`无法访问麦克风 ${err.name}`, 'error');
        console.log('获取麦克风失败: ', err.name, err.stack);
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
  const audioBlob = new Blob(audioChunks, { type: MINE_TYPE });

  // 重置音频数据
  audioChunks = [];

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
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, sans-serif;

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
    z-index: 999;
  }

  .drawer-enter-active, .drawer-leave-active {
    transition: transform 0.3s cubic-bezier(0.25, 0.8, 0.25, 1);
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
    background: #ffffff;
    border-radius: 24px 24px 0 0;
    padding: 0;
    box-shadow: 0 -4px 20px rgba(0, 0, 0, 0.15);
    z-index: 1000;
    display: flex;
    flex-direction: column;
    overflow: hidden;

    // 移动端适配
    @media (min-width: 768px) {
      left: 50%;
      bottom: 5%;
      width: 400px;
      transform: translateX(-50%);
      border-radius: 20px;
      max-height: 500px;
    }

    .drawer-header {
      display: flex;
      align-items: center;
      padding: 20px 24px 16px;
      border-bottom: 1px solid #f0f0f0;

      .close-btn {
        background: none;
        border: none;
        padding: 8px;
        cursor: pointer;
        border-radius: 50%;
        color: #666;
        transition: all 0.2s;

        &:hover, &:active {
          background: #f5f5f5;
          color: #333;
        }

        &:active {
          transform: scale(0.95);
        }
      }

      .drawer-title {
        margin: 0 auto;
        font-size: 18px;
        font-weight: 600;
        color: #333;
        transform: translateX(-16px); // 平衡关闭按钮的位置
      }
    }

    .recording-container {
      padding: 32px 24px 40px;
      display: flex;
      flex-direction: column;
      align-items: center;
      flex: 1;

      .duration-display {
        text-align: center;
        margin-bottom: 48px;
        transition: all 0.3s;

        &.recording {
          .duration-text {
            color: #ff4757;
            transform: scale(1.05);
          }
        }

        .duration-text {
          font-size: 48px;
          font-weight: 700;
          font-variant-numeric: tabular-nums;
          color: #333;
          margin-bottom: 8px;
          transition: all 0.3s;

          @media (min-width: 768px) {
            font-size: 56px;
          }
        }

        .duration-label {
          font-size: 14px;
          color: #666;
          font-weight: 500;
        }
      }

      .button-container {
        position: relative;
        margin-bottom: 32px;

        .record-btn {
          position: relative;
          width: 80px;
          height: 80px;
          border-radius: 50%;
          border: none;
          background: linear-gradient(135deg, #4a6cf7 0%, #3a56d4 100%);
          color: white;
          cursor: pointer;
          padding: 0;
          transition: all 0.3s cubic-bezier(0.175, 0.885, 0.32, 1.275);
          box-shadow: 0 6px 20px rgba(74, 108, 247, 0.3);

          // 移动端触摸优化
          @media (max-width: 767px) {
            &:active {
              transform: scale(0.95);
            }
          }

          @media (min-width: 768px) {
            width: 100px;
            height: 100px;

            &:hover {
              transform: scale(1.05);
              box-shadow: 0 10px 30px rgba(74, 108, 247, 0.4);
            }
          }

          &.recording {
            background: linear-gradient(135deg, #ff4757 0%, #ff3742 100%);
            box-shadow: 0 6px 20px rgba(255, 71, 87, 0.3);

            @media (min-width: 768px) {
              &:hover {
                box-shadow: 0 10px 30px rgba(255, 71, 87, 0.4);
              }
            }

            .btn-text {
              color: #ff4757;
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
                fill: white;

                @media (min-width: 768px) {
                  width: 36px;
                  height: 36px;
                }
              }

              .stop-icon {
                width: 24px;
                height: 24px;
                background: white;
                border-radius: 4px;

                @media (min-width: 768px) {
                  width: 28px;
                  height: 28px;
                }
              }
            }

            .btn-text {
              font-size: 12px;
              font-weight: 600;
              color: white;
              transition: color 0.3s;

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
            border-radius: 50%;
            border: 2px solid rgba(255, 71, 87, 0.6);
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
        color: #888;
        line-height: 1.5;
        max-width: 280px;
        margin: 0 auto;

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
    opacity: 1;
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

// 深色模式支持
@media (prefers-color-scheme: dark) {
  .capture-audio-drawer {
    .drawer {
      background: #1e1e1e;
      color: #fff;

      .drawer-header {
        border-bottom-color: #333;

        .drawer-title {
          color: #fff;
        }

        .close-btn {
          color: #aaa;

          &:hover, &:active {
            background: #333;
            color: #fff;
          }
        }
      }

      .recording-container {
        .duration-display {
          .duration-text {
            color: #fff;
          }

          &.recording {
            .duration-text {
              color: #ff6b81;
            }
          }

          .duration-label {
            color: #aaa;
          }
        }

        .hint-text {
          color: #aaa;
        }

        .button-container .record-btn.recording .btn-text {
          color: #ff6b81;
        }
      }
    }
  }
}
</style>