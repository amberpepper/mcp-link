import { localPlatformAPI } from "@/renderer/platform-api/runtime-platform-api";
import type { PlatformAPI } from "@mcp_link/shared";

export function usePlatformAPI(): PlatformAPI {
  return localPlatformAPI;
}
