// UI language. English text in the source is the key; locales/<lang>.json maps it to Chinese,
// Japanese and Korean, and anything untranslated falls back to the English source text. The
// language is read once at startup (module-level labels depend on it); switching saves editors
// and reloads the window.
import zh from "../locales/zh.json";
import ja from "../locales/ja.json";
import ko from "../locales/ko.json";

export type Lang = "en" | "zh" | "ja" | "ko";
export const languages: [Lang, string][] = [
  ["en", "English"],
  ["zh", "中文"],
  ["ja", "日本語"],
  ["ko", "한국어"],
];
const storageKey = "gc.lang";
const dictionaries: Record<Lang, Record<string, string>> = { en: {}, zh, ja, ko };

function detect(): Lang {
  try {
    const saved = localStorage.getItem(storageKey) as Lang | null;
    if (saved && saved in dictionaries) return saved;
  } catch {
    /* storage unavailable */
  }
  const preferred = (typeof navigator !== "undefined" && navigator.language) || "en";
  const code = preferred.toLowerCase().slice(0, 2);
  return code === "zh" || code === "ja" || code === "ko" ? code : "en";
}

export const lang: Lang = detect();
/** BCP 47 locale for dates and numbers. */
export const locale = { en: "en-US", zh: "zh-CN", ja: "ja-JP", ko: "ko-KR" }[lang];
const dictionary = dictionaries[lang];
if (typeof document !== "undefined") document.documentElement.lang = locale;

/** Translates English source text; `{name}` placeholders are filled from `vars`. */
export function t(text: string, vars?: Record<string, string | number>): string {
  const out = dictionary[text] || text;
  return vars ? out.replace(/\{(\w+)\}/g, (m, k: string) => (k in vars ? String(vars[k]) : m)) : out;
}

export function setLang(next: Lang) {
  try {
    localStorage.setItem(storageKey, next);
  } catch {
    /* storage unavailable */
  }
  location.reload();
}
