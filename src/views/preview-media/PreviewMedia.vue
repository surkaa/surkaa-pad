<script setup lang="ts">
import {onMounted, onUnmounted, ref, watch} from "vue";
import {useRoute, useRouter} from "vue-router";
import {useEventListener} from "../../utils/useEventListener.ts";
import {resolveMediaAttachmentUrl} from "../../utils/resolveMediaAttachmentUrl.ts";

const route = useRoute();
const router = useRouter();
const url = ref('');
const imageRef = ref<HTMLImageElement>();
const containerRef = ref<HTMLElement>();

// 缩放、旋转和拖拽状态
const scale = ref(1);
const rotation = ref(0); // 旋转角度，单位：度
const position = ref({x: 0, y: 0});
const isDragging = ref(false);
const lastPosition = ref({x: 0, y: 0});
const initialDistance = ref(0);
const isPinching = ref(false);
const maxScale = ref(10);
const minScale = ref(0.1);
let animationFrameId: number | null = null;

// 提示信息
const showTip = ref(false);
const tipMessage = ref('');
let tipTimer: number | null = null;

// 显示提示信息
function showTipMessage(message: string, duration: number = 2000) {
  tipMessage.value = message;
  showTip.value = true;

  if (tipTimer) {
    clearTimeout(tipTimer);
  }

  tipTimer = window.setTimeout(() => {
    showTip.value = false;
  }, duration);
}

// 旋转图片（顺时针90度）
function rotateImage() {
  rotation.value = rotation.value + 90;
  showTipMessage(`已旋转: ${rotation.value % 360}°`);
}

// 重置状态
function resetTransform() {
  scale.value = 1;
  rotation.value = 0;
  position.value = {x: 0, y: 0};
  showTipMessage('已重置');
}

// 检查是否在边界内
function clamp(value: number, min: number, max: number): number {
  return Math.min(Math.max(value, min), max);
}

// 桌面端鼠标事件
function handleWheel(event: WheelEvent) {
  event.preventDefault();

  const delta = event.deltaY > 0 ? -0.1 : 0.1;
  const newScale = clamp(scale.value + delta, minScale.value, maxScale.value);

  // 计算缩放中心点
  const rect = imageRef.value?.getBoundingClientRect();
  if (rect) {
    const mouseX = event.clientX;
    const mouseY = event.clientY;

    // 计算相对于图片中心的偏移
    const offsetX = (mouseX - rect.left - rect.width / 2) / scale.value;
    const offsetY = (mouseY - rect.top - rect.height / 2) / scale.value;

    // 调整位置以保持鼠标点不变
    position.value.x += offsetX * (scale.value - newScale);
    position.value.y += offsetY * (scale.value - newScale);
  }

  scale.value = newScale;
  showTipMessage(`${Math.round(scale.value * 100)}%`);
}

function handleMouseDown(event: MouseEvent) {
  if (event.button !== 0) return; // 只处理左键

  isDragging.value = true;
  lastPosition.value = {x: event.clientX, y: event.clientY};

  // 添加样式
  if (imageRef.value) {
    imageRef.value.style.cursor = 'grabbing';
  }

  event.preventDefault();
}

function handleMouseMove(event: MouseEvent) {
  if (!isDragging.value) return;

  // 取消之前的动画帧
  if (animationFrameId) {
    cancelAnimationFrame(animationFrameId);
  }

  // 使用 requestAnimationFrame 进行平滑更新
  animationFrameId = requestAnimationFrame(() => {
    const deltaX = event.clientX - lastPosition.value.x;
    const deltaY = event.clientY - lastPosition.value.y;

    position.value.x += deltaX;
    position.value.y += deltaY;

    lastPosition.value = {x: event.clientX, y: event.clientY};
    animationFrameId = null;
  });

  event.preventDefault();
}

