import { invoke } from "@tauri-apps/api/core";
import { createPlatformAPI } from "./create-platform-api";

export function isTauriRuntime() {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

async function callPlatform<T>(
  method: string,
  args: unknown[] = [],
): Promise<T> {
  let lastError: unknown;
  for (let attempt = 0; attempt < 3; attempt += 1) {
    try {
      return await invoke<T>("platform_call", { method, args });
    } catch (error) {
      lastError = error;
      const transientIpcFailure =
        error instanceof TypeError && error.message.includes("Failed to fetch");
      if (!transientIpcFailure || attempt === 2) throw error;
      await new Promise((resolve) => setTimeout(resolve, 150 * (attempt + 1)));
    }
  }
  throw lastError;
}

export const tauriPlatformAPI = createPlatformAPI(callPlatform);
