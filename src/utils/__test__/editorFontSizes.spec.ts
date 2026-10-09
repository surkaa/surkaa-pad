import {describe, expect, it} from 'vitest';
import {
  DEFAULT_EDITOR_FONT_SIZES,
  editorFontSizeCssVariables,
  normalizeEditorFontSizeConfig,
} from '../editorFontSizes';

describe('editor font sizes', () => {
  it('uses defaults for missing and invalid values', () => {
    expect(normalizeEditorFontSizeConfig(undefined)).toEqual(DEFAULT_EDITOR_FONT_SIZES);
    expect(normalizeEditorFontSizeConfig({body: Number.NaN, heading1: '48'})).toEqual(
      DEFAULT_EDITOR_FONT_SIZES,
    );
  });

  it('rounds and clamps every configured size', () => {
    expect(normalizeEditorFontSizeConfig({
      body: 9,
      heading1: 80,
      heading2: 30.6,
    })).toMatchObject({
      body: 10,
      heading1: 64,
      heading2: 31,
    });
  });

  it('creates CSS variables from normalized values', () => {
    expect(editorFontSizeCssVariables({
      ...DEFAULT_EDITOR_FONT_SIZES,
      body: 18,
      heading5: 14,
    })).toMatchObject({
      '--diary-body-font-size': '18px',
      '--diary-heading-5-font-size': '14px',
    });
  });
});