function handleMouseUp() {
  isDragging.value = false;

  if (imageRef.value) {
    imageRef.value.style.cursor = scale.value > 1 ? 'grab' : 'default';
  }
}

// 移动端触摸事件
function handleTouchStart(event: TouchEvent) {
  if (event.touches.length === 1) {
    // 单指拖拽
    isDragging.value = true;
    lastPosition.value = {
      x: event.touches[0].clientX,
      y: event.touches[0].clientY
    };

    if (imageRef.value) {
      imageRef.value.style.cursor = 'grabbing';
    }
  } else if (event.touches.length === 2) {
    // 双指缩放
    isPinching.value = true;
    const touch1 = event.touches[0];
    const touch2 = event.touches[1];

    initialDistance.value = Math.sqrt(
        Math.pow(touch2.clientX - touch1.clientX, 2) +
        Math.pow(touch2.clientY - touch1.clientY, 2)
    );
  }

  // 如果不是button则阻止默认行为
  const target = event.target as HTMLElement;
  if (!target.closest('.control-btn')) {
    event.preventDefault();
  }
}

function handleTouchMove(event: TouchEvent) {
  if (isPinching.value && event.touches.length === 2) {
    // 双指缩放
    const touch1 = event.touches[0];
    const touch2 = event.touches[1];

    const currentDistance = Math.sqrt(
        Math.pow(touch2.clientX - touch1.clientX, 2) +
        Math.pow(touch2.clientY - touch1.clientY, 2)
    );

    if (initialDistance.value > 0) {
      const delta = (currentDistance - initialDistance.value) * 0.01;
      const newScale = clamp(scale.value + delta, minScale.value, maxScale.value);

      if (newScale !== scale.value) {
        scale.value = newScale;
        showTipMessage(`${Math.round(scale.value * 100)}%`);
      }

      // 更新初始距离用于下一次计算
      initialDistance.value = currentDistance;
    }
  } else if (isDragging.value && event.touches.length === 1) {
    // 单指拖拽
    const deltaX = event.touches[0].clientX - lastPosition.value.x;
    const deltaY = event.touches[0].clientY - lastPosition.value.y;

    position.value.x += deltaX;
    position.value.y += deltaY;

    lastPosition.value = {
      x: event.touches[0].clientX,
      y: event.touches[0].clientY
    };
  }

  event.preventDefault();
}

function handleTouchEnd(event: TouchEvent) {
  if (event.touches.length === 0) {
    isDragging.value = false;
    isPinching.value = false;

    if (imageRef.value) {
      imageRef.value.style.cursor = scale.value > 1 ? 'grab' : 'default';
    }
  }

  // 如果还剩一个手指，切换到拖拽模式
  if (event.touches.length === 1) {
    isPinching.value = false;
    isDragging.value = true;
    lastPosition.value = {
      x: event.touches[0].clientX,
      y: event.touches[0].clientY
    };
  }
}

// 添加事件监听
function addEventListeners() {
  if (!containerRef.value) {
    console.warn('Container ref is not defined.');
    return;
  }

  // 桌面端事件
  useEventListener(containerRef, 'wheel', handleWheel, {passive: false});
  useEventListener(containerRef, 'mousedown', handleMouseDown);
  useEventListener('mousemove', handleMouseMove);
  useEventListener('mouseup', handleMouseUp);

  // 移动端事件
  useEventListener(containerRef, 'touchstart', handleTouchStart, {passive: false});
  useEventListener(containerRef, 'touchmove', handleTouchMove, {passive: false});
  useEventListener(containerRef, 'touchend', handleTouchEnd);
  useEventListener(containerRef, 'touchcancel', handleTouchEnd);
}

// 监听缩放变化
watch(scale, (newScale) => {
  if (newScale !== 1) {
    showTipMessage(`${Math.round(newScale * 100)}%`);
  }
});

