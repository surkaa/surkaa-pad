<template>
  <q-dialog
      no-refocus
      ref="dialogRef"
      maximized
      transition-show="fade"
      transition-hide="fade"
      @hide="onDialogHide"
  >
    <div
        ref="viewportRef"
        class="image-preview"
        :class="{'is-manipulating': isManipulating, 'is-zoomed': transform.scale > 1}"
        @click="handleBackdropClick"
        @dblclick.prevent="handleDoubleClick"
        @wheel.prevent="handleWheel"
        @pointerdown="handlePointerDown"
        @pointermove="handlePointerMove"
        @pointerup="handlePointerEnd"
        @pointercancel="handlePointerEnd"
    >
      <img
          v-if="!imageError"
          v-show="!showingMotionVideo"
          ref="imageRef"
          :src="src"
          alt="图片预览"
          class="image-preview__image"
          :style="imageStyle"
          draggable="false"
          @error="imageError = true"
          @click.stop
      >
      <video
          v-if="motionVideoUrl && !motionUnavailable"
          v-show="showingMotionVideo"
          :key="motionVideoUrl"
          ref="videoRef"
          :src="motionVideoUrl"
          class="image-preview__video"
          :style="imageStyle"
          autoplay
          muted
          playsinline
          preload="auto"
          @canplay="handleMotionCanPlay"
          @error="handleMotionError"
          @click.stop
      />
      <div v-if="imageError && !showingMotionVideo" class="image-preview__error">
        图片加载失败
      </div>

      <q-btn
          v-if="motionReady"
          flat
          no-caps
          dense
          :icon="showingMotionVideo ? 'photo' : 'motion_photos_on'"
          :label="showingMotionVideo ? '查看照片' : '播放动态照片'"
          class="image-preview__motion-toggle"
          :aria-label="showingMotionVideo ? '切换为静态照片' : '播放动态照片'"
          @pointerdown.stop
          @click.stop="toggleMotionMedia"
      />

      <q-btn
          flat
          round
          dense
          icon="close"
          size="lg"
          aria-label="关闭图片预览"
          class="image-preview__close"
          @pointerdown.stop
          @click.stop="onDialogCancel"
      />

      <div class="image-preview__toolbar" @pointerdown.stop @click.stop>
        <q-btn
            flat
            round
            dense
            icon="remove"
            aria-label="缩小图片"
            :disable="transform.scale <= MIN_IMAGE_PREVIEW_SCALE"
            @click="zoomFromCenter(-0.5)"
        />
        <q-btn
            flat
            no-caps
            dense
            class="image-preview__scale"
            aria-label="重置图片缩放"
            @click="resetTransform"
        >
          {{ Math.round(transform.scale * 100) }}%
        </q-btn>
        <q-btn
            flat
            round
            dense
            icon="add"
            aria-label="放大图片"
            :disable="transform.scale >= MAX_IMAGE_PREVIEW_SCALE"
            @click="zoomFromCenter(0.5)"
        />
      </div>
    </div>
  </q-dialog>
</template>

<script setup lang="ts">
import {computed, onBeforeUnmount, reactive, ref, watch} from 'vue';
import {useDialogPluginComponent} from 'quasar';
import {
  MAX_IMAGE_PREVIEW_SCALE,
  MIN_IMAGE_PREVIEW_SCALE,
  moveImagePreview,
  pinchImagePreview,
  zoomImagePreviewAtPoint,
  type ImagePreviewTransform,
  type Point,
} from '../utils/imagePreviewTransform';
import {buildMotionPhotoVideoUrl} from '../utils/motionPhoto';

const props = defineProps<{
  src: string;
}>();

defineEmits([
  ...useDialogPluginComponent.emits,
]);

const {dialogRef, onDialogHide, onDialogCancel} = useDialogPluginComponent();
const viewportRef = ref<HTMLElement>();
const imageRef = ref<HTMLImageElement>();
const videoRef = ref<HTMLVideoElement>();
const imageError = ref(false);
const motionReady = ref(false);
const motionUnavailable = ref(false);
const showMotionVideo = ref(true);
const isManipulating = ref(false);
const transform = reactive<ImagePreviewTransform>({scale: 1, x: 0, y: 0});
const pointers = new Map<number, Point>();

let lastPointer: Point | undefined;
let pinchStart: {
  transform: ImagePreviewTransform;
  midpoint: Point;
  distance: number;
} | undefined;
let gestureMoved = false;

