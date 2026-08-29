import {describe, expect, it} from 'vitest';
import {resolveQuasarDarkMode} from '../theme';

describe('resolveQuasarDarkMode', () => {
  it('maps explicit themes to Quasar dark states', () => {
    expect(resolveQuasarDarkMode('dark')).toBe(true);
    expect(resolveQuasarDarkMode('light')).toBe(false);
  });

  it('lets Quasar track the system theme for system or missing values', () => {
    expect(resolveQuasarDarkMode('system')).toBe('auto');
    expect(resolveQuasarDarkMode(undefined)).toBe('auto');
  });
});
