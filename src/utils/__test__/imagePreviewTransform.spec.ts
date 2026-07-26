import {describe, expect, it} from 'vitest';
import {
  clampImagePreviewScale,
  clampImagePreviewTransform,
  moveImagePreview,
  pinchImagePreview,
  zoomImagePreviewAtPoint,
} from '../imagePreviewTransform';

const viewport = {width: 1000, height: 800};
const content = {width: 800, height: 600};

describe('image preview transforms', () => {
  it('limits zoom to the supported range', () => {
    expect(clampImagePreviewScale(0.5)).toBe(1);
    expect(clampImagePreviewScale(3)).toBe(3);
    expect(clampImagePreviewScale(8)).toBe(5);
  });

  it('keeps a centered image centered when zooming at the viewport center', () => {
    expect(zoomImagePreviewAtPoint(
        {scale: 1, x: 0, y: 0},
        2,
        {x: 500, y: 400},
        viewport,
        content,
    )).toEqual({scale: 2, x: 0, y: 0});
  });

  it('keeps the image point under an off-center zoom focal point', () => {
    expect(zoomImagePreviewAtPoint(
        {scale: 1, x: 0, y: 0},
        2,
        {x: 750, y: 400},
        viewport,
        content,
    )).toEqual({scale: 2, x: -250, y: 0});
  });

  it('prevents a dragged image from leaving the viewport', () => {
    expect(moveImagePreview(
        {scale: 2, x: 0, y: 0},
        {x: 1000, y: -1000},
        viewport,
        content,
    )).toEqual({scale: 2, x: 300, y: -200});
  });

  it('resets translation when the image fits inside the viewport', () => {
    expect(clampImagePreviewTransform(
        {scale: 1, x: 200, y: -100},
        viewport,
        content,
    )).toEqual({scale: 1, x: 0, y: 0});
  });

  it('supports simultaneous scale and midpoint movement during a pinch', () => {
    expect(pinchImagePreview(
        {scale: 1, x: 0, y: 0},
        {x: 500, y: 400},
        {x: 550, y: 430},
        2,
        viewport,
        content,
    )).toEqual({scale: 2, x: 50, y: 30});
  });
});