const motionVideoUrl = computed(() => buildMotionPhotoVideoUrl(props.src));
const showingMotionVideo = computed(() => motionReady.value && showMotionVideo.value);
const visibleMediaFailed = computed(() => imageError.value && !showingMotionVideo.value);

const imageStyle = computed(() => ({
  transform: `translate3d(${transform.x}px, ${transform.y}px, 0) scale(${transform.scale})`,
}));

function resetTransform() {
  Object.assign(transform, {scale: 1, x: 0, y: 0});
}

function dimensions() {
  const viewport = viewportRef.value;
  const media = showingMotionVideo.value ? videoRef.value : imageRef.value;
  if (!viewport || !media) return undefined;

  return {
    viewport: {width: viewport.clientWidth, height: viewport.clientHeight},
    content: {width: media.clientWidth, height: media.clientHeight},
  };
}

function viewportPoint(point: Point): Point {
  const rect = viewportRef.value?.getBoundingClientRect();
  return rect
      ? {x: point.x - rect.left, y: point.y - rect.top}
      : point;
}

function setTransform(next: ImagePreviewTransform) {
  Object.assign(transform, next);
}

function zoomAt(targetScale: number, focalPoint: Point) {
  const sizes = dimensions();
  if (!sizes) return;
  setTransform(zoomImagePreviewAtPoint(
      transform,
      targetScale,
      focalPoint,
      sizes.viewport,
      sizes.content,
  ));
}

function zoomFromCenter(delta: number) {
  const sizes = dimensions();
  if (!sizes) return;
  zoomAt(transform.scale + delta, {
    x: sizes.viewport.width / 2,
    y: sizes.viewport.height / 2,
  });
}

function handleWheel(event: WheelEvent) {
  if (visibleMediaFailed.value) return;
  const scaleFactor = Math.exp(-event.deltaY * 0.002);
  zoomAt(transform.scale * scaleFactor, viewportPoint(event));
}

function handleDoubleClick(event: MouseEvent) {
  if (visibleMediaFailed.value) return;
  zoomAt(transform.scale > 1 ? 1 : 2, viewportPoint(event));
}

function pointerPair(): [Point, Point] | undefined {
  const pair = [...pointers.values()];
  return pair.length >= 2 ? [pair[0], pair[1]] : undefined;
}

function midpoint(first: Point, second: Point): Point {
  return {x: (first.x + second.x) / 2, y: (first.y + second.y) / 2};
}

function distance(first: Point, second: Point): number {
  return Math.hypot(second.x - first.x, second.y - first.y);
}

function beginPinch() {
  const pair = pointerPair();
  if (!pair) return;
  pinchStart = {
    transform: {...transform},
    midpoint: midpoint(...pair),
    distance: Math.max(1, distance(...pair)),
  };
  lastPointer = undefined;
}

function handlePointerDown(event: PointerEvent) {
  if (visibleMediaFailed.value || event.button !== 0) return;
  const point = viewportPoint(event);
  pointers.set(event.pointerId, point);
  viewportRef.value?.setPointerCapture(event.pointerId);
  gestureMoved = false;
  isManipulating.value = true;

  if (pointers.size >= 2) {
    beginPinch();
  } else {
    lastPointer = point;
  }
}

function handlePointerMove(event: PointerEvent) {
  if (!pointers.has(event.pointerId)) return;
  const point = viewportPoint(event);
  pointers.set(event.pointerId, point);
  const sizes = dimensions();
  if (!sizes) return;

  if (pointers.size >= 2 && pinchStart) {
    const pair = pointerPair();
    if (!pair) return;
    const currentMidpoint = midpoint(...pair);
    const currentDistance = distance(...pair);
    if (Math.abs(currentDistance - pinchStart.distance) > 2
        || Math.hypot(
            currentMidpoint.x - pinchStart.midpoint.x,
            currentMidpoint.y - pinchStart.midpoint.y,
        ) > 2) {
      gestureMoved = true;
    }
    setTransform(pinchImagePreview(
        pinchStart.transform,
        pinchStart.midpoint,
        currentMidpoint,
        currentDistance / pinchStart.distance,
        sizes.viewport,
        sizes.content,
    ));
    return;
  }

  if (pointers.size === 1 && lastPointer && transform.scale > 1) {
    const delta = {x: point.x - lastPointer.x, y: point.y - lastPointer.y};
    if (Math.hypot(delta.x, delta.y) > 2) gestureMoved = true;
    setTransform(moveImagePreview(
        transform,
        delta,
        sizes.viewport,
        sizes.content,
    ));
    lastPointer = point;
  }
}

