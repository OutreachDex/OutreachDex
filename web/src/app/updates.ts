import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { getVersion } from "@tauri-apps/api/app";

/** Checks GitHub Releases for a newer signed build and installs it in the background; the new
 *  version takes effect after a restart. Returns the version installed, or null when current. */
export async function installUpdate(): Promise<string | null> {
  const update = await check();
  if (!update) return null;
  await update.downloadAndInstall();
  return update.version;
}

export const restartApp = () => relaunch();
export const appVersion = () => getVersion();
