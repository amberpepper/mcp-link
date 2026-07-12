import { httpPlatformAPI } from "./http-platform-api";
import { isTauriRuntime, tauriPlatformAPI } from "./tauri-platform-api";

export const localPlatformAPI = isTauriRuntime()
  ? tauriPlatformAPI
  : httpPlatformAPI;
