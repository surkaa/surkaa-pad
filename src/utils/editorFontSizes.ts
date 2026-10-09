export const EDITOR_FONT_SIZE_KEYS = [
  'body',
  'heading1',
  'heading2',
  'heading3',
  'heading4',
  'heading5',
] as const;

export type EditorFontSizeKey = typeof EDITOR_FONT_SIZE_KEYS[number];
export type EditorFontSizeConfig = Record<EditorFontSizeKey, number>;

export const MIN_EDITOR_FONT_SIZE = 10;
export const MAX_EDITOR_FONT_SIZE = 64;

/** 保持与修改前浏览器默认字号接近。 */
export const DEFAULT_EDITOR_FONT_SIZES: EditorFontSizeConfig = {
  body: 16,
  heading1: 32,
  heading2: 24,
  heading3: 19,
  heading4: 16,
  heading5: 13,
};

export const EDITOR_FONT_SIZE_LABELS: Record<EditorFontSizeKey, string> = {
  body: '正文',
  heading1: '一级标题',
  heading2: '二级标题',
  heading3: '三级标题',
  heading4: '四级标题',
  heading5: '五级标题',
};

function normalizeSize(value: unknown, fallback: number): number {
  if (typeof value !== 'number' || !Number.isFinite(value)) return fallback;
  return Math.min(MAX_EDITOR_FONT_SIZE, Math.max(MIN_EDITOR_FONT_SIZE, Math.round(value)));
}

export function normalizeEditorFontSizeConfig(value: unknown): EditorFontSizeConfig {
  const source = value && typeof value === 'object'
    ? value as Partial<Record<EditorFontSizeKey, unknown>>
    : {};
  return Object.fromEntries(EDITOR_FONT_SIZE_KEYS.map(key => [
    key,
    normalizeSize(source[key], DEFAULT_EDITOR_FONT_SIZES[key]),
  ])) as EditorFontSizeConfig;
}

export function editorFontSizeCssVariables(
  value: EditorFontSizeConfig,
): Record<string, string> {
  const sizes = normalizeEditorFontSizeConfig(value);
  return {
    '--diary-body-font-size': `${sizes.body}px`,
    '--diary-heading-1-font-size': `${sizes.heading1}px`,
    '--diary-heading-2-font-size': `${sizes.heading2}px`,
    '--diary-heading-3-font-size': `${sizes.heading3}px`,
    '--diary-heading-4-font-size': `${sizes.heading4}px`,
    '--diary-heading-5-font-size': `${sizes.heading5}px`,
  };
}
