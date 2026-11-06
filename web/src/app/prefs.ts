import { useSyncExternalStore } from "react";

export type ThemeChoice = "system" | "light" | "dark";
export type Density = "comfortable" | "compact";
export type Prefs = { theme: ThemeChoice; density: Density };

const KEY = "gc.ui";
const media = window.matchMedia("(prefers-color-scheme: dark)");
const listeners = new Set<() => void>();

function load(): Prefs {
  try {
    const saved = JSON.parse(localStorage.getItem(KEY) || "{}");
    return {
      theme: ["system", "light", "dark"].includes(saved.theme) ? saved.theme : "system",
      density: saved.density === "compact" ? "compact" : "comfortable",
    };
  } catch {
    return { theme: "system", density: "comfortable" };
  }
}

let prefs = load();
let scheme: "light" | "dark" = resolve(prefs.theme);

function resolve(theme: ThemeChoice) {
  return theme === "system" ? (media.matches ? "dark" : "light") : theme;
}

function apply() {
  scheme = resolve(prefs.theme);
  const root = document.documentElement;
  root.dataset.theme = scheme;
  root.dataset.density = prefs.density;
  listeners.forEach((fn) => fn());
}

export function setPrefs(next: Partial<Prefs>) {
  prefs = { ...prefs, ...next };
  try {
    localStorage.setItem(KEY, JSON.stringify(prefs));
  } catch {
    /* Preferences are optional. */
  }
  apply();
}

function subscribe(fn: () => void) {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

media.addEventListener("change", () => {
  if (prefs.theme === "system") apply();
});
apply();

export function usePrefs() {
  return useSyncExternalStore(subscribe, () => prefs);
}

/** The resolved color scheme, for kit editors and the terminal. */
export function useScheme() {
  return useSyncExternalStore(subscribe, () => scheme);
}
export const currentScheme = () => scheme;
