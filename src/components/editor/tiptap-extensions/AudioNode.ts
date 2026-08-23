import {Node as TiptapNode, mergeAttributes} from '@tiptap/vue-3'
import type {Node as ProseMirrorNode} from '@tiptap/pm/model'
import type {NodeView} from '@tiptap/pm/view'
import {platform} from '@tauri-apps/plugin-os'
import type WaveSurfer from 'wavesurfer.js'
import type {AttachmentMeta, AudioWaveform} from '../../../bindings'
import {
  AUDIO_WAVEFORM_PEAK_COUNT,
  AUDIO_WAVEFORM_VERSION,
  calculateAudioProgress,
  calculateAudioPlayerWidth,
  calculateCenteredWaveformBars,
  decodeSignedWaveformPeaks,
  encodeSignedWaveformPeaks,
  MAX_AUTO_WAVEFORM_SOURCE_BYTES,
} from '../../../utils/audioWaveform'

export interface AudioNodeOptions {
  getAttachment: (attachmentId: string) => AttachmentMeta | null
  onAudioInfoGenerated: (
    attachmentId: string,
    durationMs: number,
    waveform: AudioWaveform,
  ) => void | Promise<void>
}

let waveformGenerationQueue: Promise<void> = Promise.resolve()

function enqueueWaveformGeneration(task: () => Promise<void>): Promise<void> {
  const run = waveformGenerationQueue.then(task, task)
  waveformGenerationQueue = run.catch(() => undefined)
  return run
}

declare module '@tiptap/vue-3' {
  interface Commands<ReturnType> {
    audioNode: {
      insertAudio: (attrs: { id: string; src?: string }) => ReturnType
    }
  }
}

