import type {AudioWaveform} from '../bindings'

export const AUDIO_WAVEFORM_VERSION = 1
export const AUDIO_WAVEFORM_PEAK_COUNT = 128
export const MAX_AUTO_WAVEFORM_SOURCE_BYTES = 20 * 1024 * 1024
export const MIN_AUDIO_PLAYER_WIDTH_PX = 220

const AUDIO_PLAYER_WIDTH_PER_SECOND_PX = 6
const WAVEFORM_BAR_WIDTH_PX = 3
const WAVEFORM_BAR_GAP_PX = 3
const WAVEFORM_BAR_MIN_HEIGHT_PX = 2
const WAVEFORM_BAR_MAX_HEIGHT_RATIO = 0.82

export interface GeneratedAudioWaveform {
  durationMs: number
  waveform: AudioWaveform
}

export interface CenteredWaveformBar {
  x: number
  y: number
  width: number
  height: number
}

export function calculateAudioPlayerWidth(durationMs: number): number {
  const safeDurationMs = Number.isFinite(durationMs) ? Math.max(0, durationMs) : 0
  return Math.round(MIN_AUDIO_PLAYER_WIDTH_PX + safeDurationMs / 1_000 * AUDIO_PLAYER_WIDTH_PER_SECOND_PX)
}

export function calculateCenteredWaveformBars(
  channels: readonly ArrayLike<number>[],
  canvasWidth: number,
  canvasHeight: number,
  pixelRatio = 1,
): CenteredWaveformBar[] {
  const width = Number.isFinite(canvasWidth) ? Math.max(0, canvasWidth) : 0
  const height = Number.isFinite(canvasHeight) ? Math.max(0, canvasHeight) : 0
  const ratio = Number.isFinite(pixelRatio) ? Math.max(1, pixelRatio) : 1
  const sampleCount = channels.reduce((length, channel) => Math.max(length, channel.length), 0)
  if (!width || !height || !sampleCount) return []

  const barWidth = WAVEFORM_BAR_WIDTH_PX * ratio
  const barGap = WAVEFORM_BAR_GAP_PX * ratio
  const availableBarCount = Math.max(1, Math.floor((width + barGap) / (barWidth + barGap)))
  const barCount = Math.min(availableBarCount, sampleCount)
  const amplitudes: number[] = []
  let maxAmplitude = 0

  for (let barIndex = 0; barIndex < barCount; barIndex += 1) {
    const start = Math.floor(barIndex * sampleCount / barCount)
    const end = Math.max(start + 1, Math.ceil((barIndex + 1) * sampleCount / barCount))
    let amplitude = 0
    for (const channel of channels) {
      const channelEnd = Math.min(end, channel.length)
      for (let sampleIndex = start; sampleIndex < channelEnd; sampleIndex += 1) {
        const sample = channel[sampleIndex]
        if (Number.isFinite(sample)) amplitude = Math.max(amplitude, Math.abs(sample))
      }
    }
    amplitudes.push(amplitude)
    maxAmplitude = Math.max(maxAmplitude, amplitude)
  }

  const totalWidth = barCount * barWidth + (barCount - 1) * barGap
  const startX = Math.max(0, (width - totalWidth) / 2)
  const minBarHeight = Math.min(height, WAVEFORM_BAR_MIN_HEIGHT_PX * ratio)
  const maxBarHeight = Math.max(minBarHeight, height * WAVEFORM_BAR_MAX_HEIGHT_RATIO)

  return amplitudes.map((amplitude, index) => {
    const barHeight = maxAmplitude > 0
      ? Math.max(minBarHeight, amplitude / maxAmplitude * maxBarHeight)
      : minBarHeight
    return {
      x: startX + index * (barWidth + barGap),
      y: (height - barHeight) / 2,
      width: barWidth,
      height: barHeight,
    }
  })
}

export function encodeSignedWaveformPeaks(peaks: readonly number[]): number[] {
  return peaks.map(peak => {
    const normalized = Number.isFinite(peak) ? Math.max(-1, Math.min(1, peak)) : 0
    return normalized <= 0
      ? Math.round(128 + normalized * 128)
      : Math.round(128 + normalized * 127)
  })
}

export function decodeSignedWaveformPeaks(peaks: readonly number[]): number[] {
  return peaks.map(peak => {
    const encoded = Math.max(0, Math.min(255, Math.round(peak)))
    return encoded <= 128
      ? (encoded - 128) / 128
      : (encoded - 128) / 127
  })
}

export function extractSignedWaveformPeaks(
  channels: readonly Float32Array[],
  maxLength = AUDIO_WAVEFORM_PEAK_COUNT,
): number[] {
  const sampleCount = channels.reduce((length, channel) => Math.max(length, channel.length), 0)
  if (!sampleCount || maxLength <= 0) return []

  const peakCount = Math.min(Math.floor(maxLength), sampleCount)
  const samplesPerPeak = sampleCount / peakCount
  const result: number[] = []
  for (let peakIndex = 0; peakIndex < peakCount; peakIndex += 1) {
    const start = Math.floor(peakIndex * samplesPerPeak)
    const end = Math.max(start + 1, Math.ceil((peakIndex + 1) * samplesPerPeak))
    let strongest = 0
    for (const channel of channels) {
      const channelEnd = Math.min(end, channel.length)
      for (let sampleIndex = start; sampleIndex < channelEnd; sampleIndex += 1) {
        const sample = channel[sampleIndex]
        if (Number.isFinite(sample) && Math.abs(sample) > Math.abs(strongest)) {
          strongest = sample
        }
      }
    }
    result.push(strongest)
  }
  return result
}

export async function generateAudioWaveform(blob: Blob): Promise<GeneratedAudioWaveform> {
  const context = createDecodeAudioContext()
  try {
    const decoded = await context.decodeAudioData(await blob.arrayBuffer())
    const channels = Array.from(
      {length: decoded.numberOfChannels},
      (_, index) => decoded.getChannelData(index),
    )
    const peaks = extractSignedWaveformPeaks(channels)
    if (!peaks.length || !Number.isFinite(decoded.duration)) {
      throw new Error('音频中没有可用的音波数据')
    }
    return {
      durationMs: Math.max(0, Math.round(decoded.duration * 1_000)),
      waveform: {
        version: AUDIO_WAVEFORM_VERSION,
        peaks: encodeSignedWaveformPeaks(peaks),
      },
    }
  } finally {
    if (context.state !== 'closed') await context.close().catch(() => undefined)
  }
}

function createDecodeAudioContext(): AudioContext {
  try {
    // 降采样足以生成语音条，并显著减少长录音解码后的 PCM 内存。
    return new AudioContext({sampleRate: 8_000})
  } catch {
    return new AudioContext()
  }
}
