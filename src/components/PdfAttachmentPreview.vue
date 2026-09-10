<template>
  <div class="pdf-preview">
    <div class="pdf-preview-toolbar">
      <q-btn flat round dense icon="chevron_left" aria-label="上一页" :disable="currentPage <= 1" @click="goToPage(currentPage - 1)"/>
      <span class="pdf-page-counter">{{ currentPage }} / {{ pageCount || '—' }}</span>
      <q-btn flat round dense icon="chevron_right" aria-label="下一页" :disable="currentPage >= pageCount" @click="goToPage(currentPage + 1)"/>
      <q-separator vertical inset class="q-mx-sm"/>
      <q-btn flat round dense icon="remove" aria-label="缩小 PDF" :disable="zoom <= MIN_ZOOM" @click="changeZoom(-ZOOM_STEP)"/>
      <span class="pdf-zoom-label">{{ Math.round(zoom * 100) }}%</span>
      <q-btn flat round dense icon="add" aria-label="放大 PDF" :disable="zoom >= MAX_ZOOM" @click="changeZoom(ZOOM_STEP)"/>
      <q-btn flat round dense icon="fit_screen" color="primary" aria-label="适应宽度" @click="fitWidth">
        <q-tooltip>适应宽度</q-tooltip>
      </q-btn>
    </div>
    <q-linear-progress
      v-if="loading"
      :value="loadProgress"
      :indeterminate="totalBytes <= 0"
      color="primary"
    />

    <div v-if="fatalError" class="pdf-preview-state pdf-preview-error">
      <q-icon name="error_outline" size="36px"/>
      <span>{{ fatalError }}</span>
    </div>
    <div v-else ref="scrollElement" class="pdf-page-scroll" @scroll.passive="handleScroll">
      <div v-if="loading" class="pdf-preview-state">
        <q-spinner color="primary" size="36px"/>
        <span>{{ loadProgressText }}</span>
      </div>
      <div v-else class="pdf-page-list">
        <div
          v-for="pageNumber in pageCount"
          :key="pageNumber"
          :ref="element => setPageElement(pageNumber, element as HTMLElement | null)"
          class="pdf-page"
          :style="pageStyle"
          :data-page-number="pageNumber"
        >
          <canvas :ref="element => setPageCanvas(pageNumber, element as HTMLCanvasElement | null)"/>
          <q-spinner v-if="renderingPages.has(pageNumber)" color="primary" size="28px"/>
          <div v-else-if="renderErrors.has(pageNumber)" class="pdf-page-error">
            第 {{ pageNumber }} 页渲染失败
          </div>
          <span class="pdf-page-number">{{ pageNumber }}</span>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import {computed, nextTick, onBeforeUnmount, onMounted, ref} from 'vue';
import type {
  PDFDocumentLoadingTask,
  PDFDocumentProxy,
  RenderTask,
} from 'pdfjs-dist';
import pdfWorkerUrl from 'pdfjs-dist/build/pdf.worker.min.mjs?url';
import {formatError} from '../utils/formatError';
import {
  calculatePdfOutputScale,
  calculatePdfPageWidth,
} from '../utils/pdfPreviewLayout';

const props = defineProps<{url: string}>();

const MIN_ZOOM = 0.5;
const MAX_ZOOM = 3;
const ZOOM_STEP = 0.25;
const PDF_RANGE_CHUNK_BYTES = 1024 * 1024;
const scrollElement = ref<HTMLElement>();
const loading = ref(true);
const loadedBytes = ref(0);
const totalBytes = ref(0);
const fatalError = ref('');
const pageCount = ref(0);
const currentPage = ref(1);
const zoom = ref(1);
const availableWidth = ref(800);
const renderingPages = ref(new Set<number>());
const renderedPages = ref(new Set<number>());
const renderErrors = ref(new Set<number>());
const pageElements = new Map<number, HTMLElement>();
const pageCanvases = new Map<number, HTMLCanvasElement>();
const renderTasks = new Map<number, RenderTask>();
let loadingTask: PDFDocumentLoadingTask | null = null;
let documentProxy: PDFDocumentProxy | null = null;
let pageObserver: IntersectionObserver | null = null;
let resizeObserver: ResizeObserver | null = null;
let renderRevision = 0;
let scrollFrame = 0;
let resizeFrame = 0;
let disposed = false;

const loadProgress = computed(() => (
  totalBytes.value > 0 ? Math.min(1, loadedBytes.value / totalBytes.value) : 0
));
const loadProgressText = computed(() => (
  totalBytes.value > 0
    ? `正在载入 PDF… ${Math.round(loadProgress.value * 100)}%`
    : '正在载入 PDF…'
));
const pageWidth = computed(() => calculatePdfPageWidth(availableWidth.value, zoom.value));
const pageStyle = computed(() => ({
  width: `${pageWidth.value}px`,
  minHeight: `${Math.round(pageWidth.value * 1.414)}px`,
}));

onMounted(() => void loadPdf());
onBeforeUnmount(cleanup);

