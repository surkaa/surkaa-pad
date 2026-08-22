import type {AudioWaveform} from '../bindings'

export const AUDIO_WAVEFORM_VERSION = 1
export const AUDIO_WAVEFORM_PEAK_COUNT = 128
export const MAX_AUTO_WAVEFORM_SOURCE_BYTES = 20 * 1024 * 1024

export interface GeneratedAudioWaveform {
  durationMs: number
  waveform: AudioWaveform
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
