import { ref } from 'vue';
import { messages } from './messages';

export const languages = [
  { id: 'en', name: 'English' },
  { id: 'ko', name: '한국어' },
  { id: 'zh-TW', name: '繁體中文' },
  { id: 'es', name: 'Español' },
  { id: 'ja', name: '日本語' },
] as const;
export type Locale = typeof languages[number]['id'];
export const languageKey = 'k11c-installer.language';
const column: Record<Locale, number> = { ko: 0, en: 1, 'zh-TW': 2, es: 3, ja: 4 };
const dictionary = new Map<string, readonly string[]>(messages.map(row => [row[0], row]));
export function isLocale(value: unknown): value is Locale {
  return languages.some(language => language.id === value);
}
export function detectLocale(saved: unknown, preferred: readonly string[]): Locale {
  if (isLocale(saved)) return saved;
  for (const tag of preferred) {
    const base = tag.toLowerCase().replace('_', '-').split('-')[0];
    const candidate = base === 'zh' ? 'zh-TW' : base;
    if (isLocale(candidate)) return candidate;
  }
  return 'en';
}
function initialLocale(): Locale {
  let saved: string | null = null;
  try { saved = window.localStorage.getItem(languageKey); } catch { /* Storage may be unavailable. */ }
  const preferred = typeof navigator === 'undefined' ? [] : navigator.languages?.length ? navigator.languages : [navigator.language];
  return detectLocale(saved, preferred);
}
export const locale = ref<Locale>(initialLocale());
function setDocumentLanguage() {
  if (typeof document !== 'undefined') document.documentElement.lang = locale.value;
}
setDocumentLanguage();
export function setLocale(value: string): boolean {
  if (!isLocale(value)) return false;
  locale.value = value;
  try { window.localStorage.setItem(languageKey, value); } catch { /* Keep the current selection in memory. */ }
  setDocumentLanguage();
  return true;
}
export function t(key: string | null | undefined, parameters: Record<string, string | number> = {}): string {
  if (!key) return '';
  const row = dictionary.get(key);
  const message = row ? row[column[locale.value]] : key;
  return message.replace(/\{(\w+)\}/g, (token, name: string) => Object.hasOwn(parameters, name) ? String(parameters[name]) : token);
}
export function number(value: number, decimals = 0): string {
  return new Intl.NumberFormat(locale.value, { minimumFractionDigits: decimals, maximumFractionDigits: decimals }).format(value);
}
export function dateTime(seconds: number): string {
  return new Date(seconds * 1000).toLocaleString(locale.value);
}