async function loadPdf() {
  loading.value = true;
  fatalError.value = '';
  try {
    const pdfjs = await import('pdfjs-dist');
    if (disposed) return;
    pdfjs.GlobalWorkerOptions.workerSrc = pdfWorkerUrl;
    loadingTask = pdfjs.getDocument({
      url: props.url,
      rangeChunkSize: PDF_RANGE_CHUNK_BYTES,
      disableStream: true,
      disableAutoFetch: true,
    });
    loadingTask.onProgress = ({loaded, total}: {loaded: number; total: number}) => {
      loadedBytes.value = loaded;
      totalBytes.value = total;
    };
    loadingTask.onPassword = () => {
      fatalError.value = '该 PDF 需要打开密码，目前暂不支持预览';
      void loadingTask?.destroy();
    };
    const loadedDocument = await loadingTask.promise;
    if (disposed) {
      await loadedDocument.cleanup();
      return;
    }
    documentProxy = loadedDocument;
    pageCount.value = documentProxy.numPages;
    if (pageCount.value < 1) throw new Error('PDF 中没有可显示的页面');
    loading.value = false;
    await nextTick();
    setupObservers();
  } catch (error) {
    if (disposed) return;
    if (!fatalError.value) fatalError.value = `PDF 载入失败：${formatError(error)}`;
    loading.value = false;
  }
}

function setupObservers() {
  const root = scrollElement.value;
  if (!root) return;
  updateAvailableWidth();
  pageObserver = new IntersectionObserver(entries => {
    for (const entry of entries) {
      if (!entry.isIntersecting) continue;
      const pageNumber = Number((entry.target as HTMLElement).dataset.pageNumber);
      if (Number.isInteger(pageNumber)) void renderPage(pageNumber);
    }
  }, {root, rootMargin: '600px 0px', threshold: 0.01});
  for (const element of pageElements.values()) pageObserver.observe(element);

  resizeObserver = new ResizeObserver(() => {
    cancelAnimationFrame(resizeFrame);
    resizeFrame = requestAnimationFrame(() => {
      const oldWidth = availableWidth.value;
      updateAvailableWidth();
      if (Math.abs(oldWidth - availableWidth.value) > 1) rerenderVisiblePages();
    });
  });
  resizeObserver.observe(root);
  rerenderVisiblePages();
}

async function renderPage(pageNumber: number) {
  const pdf = documentProxy;
  const canvas = pageCanvases.get(pageNumber);
  if (!pdf || !canvas || renderedPages.value.has(pageNumber) || renderingPages.value.has(pageNumber)) {
    return;
  }

  const revision = renderRevision;
  let renderTask: RenderTask | undefined;
  renderingPages.value = new Set(renderingPages.value).add(pageNumber);
  renderErrors.value = withoutValue(renderErrors.value, pageNumber);
  try {
    const page = await pdf.getPage(pageNumber);
    if (revision !== renderRevision) return;
    const originalViewport = page.getViewport({scale: 1});
    const viewport = page.getViewport({scale: pageWidth.value / originalViewport.width});
    const outputScale = calculatePdfOutputScale(
      viewport.width,
      viewport.height,
      window.devicePixelRatio || 1,
    );
    canvas.width = Math.max(1, Math.floor(viewport.width * outputScale));
    canvas.height = Math.max(1, Math.floor(viewport.height * outputScale));
    canvas.style.width = `${Math.floor(viewport.width)}px`;
    canvas.style.height = `${Math.floor(viewport.height)}px`;
    const pageElement = pageElements.get(pageNumber);
    if (pageElement) pageElement.style.minHeight = `${Math.floor(viewport.height)}px`;
    const context = canvas.getContext('2d');
    if (!context) throw new Error('当前设备无法创建 PDF 画布');
    renderTask = page.render({
      canvas: null,
      canvasContext: context,
      viewport,
      annotationMode: 0,
      transform: outputScale === 1 ? undefined : [outputScale, 0, 0, outputScale, 0, 0],
    });
    renderTasks.set(pageNumber, renderTask);
    await renderTask.promise;
    if (revision !== renderRevision) return;
    renderedPages.value = new Set(renderedPages.value).add(pageNumber);
  } catch (error) {
    if (revision === renderRevision && !isCancelledRender(error)) {
      console.error(`渲染 PDF 第 ${pageNumber} 页失败:`, error);
      renderErrors.value = new Set(renderErrors.value).add(pageNumber);
    }
  } finally {
    if (renderTask && renderTasks.get(pageNumber) === renderTask) {
      renderTasks.delete(pageNumber);
    }
    if (revision === renderRevision) {
      renderingPages.value = withoutValue(renderingPages.value, pageNumber);
    }
  }
}

function changeZoom(delta: number) {
  zoom.value = Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, zoom.value + delta));
  rerenderVisiblePages();
}

function fitWidth() {
  zoom.value = 1;
  rerenderVisiblePages();
}

