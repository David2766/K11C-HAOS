import { isTauri } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import type { ThemePreference, ResolvedTheme } from './theme';

export async function syncWindowTheme(preference: ThemePreference, resolved: ResolvedTheme): Promise<void> {
  if (!isTauri()) return;
  const window = getCurrentWindow();
  await window.setTheme(preference === 'system' ? null : preference);
  await window.setBackgroundColor(resolved === 'dark' ? '#202020' : '#f3f3f3');
}