function handlePointerEnd(event: PointerEvent) {
  pointers.delete(event.pointerId);
  if (viewportRef.value?.hasPointerCapture(event.pointerId)) {
    viewportRef.value.releasePointerCapture(event.pointerId);
  }

  if (pointers.size >= 2) {
    beginPinch();
  } else if (pointers.size === 1) {
    pinchStart = undefined;
    lastPointer = [...pointers.values()][0];
  } else {
    pinchStart = undefined;
    lastPointer = undefined;
    isManipulating.value = false;
  }
}

function handleBackdropClick(event: MouseEvent) {
  if (event.target === event.currentTarget && !gestureMoved) {
    onDialogCancel();
  }
  gestureMoved = false;
}

function handleResize() {
  if (transform.scale === 1) return;
  const sizes = dimensions();
  if (!sizes) return;
  setTransform(moveImagePreview(
      transform,
      {x: 0, y: 0},
      sizes.viewport,
      sizes.content,
  ));
}

function handleMotionCanPlay() {
  const video = videoRef.value;
  if (!video) return;
  motionReady.value = true;
  video.muted = true;
  if (showMotionVideo.value) {
    void video.play().catch(error => console.warn('自动播放动态照片失败:', error));
  } else {
    video.pause();
  }
}

function handleMotionError() {
  motionReady.value = false;
  motionUnavailable.value = true;
}

function toggleMotionMedia() {
  const video = videoRef.value;
  showMotionVideo.value = !showMotionVideo.value;
  if (showMotionVideo.value) {
    if (video) {
      if (video.ended) video.currentTime = 0;
      void video.play().catch(error => console.warn('播放动态照片失败:', error));
    }
  } else {
    video?.pause();
  }
}

watch(() => props.src, () => {
  imageError.value = false;
  motionReady.value = false;
  motionUnavailable.value = false;
  showMotionVideo.value = true;
  resetTransform();
}, {immediate: true});

window.addEventListener('resize', handleResize);
onBeforeUnmount(() => window.removeEventListener('resize', handleResize));
</script>

<style scoped lang="scss">
.image-preview {
  position: relative;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 100vw;
  height: 100vh;
  overflow: hidden;
  color: var(--pad-preview-text-color);
  background: var(--pad-preview-backdrop);
  cursor: default;
  touch-action: none;
  user-select: none;

  &.is-zoomed {
    cursor: grab;
  }

  &.is-manipulating.is-zoomed {
    cursor: grabbing;
  }
}

.image-preview__image,
.image-preview__video {
  display: block;
  max-width: 95vw;
  max-height: 95vh;
  object-fit: contain;
  transform-origin: center;
  will-change: transform;
  -webkit-user-drag: none;
}

.image-preview:not(.is-manipulating) .image-preview__image,
.image-preview:not(.is-manipulating) .image-preview__video {
  transition: transform 120ms ease-out;
}

.image-preview__motion-toggle {
  position: absolute;
  z-index: 2;
  top: 16px;
  left: 16px;
  border: 1px solid var(--pad-preview-control-border);
  border-radius: var(--pad-radius-full);
  color: var(--pad-preview-text-color);
  background: var(--pad-preview-control-background);
  backdrop-filter: blur(8px);
}

.image-preview__error {
  display: flex;
  align-items: center;
  justify-content: center;
  min-width: 12rem;
  min-height: 8rem;
  border-radius: 8px;
  background: var(--q-negative);
}

.image-preview__close {
  position: absolute;
  z-index: 2;
  top: 12px;
  right: 12px;
  color: var(--pad-preview-text-color);
  background: var(--pad-preview-control-background);
}

.image-preview__toolbar {
  position: absolute;
  z-index: 2;
  bottom: max(16px, env(safe-area-inset-bottom));
  left: 50%;
  display: flex;
  align-items: center;
  gap: 2px;
  padding: 4px 8px;
  color: var(--pad-preview-text-color);
  background: var(--pad-preview-control-background);
  border: 1px solid var(--pad-preview-control-border);
  border-radius: 24px;
  transform: translateX(-50%);
  backdrop-filter: blur(8px);
}

.image-preview__scale {
  min-width: 58px;
}
</style>
