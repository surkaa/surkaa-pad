import {describe, expect, it} from 'vitest'
import {
  calculateAudioPlayerWidth,
  calculateCenteredWaveformBars,
  decodeSignedWaveformPeaks,
  encodeSignedWaveformPeaks,
  extractSignedWaveformPeaks,
  MIN_AUDIO_PLAYER_WIDTH_PX,
} from '../audioWaveform'

describe('audio waveform data', () => {
  it('round-trips signed peaks through compact bytes', () => {
    const source = [-1, -0.5, 0, 0.5, 1]
    const encoded = encodeSignedWaveformPeaks(source)

    expect(encoded).toEqual([0, 64, 128, 192, 255])
    const decoded = decodeSignedWaveformPeaks(encoded)
    decoded.forEach((value, index) => expect(value).toBeCloseTo(source[index], 2))
  })

  it('clamps invalid and out-of-range values', () => {
    expect(encodeSignedWaveformPeaks([-2, Number.NaN, 2])).toEqual([0, 128, 255])
    expect(decodeSignedWaveformPeaks([-10, 128, 999])).toEqual([-1, 0, 1])
  })

  it('keeps the strongest signed sample across all channels in each segment', () => {
    const left = new Float32Array([0.1, -0.8, 0.2, 0.3, -0.4, 0.1])
    const right = new Float32Array([0.7, 0.2, -0.9, 0.1, 0.2, 0.6])

    expect(extractSignedWaveformPeaks([left, right], 3)).toEqual([
      expect.closeTo(-0.8),
      expect.closeTo(-0.9),
      expect.closeTo(0.6),
    ])
  })

  it('handles empty channels and peak counts larger than the source', () => {
    expect(extractSignedWaveformPeaks([], 10)).toEqual([])
    expect(extractSignedWaveformPeaks([new Float32Array([0.25, -0.5])], 10)).toEqual([
      expect.closeTo(0.25),
      expect.closeTo(-0.5),
    ])
  })

  it('grows the player continuously with duration', () => {
    expect(calculateAudioPlayerWidth(0)).toBe(MIN_AUDIO_PLAYER_WIDTH_PX)
    expect(calculateAudioPlayerWidth(10_000)).toBe(280)
    expect(calculateAudioPlayerWidth(30_000)).toBe(400)
    expect(calculateAudioPlayerWidth(10 * 60_000)).toBe(3_820)
    expect(calculateAudioPlayerWidth(Number.NaN)).toBe(MIN_AUDIO_PLAYER_WIDTH_PX)
  })

  it('centers every waveform bar around the horizontal axis', () => {
    const bars = calculateCenteredWaveformBars([
      new Float32Array([0.25, -1, 0.5, -0.75]),
    ], 24, 40)

    expect(bars).toHaveLength(4)
    for (const bar of bars) {
      expect(bar.y + bar.height / 2).toBeCloseTo(20)
    }
    expect(bars[1].height).toBeGreaterThan(bars[0].height)
  })

  it('draws silent waveform bars at a centered minimum height', () => {
    const bars = calculateCenteredWaveformBars([
      new Float32Array([0, 0, 0]),
    ], 18, 20)

    expect(bars).toHaveLength(3)
    for (const bar of bars) {
      expect(bar.height).toBe(2)
      expect(bar.y).toBe(9)
    }
  })
})
