export type AppTheme = 'light' | 'dark' | 'system';

export function resolveQuasarDarkMode(theme: AppTheme | string | undefined): boolean | 'auto' {
  if (theme === 'dark') {
    return true;
  }
  if (theme === 'light') {
    return false;
  }
  return 'auto';
}
