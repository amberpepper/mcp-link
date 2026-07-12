import { invoke } from "@tauri-apps/api/core";
import { createPlatformAPI } from "./create-platform-api";

export function isTauriRuntime() {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

async function callPlatform<T>(
  method: string,
  args: unknown[] = [],
): Promise<T> {
  return await invoke<T>("platform_call", { method, args });
}

export const tauriPlatformAPI = createPlatformAPI(callPlatform);
