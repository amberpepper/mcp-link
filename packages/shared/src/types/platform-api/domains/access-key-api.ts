import type { ServerAccessMap } from "../../server-access";

export interface AccessKeySummary {
  id: string;
  name: string;
  keyPrefix: string;
  createdAt: string;
  lastUsedAt?: string | null;
  serverAccess: ServerAccessMap;
}

export interface AccessKeyGenerateOptions {
  name: string;
  serverAccess?: ServerAccessMap;
}

export interface AccessKeyAPI {
  list(): Promise<AccessKeySummary[]>;
  generate(options: AccessKeyGenerateOptions): Promise<string>;
  revoke(id: string): Promise<boolean>;
  updateServerAccess(
    id: string,
    serverAccess: ServerAccessMap,
  ): Promise<AccessKeySummary>;
}
