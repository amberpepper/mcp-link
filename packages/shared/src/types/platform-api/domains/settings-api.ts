/**
 * Settings management domain API
 */

import type { AppSettings, SessionTerminalOption } from "../../settings-types";

export interface NetworkInterfaceAddress {
  name: string;
  address: string;
  family: "ipv4" | "ipv6";
  isLoopback: boolean;
  label: string;
}

export interface McpEndpointStatus {
  endpoint: string;
  running: boolean;
  error?: string | null;
}

export interface SettingsAPI {
  get(): Promise<AppSettings>;
  getMcpEndpoint(): Promise<string>;
  getMcpEndpointStatus(): Promise<McpEndpointStatus>;
  save(settings: AppSettings): Promise<boolean>;
  exportMcpConfig(fileName: string, content: string): Promise<boolean>;
  listNetworkInterfaces(): Promise<NetworkInterfaceAddress[]>;
  listSessionTerminals(): Promise<SessionTerminalOption[]>;
  restartDesktopMcpEndpoint(): Promise<McpEndpointStatus>;
}
