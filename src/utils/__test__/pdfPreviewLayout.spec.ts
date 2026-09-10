import {describe, expect, it} from 'vitest'
import {
  calculatePdfOutputScale,
  calculatePdfPageWidth,
  MAX_PDF_CANVAS_PIXELS,
} from '../pdfPreviewLayout'

describe('calculatePdfPageWidth', () => {
  it('适应宽度时使用完整可用宽度而不是限制在 1200px', () => {
    expect(calculatePdfPageWidth(1920, 1)).toBe(1888)
  })

  it('保留缩放倍率和最小可操作宽度', () => {
    expect(calculatePdfPageWidth(1000, 0.5)).toBe(484)
    expect(calculatePdfPageWidth(300, 0.5)).toBe(240)
  })
})

describe('calculatePdfOutputScale', () => {
  it('普通页面最多使用两倍设备像素比', () => {
    expect(calculatePdfOutputScale(800, 1200, 3)).toBe(2)
  })

  it('超大页面仍限制画布像素总量', () => {
    const width = 3800
    const height = 5373
    const scale = calculatePdfOutputScale(width, height, 2)

    expect(scale).toBeLessThan(1)
    expect(width * height * scale * scale).toBeCloseTo(MAX_PDF_CANVAS_PIXELS, 5)
  })
})