function createAudioNodeView(
  initialNode: ProseMirrorNode,
  options: AudioNodeOptions,
): NodeView {
  let currentNode = initialNode
  const isAndroid = platform() === 'android'
  let wavesurfer: WaveSurfer | null = null
  let nativeAudio: HTMLAudioElement | null = null
  let nativeCanvas: HTMLCanvasElement | null = null
  let nativePeaks: number[] = []
  let nativeResizeObserver: ResizeObserver | null = null
  let generationStarted = false
  let releaseGeneration: (() => void) | null = null
  let loadVersion = 0
  let destroyed = false
  let visible = false
  let intersectionObserver: IntersectionObserver | null = null

  const dom = document.createElement('div')
  const playButton = document.createElement('button')
  const playIcon = document.createElement('span')
  const waveformContainer = document.createElement('div')
  const time = document.createElement('span')
  const status = document.createElement('span')

  dom.className = 'editor-audio-attachment'
  dom.contentEditable = 'false'
  playButton.type = 'button'
  playButton.className = 'editor-audio-play'
  playButton.title = '播放录音'
  playButton.setAttribute('aria-label', '播放录音')
  playIcon.className = 'editor-audio-play-icon'
  playIcon.textContent = '▶'
  playButton.append(playIcon)
  waveformContainer.className = 'editor-audio-waveform'
  time.className = 'editor-audio-time'
  status.className = 'editor-audio-status'
  dom.append(playButton, waveformContainer, time, status)
  updatePlayerWidth(0)

  const preventFocus = (event: Event) => {
    event.stopPropagation()
  }
  playButton.addEventListener('pointerdown', preventFocus)
  playButton.addEventListener('click', event => {
    event.preventDefault()
    event.stopPropagation()
    if (wavesurfer) {
      void wavesurfer.playPause().catch(error => {
        console.error('播放音频失败:', error)
      })
    } else if (nativeAudio) {
      if (nativeAudio.paused) {
        void nativeAudio.play().catch(error => console.error('播放音频失败:', error))
      } else {
        nativeAudio.pause()
      }
    }
  })
  waveformContainer.addEventListener('pointerdown', preventFocus)
  waveformContainer.addEventListener('click', seekNativeAudio)

  function resetPlayer() {
    loadVersion += 1
    releaseGeneration?.()
    releaseGeneration = null
    wavesurfer?.destroy()
    wavesurfer = null
    nativeResizeObserver?.disconnect()
    nativeResizeObserver = null
    if (nativeAudio) {
      nativeAudio.pause()
      nativeAudio.removeAttribute('src')
      nativeAudio.load()
    }
    nativeAudio = null
    nativeCanvas = null
    nativePeaks = []
    waveformContainer.classList.remove('editor-audio-waveform--native')
    waveformContainer.replaceChildren()
    time.textContent = '--:--'
    status.textContent = ''
    playButton.disabled = true
    playIcon.textContent = '▶'
    generationStarted = false
  }

  function mountNativePlayer(
    src: string,
    message: string,
    waveform: AudioWaveform | null = null,
    storedDurationSeconds = 0,
  ) {
    wavesurfer?.destroy()
    wavesurfer = null
    waveformContainer.replaceChildren()

    waveformContainer.classList.add('editor-audio-waveform--native')
    const canvas = document.createElement('canvas')
    canvas.className = 'editor-audio-native-waveform'
    const audio = document.createElement('audio')
    audio.preload = 'metadata'
    audio.src = src
    audio.dataset.id = String(currentNode.attrs.id || '')
    audio.className = 'editor-audio-native-backend'
    nativeAudio = audio
    nativeCanvas = canvas
    nativePeaks = waveform
      ? decodeSignedWaveformPeaks(waveform.peaks)
      : Array.from({length: AUDIO_WAVEFORM_PEAK_COUNT}, () => 0)
    waveformContainer.append(canvas, audio)

    const resolveDuration = () => Number.isFinite(audio.duration) && audio.duration > 0
      ? audio.duration
      : storedDurationSeconds
    const markReady = () => {
      if (audio !== nativeAudio) return
      const duration = resolveDuration()
      playButton.disabled = false
      updatePlayerWidth(duration)
      updateTime(audio.currentTime, duration)
      requestAnimationFrame(drawNativeWaveform)
    }
    audio.addEventListener('loadedmetadata', markReady)
    audio.addEventListener('canplay', markReady)
    audio.addEventListener('timeupdate', () => {
      if (audio !== nativeAudio) return
      updateTime(audio.currentTime, resolveDuration())
      drawNativeWaveform()
    })
    audio.addEventListener('play', () => setPlayingState(true))
    audio.addEventListener('pause', () => setPlayingState(false))
    audio.addEventListener('ended', () => setPlayingState(false))
    audio.addEventListener('error', () => {
      if (audio !== nativeAudio) return
      playButton.disabled = true
      status.textContent = '音频加载失败'
    })
    if (typeof ResizeObserver === 'function') {
      nativeResizeObserver = new ResizeObserver(() => drawNativeWaveform())
      nativeResizeObserver.observe(waveformContainer)
    }

    status.textContent = message
    playButton.hidden = false
    time.hidden = false
    if (storedDurationSeconds > 0) {
      playButton.disabled = false
      updateTime(0, storedDurationSeconds)
    }
    requestAnimationFrame(drawNativeWaveform)
  }

  function seekNativeAudio(event: MouseEvent) {
    const audio = nativeAudio
    if (!audio || !Number.isFinite(audio.duration) || audio.duration <= 0) return
    event.preventDefault()
    event.stopPropagation()
    const bounds = waveformContainer.getBoundingClientRect()
    if (bounds.width <= 0) return
    const progress = Math.max(0, Math.min(1, (event.clientX - bounds.left) / bounds.width))
    audio.currentTime = progress * audio.duration
  }

  function setPlayingState(playing: boolean) {
    playIcon.textContent = playing ? '❚❚' : '▶'
    playButton.title = playing ? '暂停录音' : '播放录音'
    playButton.setAttribute('aria-label', playButton.title)
  }

  function drawNativeWaveform() {
    const canvas = nativeCanvas
    if (!canvas) return
    const cssWidth = waveformContainer.getBoundingClientRect().width
    if (cssWidth <= 0) return
    const cssHeight = 44
    const pixelRatio = window.devicePixelRatio || 1
    const width = Math.max(1, Math.round(cssWidth * pixelRatio))
    const height = Math.max(1, Math.round(cssHeight * pixelRatio))
    if (canvas.width !== width || canvas.height !== height) {
      canvas.width = width
      canvas.height = height
      canvas.style.width = `${cssWidth}px`
      canvas.style.height = `${cssHeight}px`
    }
    const context = canvas.getContext('2d')
    if (!context) return
    const styles = getComputedStyle(dom)
    const waveColor = styles.getPropertyValue('--pad-text-color-400').trim() || '#8292a6'
    const progressColor = styles.getPropertyValue('--pad-primary-color').trim() || '#c9a37f'
    const progress = nativeAudio
      ? calculateAudioProgress(nativeAudio.currentTime, nativeAudio.duration)
      : 0

    context.clearRect(0, 0, width, height)
    context.fillStyle = waveColor
    renderCenteredWaveform([nativePeaks], context)
    if (progress <= 0) return
    context.save()
    context.beginPath()
    context.rect(0, 0, width * progress, height)
    context.clip()
    context.fillStyle = progressColor
    renderCenteredWaveform([nativePeaks], context)
    context.restore()
  }

  async function initialize(node: ProseMirrorNode) {
    currentNode = node
    resetPlayer()
    playButton.hidden = false
    time.hidden = false
    const version = loadVersion
    const attachmentId = String(node.attrs.id || '')
    const src = String(node.attrs.src || '')
    dom.dataset.id = attachmentId
    if (!attachmentId || !src) {
      status.textContent = '音频地址不可用'
      return
    }

    const attachment = options.getAttachment(attachmentId)
    const audioInfo = attachment?.contentInfo?.type === 'audio'
      ? attachment.contentInfo
      : null
    const storedWaveform = audioInfo?.waveform?.version === AUDIO_WAVEFORM_VERSION
      && audioInfo.waveform.peaks.length > 0
      ? audioInfo.waveform
      : null
    const storedDurationSeconds = audioInfo?.durationMs
      ? audioInfo.durationMs / 1_000
      : 0
    updatePlayerWidth(storedDurationSeconds)

    if (!storedWaveform && attachment && attachment.size > MAX_AUTO_WAVEFORM_SOURCE_BYTES) {
      mountNativePlayer(src, '附件较大，暂不自动生成音波', null, storedDurationSeconds)
      return
    }

    // Android WebView 的 Web Audio 解码支持弱于原生媒体播放。已有音波时直接让
    // 原生 audio 负责播放，自定义 Canvas 负责显示，避免失败后暴露原生控件。
    if (isAndroid && storedWaveform) {
      mountNativePlayer(src, '', storedWaveform, storedDurationSeconds)
      return
    }

    if (!storedWaveform) {
      status.textContent = '等待生成音波…'
      await enqueueWaveformGeneration(async () => {
        if (destroyed || version !== loadVersion) return
        status.textContent = '正在生成音波…'
        await mountWaveSurfer(
          version,
          attachmentId,
          src,
          null,
          0,
        )
      })
      return
    }

    status.textContent = '正在加载…'
    await mountWaveSurfer(
      version,
      attachmentId,
      src,
      storedWaveform,
      storedDurationSeconds,
    )
  }

  async function mountWaveSurfer(
    version: number,
    attachmentId: string,
    src: string,
    storedWaveform: AudioWaveform | null,
    storedDurationSeconds: number,
  ) {
    try {
      const WaveSurfer = (await import('wavesurfer.js')).default
      if (destroyed || version !== loadVersion) return
      const styles = getComputedStyle(dom)
      const waveColor = styles.getPropertyValue('--pad-text-color-400').trim() || '#8292a6'
      const progressColor = styles.getPropertyValue('--pad-primary-color').trim() || '#c9a37f'
      wavesurfer = WaveSurfer.create({
        container: waveformContainer,
        url: src,
        height: 44,
        waveColor,
        progressColor,
        cursorWidth: 0,
        barWidth: 3,
        barGap: 3,
        barRadius: 3,
        barMinHeight: 2,
        normalize: true,
        dragToSeek: true,
        renderFunction: renderCenteredWaveform,
        peaks: storedWaveform
          ? [decodeSignedWaveformPeaks(storedWaveform.peaks)]
          : undefined,
        duration: storedWaveform && storedDurationSeconds > 0
          ? storedDurationSeconds
          : undefined,
      })
      if (!storedWaveform) {
        const generationFinished = new Promise<void>(resolve => {
          let released = false
          const settleGeneration = () => {
            if (released) return
            released = true
            releaseGeneration = null
            resolve()
          }
          releaseGeneration = settleGeneration
          bindWaveSurferEvents(
            wavesurfer!,
            attachmentId,
            false,
            storedWaveform,
            storedDurationSeconds,
            settleGeneration,
          )
        })
        await generationFinished
      } else {
        bindWaveSurferEvents(
          wavesurfer,
          attachmentId,
          true,
          storedWaveform,
          storedDurationSeconds,
        )
      }
    } catch (error) {
      if (destroyed || version !== loadVersion) return
      console.error('初始化音波失败:', error)
      mountNativePlayer(src, '音波不可用', storedWaveform, storedDurationSeconds)
    }
  }

  function bindWaveSurferEvents(
    player: WaveSurfer,
    attachmentId: string,
    hasStoredWaveform: boolean,
    fallbackWaveform: AudioWaveform | null,
    fallbackDurationSeconds: number,
    onGenerationSettled?: () => void,
  ) {
    player.on('ready', duration => {
      if (player !== wavesurfer) return
      status.textContent = ''
      playButton.disabled = false
      updatePlayerWidth(duration)
      updateTime(0, duration)
      onGenerationSettled?.()
    })
    player.on('play', () => {
      playIcon.textContent = '❚❚'
      playButton.title = '暂停录音'
      playButton.setAttribute('aria-label', '暂停录音')
    })
    player.on('pause', () => {
      playIcon.textContent = '▶'
      playButton.title = '播放录音'
      playButton.setAttribute('aria-label', '播放录音')
    })
    player.on('timeupdate', currentTime => updateTime(currentTime, player.getDuration()))
    player.on('loading', progress => {
      if (hasStoredWaveform) status.textContent = `正在加载 ${progress}%`
    })
    player.on('decode', duration => {
      if (hasStoredWaveform || generationStarted || player !== wavesurfer) return
      generationStarted = true
      try {
        const peaks = player.exportPeaks({channels: 1, maxLength: AUDIO_WAVEFORM_PEAK_COUNT})[0]
        const waveform: AudioWaveform = {
          version: AUDIO_WAVEFORM_VERSION,
          peaks: encodeSignedWaveformPeaks(peaks),
        }
        if (!waveform.peaks.length) return
        void Promise.resolve(options.onAudioInfoGenerated(
          attachmentId,
          Math.max(0, Math.round(duration * 1_000)),
          waveform,
        )).catch(error => console.error('静默保存音波失败:', error))
      } catch (error) {
        console.error('生成音波数据失败:', error)
      } finally {
        onGenerationSettled?.()
      }
    })
    player.on('error', error => {
      if (player !== wavesurfer) return
      console.error('加载音频失败:', error)
      onGenerationSettled?.()
      mountNativePlayer(
        String(currentNode.attrs.src || ''),
        '音波不可用',
        fallbackWaveform,
        fallbackDurationSeconds,
      )
    })
  }

  function updateTime(currentSeconds: number, durationSeconds: number) {
    const current = Number.isFinite(currentSeconds) ? currentSeconds : 0
    const duration = Number.isFinite(durationSeconds) ? durationSeconds : 0
    time.textContent = current > 0
      ? `${formatDuration(current)} / ${formatDuration(duration)}`
      : formatDuration(duration)
  }

  function updatePlayerWidth(durationSeconds: number) {
    const durationMs = Number.isFinite(durationSeconds) ? durationSeconds * 1_000 : 0
    dom.style.setProperty('--editor-audio-width', `${calculateAudioPlayerWidth(durationMs)}px`)
  }

  if (typeof IntersectionObserver === 'function') {
    intersectionObserver = new IntersectionObserver(entries => {
      if (!entries.some(entry => entry.isIntersecting)) return
      visible = true
      intersectionObserver?.disconnect()
      intersectionObserver = null
      void initialize(currentNode)
    })
    queueMicrotask(() => {
      if (!destroyed) intersectionObserver?.observe(dom)
    })
  } else {
    visible = true
    void initialize(initialNode)
  }

  return {
    dom,
    update(node) {
      if (node.type !== currentNode.type) return false
      if (node.attrs.id !== currentNode.attrs.id || node.attrs.src !== currentNode.attrs.src) {
        currentNode = node
        if (visible) void initialize(node)
      } else {
        currentNode = node
      }
      return true
    },
    stopEvent: event => dom.contains(event.target as Node),
    ignoreMutation: () => true,
    destroy() {
      destroyed = true
      intersectionObserver?.disconnect()
      intersectionObserver = null
      resetPlayer()
      playButton.removeEventListener('pointerdown', preventFocus)
      waveformContainer.removeEventListener('pointerdown', preventFocus)
      waveformContainer.removeEventListener('click', seekNativeAudio)
    },
  }
}

