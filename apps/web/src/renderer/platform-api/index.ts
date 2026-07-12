/**
 * Platform API exports for the shared web renderer
 *
 * This module provides platform API utilities
 * used by the browser app and the Tauri desktop shell
 */

// Export the store-based hook instead of the context-based one
export { usePlatformAPI } from "@/renderer/platform-api/hooks/use-platform-api";
