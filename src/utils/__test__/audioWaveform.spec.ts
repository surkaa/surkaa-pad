import {describe, expect, it} from 'vitest'
import {
  decodeSignedWaveformPeaks,
  encodeSignedWaveformPeaks,
  extractSignedWaveformPeaks,
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
})