function renderCenteredWaveform(
  peaks: Array<Float32Array | number[]>,
  context: CanvasRenderingContext2D,
) {
  const cssWidth = Number.parseFloat(context.canvas.style.width)
  const pixelRatio = Number.isFinite(cssWidth) && cssWidth > 0
    ? context.canvas.width / cssWidth
    : window.devicePixelRatio || 1
  const bars = calculateCenteredWaveformBars(
    peaks,
    context.canvas.width,
    context.canvas.height,
    pixelRatio,
  )
  context.beginPath()
  for (const bar of bars) {
    const radius = Math.min(bar.width, bar.height) / 2
    context.roundRect(bar.x, bar.y, bar.width, bar.height, radius)
  }
  context.fill()
  context.closePath()
}

function formatDuration(seconds: number): string {
  const rounded = Math.max(0, Math.floor(seconds))
  const hours = Math.floor(rounded / 3_600)
  const minutes = Math.floor((rounded % 3_600) / 60)
  const remainingSeconds = rounded % 60
  return hours > 0
    ? `${hours}:${minutes.toString().padStart(2, '0')}:${remainingSeconds.toString().padStart(2, '0')}`
    : `${minutes}:${remainingSeconds.toString().padStart(2, '0')}`
}

export const AudioNode = TiptapNode.create<AudioNodeOptions>({
  name: 'audioNode',
  group: 'block',
  selectable: true,
  draggable: true,
  atom: true,

  addOptions() {
    return {
      getAttachment: () => null,
      onAudioInfoGenerated: () => undefined,
    }
  },

  addAttributes() {
    return {
      id: {default: null},
      src: {default: null},
    }
  },

  parseHTML() {
    return [{
      tag: 'audio[data-id]',
      getAttrs: el => ({
        id: (el as HTMLElement).getAttribute('data-id'),
        src: (el as HTMLElement).getAttribute('src'),
      }),
    }]
  },

  renderHTML({node}) {
    return ['audio', mergeAttributes({
      src: node.attrs.src || '',
      'data-id': node.attrs.id,
      controls: 'true',
    })]
  },

  addNodeView() {
    return ({node}) => createAudioNodeView(node, this.options)
  },

  addCommands() {
    return {
      insertAudio: attrs => ({commands}) => commands.insertContent({type: this.name, attrs}),
    }
  },
})
