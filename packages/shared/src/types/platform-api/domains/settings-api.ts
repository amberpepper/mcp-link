/**
 * Settings management domain API
 */

import type { AppSettings } from "../../settings-types";

export interface NetworkInterfaceAddress {
  name: string;
  address: string;
  family: "ipv4" | "ipv6";
  isLoopback: boolean;
  label: string;
}

export interface SettingsAPI {
  get(): Promise<AppSettings>;
  save(settings: AppSettings): Promise<boolean>;
  listNetworkInterfaces(): Promise<NetworkInterfaceAddress[]>;
  restartDesktopMcpEndpoint(): Promise<boolean>;
}
