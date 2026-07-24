export type GatewayProtocol =
  | "openai-compatible"
  | "openai-responses"
  | "anthropic";

export interface GatewayProvider {
  id: string;
  name: string;
  protocol: GatewayProtocol;
  baseUrl: string;
  apiKey: string;
  models: string[];
  enabled: boolean;
  createdAt: string;
  updatedAt: string;
}

export interface GatewayProviderInput {
  name: string;
  protocol: GatewayProtocol;
  baseUrl: string;
  apiKey?: string;
  models?: string[];
  enabled?: boolean;
}

export interface GatewayRoute {
  id: string;
  alias: string;
  protocol: GatewayProtocol;
  providerId: string;
  upstreamModel: string;
  createdAt: string;
  updatedAt: string;
}

export interface GatewayRouteInput {
  alias: string;
  providerId: string;
  upstreamModel: string;
}

export interface GatewayProviderDraft {
  name: string;
  protocol: GatewayProtocol;
  baseUrl: string;
  apiKey: string;
  models: string[];
  enabled: boolean;
}

export interface GatewayRouteDraft {
  alias: string;
  providerId: string;
  upstreamModel: string;
}

export type GatewayRemoveTarget =
  | { type: "provider"; item: GatewayProvider }
  | { type: "route"; item: GatewayRoute };

export interface GatewaySettings {
  listenHost: string;
  listenPort: number;
  accessKey: string;
  activeProviderId?: string | null;
  endpoint?: string | null;
  listenerError?: string | null;
}

export type GatewayCallStatus =
  | "running"
  | "succeeded"
  | "failed"
  | "cancelled";

export interface GatewayCallLog {
  id: string;
  requestId: string;
  startedAt: number;
  finishedAt?: number | null;
  status: GatewayCallStatus;
  httpStatus?: number | null;
  streaming: boolean;
  clientProtocol: GatewayProtocol;
  upstreamProtocol: GatewayProtocol;
  requestedModel: string;
  upstreamModel: string;
  providerId: string;
  providerName: string;
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens: number;
  cacheWriteTokens: number;
  totalTokens: number;
  firstTokenMs?: number | null;
  durationMs?: number | null;
  error?: string | null;
}

export interface GatewayCallLogQuery {
  limit?: number;
  offset?: number;
  page?: number;
  before?: number;
  status?: GatewayCallStatus | "all";
  providerId?: string;
  search?: string;
}

export interface GatewayCallLogPage {
  items: GatewayCallLog[];
  total: number;
  limit: number;
  offset: number;
  page: number;
  hasMore: boolean;
}

export interface GatewayAPI {
  getSettings(): Promise<GatewaySettings>;
  saveSettings(
    settings: Pick<GatewaySettings, "listenHost" | "listenPort">,
  ): Promise<GatewaySettings>;
  regenerateAccessKey(): Promise<string>;
  listProviders(): Promise<GatewayProvider[]>;
  createProvider(input: GatewayProviderInput): Promise<GatewayProvider>;
  updateProvider(
    id: string,
    updates: Partial<GatewayProviderInput>,
  ): Promise<GatewayProvider>;
  fetchProviderModels(
    input: Pick<GatewayProviderInput, "protocol" | "baseUrl" | "apiKey">,
  ): Promise<string[]>;
  setActiveProvider(id: string): Promise<string>;
  removeProvider(id: string): Promise<boolean>;
  listRoutes(): Promise<GatewayRoute[]>;
  createRoute(input: GatewayRouteInput): Promise<GatewayRoute>;
  updateRoute(
    id: string,
    updates: Partial<GatewayRouteInput>,
  ): Promise<GatewayRoute>;
  removeRoute(id: string): Promise<boolean>;
  listCallLogs(query?: GatewayCallLogQuery): Promise<GatewayCallLogPage>;
  clearCallLogs(): Promise<number>;
}