onMounted(() => {
  const {diaryId, filename} = route.params;
  if (Array.isArray(diaryId) || Array.isArray(filename)) {
    console.log('Invalid parameter:', diaryId, filename);
    return;
  }
  url.value = resolveMediaAttachmentUrl('image', diaryId, filename);
});

onUnmounted(() => {
  tipTimer && clearTimeout(tipTimer);
});
</script>

<template>
  <div
      ref="containerRef"
      class="preview-media"
      :style="{
        '--scale': scale,
        '--translate-x': `${position.x}px`,
        '--translate-y': `${position.y}px`,
        '--is-dragging': isDragging ? 1 : 0
      }"
  >
    <!-- 背景遮罩 -->
    <div class="preview-overlay" @click="router.back"></div>

    <!-- 控制栏 -->
    <div class="preview-controls">
      <button
          class="control-btn"
          @click="resetTransform"
          :disabled="scale === 1 && position.x === 0 && position.y === 0 && rotation === 0"
          aria-label="重置缩放和位置"
      >
        <svg class="control-icon" viewBox="0 0 24 24">
          <path
              d="M12 5V1L7 6l5 5V7c3.31 0 6 2.69 6 6s-2.69 6-6 6-6-2.69-6-6H4c0 4.42 3.58 8 8 8s8-3.58 8-8-3.58-8-8-8z"/>
        </svg>
        <span class="control-text">重置</span>
      </button>

      <button
          class="control-btn"
          @click="rotateImage"
          aria-label="旋转图片"
      >
        <svg class="control-icon" viewBox="0 0 24 24">
          <path
              d="M12 6v3l4-4-4-4v3c-4.42 0-8 3.58-8 8 0 1.57.46 3.03 1.24 4.26L6.7 14.8c-.45-.83-.7-1.79-.7-2.8 0-3.31 2.69-6 6-6zm6.76 1.74L17.3 9.2c.44.84.7 1.79.7 2.8 0 3.31-2.69 6-6 6v-3l-4 4 4 4v-3c4.42 0 8-3.58 8-8 0-1.57-.46-3.03-1.24-4.26z"/>
        </svg>
        <span class="control-text">旋转</span>
      </button>

      <button
          class="control-btn close-btn"
          @click="router.back"
          aria-label="关闭预览"
      >
        <svg class="control-icon" viewBox="0 0 24 24">
          <path
              d="M19 6.41L17.59 5 12 10.59 6.41 5 5 6.41 10.59 12 5 17.59 6.41 19 12 13.41 17.59 19 19 17.59 13.41 12z"/>
        </svg>
        <span class="control-text">关闭</span>
      </button>
    </div>

    <!-- 图片容器 -->
    <div class="image-container">
      <!-- 图片 -->
      <img
          ref="imageRef"
          alt="Preview"
          :src="url"
          class="preview-image"
          :class="{ 'draggable': scale > 1 }"
          :style="{
                    transform: `translate(${position.x}px, ${position.y}px) scale(${scale}) rotate(${rotation}deg)`,
                    cursor: scale > 1 ? (isDragging ? 'grabbing' : 'grab') : 'default'
                  }"
          @load="addEventListeners"
      />
    </div>

    <!-- 提示信息 -->
    <transition name="tip-fade">
      <div v-if="showTip" class="tip-overlay">
        <div class="tip-content">
          {{ tipMessage }}
        </div>
      </div>
    </transition>
  </div>
</template>