function rerenderVisiblePages() {
  renderRevision += 1;
  for (const task of renderTasks.values()) task.cancel();
  renderTasks.clear();
  renderedPages.value = new Set();
  renderingPages.value = new Set();
  renderErrors.value = new Set();
  for (const canvas of pageCanvases.values()) {
    canvas.width = 0;
    canvas.height = 0;
    canvas.style.width = '';
    canvas.style.height = '';
  }
  void nextTick(() => {
    const root = scrollElement.value;
    if (!root) return;
    const rootRect = root.getBoundingClientRect();
    for (const [pageNumber, element] of pageElements) {
      const rect = element.getBoundingClientRect();
      if (rect.bottom >= rootRect.top - 600 && rect.top <= rootRect.bottom + 600) {
        void renderPage(pageNumber);
      }
    }
  });
}

function goToPage(pageNumber: number) {
  const target = pageElements.get(Math.min(pageCount.value, Math.max(1, pageNumber)));
  target?.scrollIntoView({behavior: 'smooth', block: 'start'});
}

function handleScroll() {
  cancelAnimationFrame(scrollFrame);
  scrollFrame = requestAnimationFrame(() => {
    const root = scrollElement.value;
    if (!root) return;
    const rootRect = root.getBoundingClientRect();
    const reference = rootRect.top + Math.min(rootRect.height / 3, 180);
    let closestPage = currentPage.value;
    let closestDistance = Number.POSITIVE_INFINITY;
    for (const [pageNumber, element] of pageElements) {
      const rect = element.getBoundingClientRect();
      const distance = Math.abs(rect.top - reference);
      if (distance < closestDistance) {
        closestDistance = distance;
        closestPage = pageNumber;
      }
    }
    currentPage.value = closestPage;
  });
}

function setPageElement(pageNumber: number, element: HTMLElement | null) {
  const oldElement = pageElements.get(pageNumber);
  if (oldElement) pageObserver?.unobserve(oldElement);
  if (!element) {
    pageElements.delete(pageNumber);
    return;
  }
  pageElements.set(pageNumber, element);
  pageObserver?.observe(element);
}

function setPageCanvas(pageNumber: number, canvas: HTMLCanvasElement | null) {
  if (canvas) pageCanvases.set(pageNumber, canvas);
  else pageCanvases.delete(pageNumber);
}

function updateAvailableWidth() {
  if (scrollElement.value) availableWidth.value = scrollElement.value.clientWidth;
}

function cleanup() {
  disposed = true;
  renderRevision += 1;
  cancelAnimationFrame(scrollFrame);
  cancelAnimationFrame(resizeFrame);
  pageObserver?.disconnect();
  resizeObserver?.disconnect();
  for (const task of renderTasks.values()) task.cancel();
  renderTasks.clear();
  void documentProxy?.cleanup();
  void loadingTask?.destroy();
  documentProxy = null;
  loadingTask = null;
}

function withoutValue(values: Set<number>, value: number): Set<number> {
  const result = new Set(values);
  result.delete(value);
  return result;
}

function isCancelledRender(error: unknown): boolean {
  return error instanceof Error && error.name === 'RenderingCancelledException';
}
</script>

<style scoped lang="scss">
.pdf-preview {
  display: flex;
  flex: 1;
  flex-direction: column;
  min-height: 0;
  background: var(--pad-bg-color-100);
}

.pdf-preview-toolbar {
  display: flex;
  flex: none;
  align-items: center;
  justify-content: center;
  min-height: 46px;
  color: var(--pad-text-color-200);
  background: var(--pad-bg-color-200);
}

.pdf-page-counter,
.pdf-zoom-label {
  min-width: 64px;
  text-align: center;
  font-variant-numeric: tabular-nums;
}

.pdf-zoom-label {
  min-width: 48px;
}

.pdf-page-scroll {
  flex: 1;
  min-height: 0;
  overflow: auto;
}

.pdf-page-list {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 18px;
  min-width: max-content;
  padding: 18px;
}

.pdf-page {
  position: relative;
  display: grid;
  place-items: center;
  overflow: hidden;
  border: 1px solid var(--pad-border-color-100);
  background: #fff;
  box-shadow: 0 3px 14px var(--pad-shadow-color-200);
}

.pdf-page canvas {
  display: block;
  max-width: none;
}

.pdf-page-number {
  position: absolute;
  right: 8px;
  bottom: 6px;
  padding: 2px 6px;
  border-radius: 5px;
  color: #444;
  background: rgb(255 255 255 / 78%);
  font-size: 11px;
}

.pdf-page-error {
  color: var(--q-negative);
}

.pdf-preview-state {
  display: flex;
  flex: 1;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 12px;
  min-height: 260px;
  padding: 24px;
  color: var(--pad-text-color-300);
}

.pdf-preview-error {
  color: var(--q-negative);
}

@media (max-width: 600px) {
  .pdf-preview-toolbar {
    justify-content: flex-start;
    overflow-x: auto;
    padding-inline: 4px;
  }

  .pdf-page-list {
    gap: 12px;
    padding: 12px;
  }
}
</style>
