export const MAX_PDF_CANVAS_PIXELS = 16 * 1024 * 1024

const MIN_PDF_PAGE_WIDTH = 240
const PDF_PAGE_HORIZONTAL_GUTTER = 32
const MAX_DEVICE_PIXEL_RATIO = 2

export function calculatePdfPageWidth(availableWidth: number, zoom: number): number {
  const safeWidth = Number.isFinite(availableWidth) ? availableWidth : MIN_PDF_PAGE_WIDTH
  const safeZoom = Number.isFinite(zoom) && zoom > 0 ? zoom : 1
  return Math.max(
    MIN_PDF_PAGE_WIDTH,
    Math.max(MIN_PDF_PAGE_WIDTH, safeWidth - PDF_PAGE_HORIZONTAL_GUTTER) * safeZoom,
  )
}

export function calculatePdfOutputScale(
  width: number,
  height: number,
  devicePixelRatio: number,
): number {
  const cssPixels = Math.max(1, width * height)
  const safeDevicePixelRatio = Number.isFinite(devicePixelRatio) && devicePixelRatio > 0
    ? devicePixelRatio
    : 1
  return Math.min(
    safeDevicePixelRatio,
    MAX_DEVICE_PIXEL_RATIO,
    Math.sqrt(MAX_PDF_CANVAS_PIXELS / cssPixels),
  )
}