<style scoped lang="scss">
.preview-media {
  position: fixed;
  top: 0;
  left: 0;
  width: 100%;
  height: 100%;
  z-index: 10000;
  display: flex;
  justify-content: center;
  align-items: center;
  overflow: hidden;
  touch-action: none;
  user-select: none;

  .preview-overlay {
    position: absolute;
    top: 0;
    left: 0;
    width: 100%;
    height: 100%;
    background: linear-gradient(135deg, var(--pad-border-color-500) 0%, var(--pad-border-color-300) 100%);
    backdrop-filter: blur(4px);
    opacity: 0.95;
    transition: opacity var(--pad-transition-base);

    &:hover {
      opacity: 0.98;
    }
  }

  .preview-controls {
    position: absolute;
    top: calc(20px + env(safe-area-inset-top));
    left: 50%;
    transform: translateX(-50%);
    display: flex;
    gap: 8px;
    padding: 12px 20px;
    background-color: var(--pad-bg-color-300);
    border-radius: var(--pad-radius-xl);
    border: 1px solid var(--pad-border-color-200);
    box-shadow: var(--pad-shadow-lg);
    backdrop-filter: blur(10px);
    z-index: 10;

    @media (max-width: 768px) {
      top: calc(10px + env(safe-area-inset-top));
      padding: 10px 16px;
      gap: 6px;
    }

    .control-btn {
      display: flex;
      align-items: center;
      gap: 6px;
      padding: 8px 16px;
      background-color: var(--pad-bg-color-200);
      border: 1px solid var(--pad-border-color-100);
      border-radius: var(--pad-radius-lg);
      color: var(--pad-text-color-200);
      font-size: 14px;
      font-weight: 500;
      cursor: pointer;
      transition: all var(--pad-transition-fast);

      &:hover:not(:disabled) {
        background-color: var(--pad-bg-color-300);
        border-color: var(--pad-border-color-300);
        color: var(--pad-text-color-100);
        transform: translateY(-1px);
        box-shadow: var(--pad-shadow-sm);
      }

      &:active:not(:disabled) {
        transform: translateY(0);
      }

      &:disabled {
        opacity: 0.5;
        cursor: not-allowed;
      }

      @media (max-width: 768px) {
        padding: 6px 12px;
        font-size: 13px;

        .control-text {
          display: none;
        }
      }

      .control-icon {
        width: 18px;
        height: 18px;
        fill: currentColor;
      }

      &.close-btn {
        background-color: var(--pad-danger-color);
        border-color: var(--pad-danger-dark);
        color: var(--pad-text-color-light);

        &:hover:not(:disabled) {
          background-color: var(--pad-danger-dark);
          transform: translateY(-1px);
        }
      }
    }
  }

  .image-container {
    position: relative;
    width: 100%;
    height: 100%;
    display: flex;
    justify-content: center;
    align-items: center;
    overflow: hidden;
    border-radius: var(--pad-radius-lg);

    @media (max-width: 768px) {
      border-radius: 0;
    }

    .preview-image {
      max-width: 100%;
      max-height: 100%;
      display: block;
      object-fit: contain;
      transition: transform 0.1s ease-out;
      will-change: transform;
      transform-origin: center center; /* 确保旋转中心在图片中心 */

      &.draggable {
        cursor: grab;
      }
    }
  }

  /* 提示信息样式 */
  .tip-overlay {
    position: absolute;
    bottom: 40px;
    left: 0;
    width: 100%;
    display: flex;
    justify-content: center;
    align-items: center;
    z-index: 20;
    pointer-events: none;

    .tip-content {
      background-color: var(--pad-bg-color-400);
      color: var(--pad-text-color-100);
      padding: 12px 24px;
      border-radius: var(--pad-radius-lg);
      font-size: 16px;
      font-weight: 500;
      box-shadow: var(--pad-shadow-lg);
      border: 1px solid var(--pad-border-color-200);
      backdrop-filter: blur(10px);
      max-width: 80%;
      text-align: center;

      @media (max-width: 768px) {
        font-size: 14px;
        padding: 10px 20px;
        bottom: 60px; /* 移动端离底部稍远一点，避免和手势冲突 */
      }
    }
  }

  /* 提示信息动画 */
  .tip-fade-enter-active,
  .tip-fade-leave-active {
    transition: opacity 0.3s ease;
  }

  .tip-fade-enter-from,
  .tip-fade-leave-to {
    opacity: 0;
  }
}
</style>
