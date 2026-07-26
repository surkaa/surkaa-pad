export const MIN_IMAGE_PREVIEW_SCALE = 1;
export const MAX_IMAGE_PREVIEW_SCALE = 5;

export interface Point {
  x: number;
  y: number;
}

export interface Size {
  width: number;
  height: number;
}

export interface ImagePreviewTransform extends Point {
  scale: number;
}

const clamp = (value: number, min: number, max: number) =>
    Math.min(max, Math.max(min, value));

const clampTranslation = (value: number, limit: number) =>
    limit === 0 ? 0 : clamp(value, -limit, limit);

export function clampImagePreviewScale(scale: number): number {
  return clamp(scale, MIN_IMAGE_PREVIEW_SCALE, MAX_IMAGE_PREVIEW_SCALE);
}

export function clampImagePreviewTransform(
    transform: ImagePreviewTransform,
    viewport: Size,
    content: Size,
): ImagePreviewTransform {
  const scale = clampImagePreviewScale(transform.scale);
  const maxX = Math.max(0, (content.width * scale - viewport.width) / 2);
  const maxY = Math.max(0, (content.height * scale - viewport.height) / 2);

  return {
    scale,
    x: clampTranslation(transform.x, maxX),
    y: clampTranslation(transform.y, maxY),
  };
}

export function zoomImagePreviewAtPoint(
    transform: ImagePreviewTransform,
    targetScale: number,
    focalPoint: Point,
    viewport: Size,
    content: Size,
): ImagePreviewTransform {
  const scale = clampImagePreviewScale(targetScale);
  const center = {x: viewport.width / 2, y: viewport.height / 2};
  const localX = (focalPoint.x - center.x - transform.x) / transform.scale;
  const localY = (focalPoint.y - center.y - transform.y) / transform.scale;

  return clampImagePreviewTransform({
    scale,
    x: focalPoint.x - center.x - localX * scale,
    y: focalPoint.y - center.y - localY * scale,
  }, viewport, content);
}

export function moveImagePreview(
    transform: ImagePreviewTransform,
    delta: Point,
    viewport: Size,
    content: Size,
): ImagePreviewTransform {
  return clampImagePreviewTransform({
    ...transform,
    x: transform.x + delta.x,
    y: transform.y + delta.y,
  }, viewport, content);
}

export function pinchImagePreview(
    transform: ImagePreviewTransform,
    startMidpoint: Point,
    currentMidpoint: Point,
    distanceRatio: number,
    viewport: Size,
    content: Size,
): ImagePreviewTransform {
  const scale = clampImagePreviewScale(transform.scale * distanceRatio);
  const center = {x: viewport.width / 2, y: viewport.height / 2};
  const localX = (startMidpoint.x - center.x - transform.x) / transform.scale;
  const localY = (startMidpoint.y - center.y - transform.y) / transform.scale;

  return clampImagePreviewTransform({
    scale,
    x: currentMidpoint.x - center.x - localX * scale,
    y: currentMidpoint.y - center.y - localY * scale,
  }, viewport, content);
}
