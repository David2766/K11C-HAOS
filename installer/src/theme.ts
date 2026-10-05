import { ref } from 'vue';

export type ThemePreference = 'system' | 'light' | 'dark';
export type ResolvedTheme = 'light' | 'dark';
export const themeKey = 'k11c-installer.theme';
export const themes = ['system', 'light', 'dark'] as const;
export function isTheme(value: unknown): value is ThemePreference {
  return themes.some(theme => theme === value);
}
function savedTheme(): ThemePreference {
  try {
    const saved = window.localStorage.getItem(themeKey);
    if (isTheme(saved)) return saved;
  } catch { /* Read-only portable profiles still support this session. */ }
  return 'system';
}
const media = typeof window !== 'undefined' && typeof window.matchMedia === 'function'
  ? window.matchMedia('(prefers-color-scheme: dark)') : null;
export const themePreference = ref<ThemePreference>(savedTheme());
export const resolvedTheme = ref<ResolvedTheme>('light');
type WindowSync = (preference: ThemePreference, resolved: ResolvedTheme) => Promise<void>;
let syncWindow: WindowSync | undefined;
let pending: Promise<void> = Promise.resolve();
function applyTheme() {
  resolvedTheme.value = themePreference.value === 'system'
    ? (media?.matches ? 'dark' : 'light') : themePreference.value;
  if (typeof document !== 'undefined') {
    document.documentElement.dataset.theme = resolvedTheme.value;
    document.documentElement.style.colorScheme = resolvedTheme.value;
  }
  if (syncWindow) {
    const preference = themePreference.value, resolved = resolvedTheme.value, sync = syncWindow;
    // Serialize title-bar changes so rapid selection cannot leave an older theme last.
    pending = pending.then(() => sync(preference, resolved)).catch(error => console.warn('Window theme:', error));
  }
}
export function setTheme(value: string): boolean {
  if (!isTheme(value)) return false;
  themePreference.value = value;
  try { window.localStorage.setItem(themeKey, value); } catch { /* Session preference is retained. */ }
  applyTheme();
  return true;
}
function systemChanged() { if (themePreference.value === 'system') applyTheme(); }
export function startThemeSync(sync: WindowSync): () => void {
  syncWindow = sync;
  media?.addEventListener('change', systemChanged);
  applyTheme();
  return () => { media?.removeEventListener('change', systemChanged); syncWindow = undefined; };
}
applyTheme();
