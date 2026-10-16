export type Theme = 'midnight' | 'daylight' | 'system';
export function savedTheme(): Theme {
  try { const theme = localStorage.getItem('axdb-theme'); return theme === 'midnight' || theme === 'daylight' ? theme : 'system'; } catch { return 'system'; }
}
export function applyTheme(theme: Theme) {
  document.documentElement.dataset.theme = theme === 'system' ? matchMedia('(prefers-color-scheme: dark)').matches ? 'midnight' : 'daylight' : theme;
  window.dispatchEvent(new Event('axdb-theme-change'));
}
export function setTheme(theme: Theme) {
  try { localStorage.setItem('axdb-theme', theme); } catch { /* Storage may be disabled; the current window still follows the selection. */ }
  applyTheme(theme);
}
applyTheme(savedTheme());
matchMedia('(prefers-color-scheme: dark)').addEventListener('change', () => { if (savedTheme() === 'system') applyTheme('system'); });

// Start the shell and isolated viewer with the same locally bundled font faces ready.
export const fontsReady = Promise.all([
  document.fonts.load('400 13px "Pretendard Variable"', '불러온 행 헤더를 클릭해 정렬 시간'),
  document.fonts.load('500 13px "Pretendard Variable"', '연결 실행 취소'),
  document.fonts.load('400 12.5px "JetBrains Mono Variable"', '0123456789'),
]).then(() => {
  const context = document.createElement('canvas').getContext('2d');
  if (!context) return;
  // FontFaceSet loads bytes; measuring common chrome glyphs also prepares font metrics.
  for (const font of ['400 13px "Pretendard Variable"', '500 13px "Pretendard Variable"', '400 12.5px "JetBrains Mono Variable"']) {
    context.font = font;
    context.measureText('ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789 불러온 행 헤더를 클릭해 정렬 시간');
  }
}).catch(() => undefined);
