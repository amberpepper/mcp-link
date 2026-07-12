// Platform-independent stores (no PlatformAPI dependency)
export * from "./server-editing-store";
export * from "./view-preferences-store";

// Platform-dependent store factories
export * from "./server-store";
export * from "./theme-store";

// Import platform API type
import type { PlatformAPI } from "@mcp_link/shared";

// Import store factories
import { createServerStore } from "./server-store";
import { createThemeStore, initializeThemeStore } from "./theme-store";
import { localPlatformAPI } from "@/renderer/platform-api/runtime-platform-api";

// Get the platform API for the current runtime
function getPlatformAPI(): PlatformAPI {
  return localPlatformAPI;
}

// Create store instances with dynamic platform API getter
export const useServerStore = createServerStore(getPlatformAPI);
export const useThemeStore = createThemeStore(getPlatformAPI);

// Store initialization utility
export const initializeStores = async () => {
  // Initialize theme from settings
  try {
    await initializeThemeStore(useThemeStore, getPlatformAPI);
  } catch (error) {
    console.error("Failed to initialize theme from settings:", error);
  }

  // Load initial server data
  try {
    await useServerStore.getState().refreshServers();
  } catch (error) {
    console.error("Failed to load initial servers:", error);
  }
};
