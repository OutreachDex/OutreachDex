import { invoke } from "@tauri-apps/api/core";

/** Copies text through the desktop host; falls back to the browser API outside the app. */
export async function copyText(text: string) {
  try {
    await invoke("clipboard_write", { text });
  } catch {
    await navigator.clipboard.writeText(text);
  }
}
