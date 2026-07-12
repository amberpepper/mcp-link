import { invoke } from "@tauri-apps/api/core";
import type { MCPServerConfig } from "@mcp_link/shared";
import { callHttpPlatform } from "@/renderer/platform-api/http-platform-api";
import { isTauriRuntime } from "@/renderer/platform-api/tauri-platform-api";
import type { McpMarketSourceId } from "./market-source-service";

export type RegistrySource = McpMarketSourceId;

interface RegistryRemote {
  type?: string;
  url?: string;
}

interface RegistryPackageArgument {
  value?: string;
}

interface RegistryPackage {
  registryType?: string;
  identifier?: string;
  version?: string;
  packageArguments?: RegistryPackageArgument[];
  environmentVariables?: Array<{
    name?: string;
    default?: string;
  }>;
  command?: string;
  args?: string[];
  env?: Record<string, string>;
}

export interface RegistryServer {
  name: string;
  title?: string;
  description?: string;
  version?: string;
  websiteUrl?: string;
  repository?: {
    url?: string;
    source?: string;
  };
  remotes?: RegistryRemote[];
  packages?: RegistryPackage[];
}

export interface RegistryServerEntry {
  server: RegistryServer;
  _meta?: Record<string, any>;
}

interface RegistryResponse {
  servers: RegistryServerEntry[];
  metadata?: {
    nextCursor?: string;
    count?: number;
    fetchedCount?: number;
    hasMore?: boolean;
    currentPage?: number;
    pages?: number;
    totalPages?: number;
  };
}

export async function fetchRegistryServers(options: {
  search?: string;
  cursor?: string;
  limit?: number;
  fetchAll?: boolean;
  maxPages?: number;
}): Promise<RegistryResponse> {
  const query = {
    search: options.search ?? "",
    cursor: options.cursor ?? "",
    limit: options.limit ?? 100,
    fetchAll: options.fetchAll ?? false,
    maxPages: options.maxPages ?? 1,
  };

  if (isTauriRuntime()) {
    return await invoke<RegistryResponse>("platform_call", {
      method: "discoverRegistryServers",
      args: [query],
    });
  }

  return await callHttpPlatform<RegistryResponse>("discoverRegistryServers", [
    query,
  ]);
}

export function registryServerToConfig(
  server: RegistryServer,
): MCPServerConfig | null {
  const remote = selectRegistryRemote(server.remotes);
  if (remote?.url) {
    return {
      id: createId(),
      name: server.title || server.name,
      description: server.description ?? "",
      serverType: remote.type === "sse" ? "remote" : "remote-streamable",
      remoteUrl: remote.url,
      args: [],
      env: {},
      autoStart: false,
      disabled: false,
    };
  }

  const pkg = server.packages?.find((item) => item.identifier);
  if (!pkg?.identifier) {
    return null;
  }

  const packageArgs =
    pkg.packageArguments
      ?.map((arg) => arg.value)
      .filter((value): value is string => Boolean(value)) ?? [];
  const env = Object.fromEntries(
    pkg.environmentVariables
      ?.filter((variable) => variable.name && variable.default)
      .map((variable) => [variable.name!, variable.default!]) ?? [],
  );

  if (pkg.registryType === "npm") {
    return {
      id: createId(),
      name: server.title || server.name,
      description: server.description ?? "",
      serverType: "local",
      command: "npx",
      args: ["-y", withVersion(pkg.identifier, pkg.version), ...packageArgs],
      env,
      autoStart: false,
      disabled: false,
    };
  }

  if (pkg.registryType === "pypi") {
    return {
      id: createId(),
      name: server.title || server.name,
      description: server.description ?? "",
      serverType: "local",
      command: "uvx",
      args: [withVersion(pkg.identifier, pkg.version), ...packageArgs],
      env,
      autoStart: false,
      disabled: false,
    };
  }

  if (pkg.registryType === "oci") {
    return {
      id: createId(),
      name: server.title || server.name,
      description: server.description ?? "",
      serverType: "local",
      command: "docker",
      args: ["run", "--rm", "-i", pkg.identifier, ...packageArgs],
      env,
      autoStart: false,
      disabled: false,
    };
  }

  if (pkg.command) {
    return {
      id: createId(),
      name: server.title || server.name,
      description: server.description ?? "",
      serverType: "local",
      command: pkg.command,
      args: pkg.args ?? [],
      env: normalizeEnv(pkg.env),
      autoStart: false,
      disabled: false,
      setupInstructions: server.websiteUrl,
      inputParams: inputParamsFromEnv(pkg.env),
    };
  }

  return null;
}

function selectRegistryRemote(remotes?: RegistryRemote[]) {
  const candidates =
    remotes?.filter((item) => item.url && item.type !== "sse") ?? [];
  if (!candidates.length) {
    return remotes?.find((item) => item.url);
  }

  return (
    candidates.find((item) => {
      try {
        const url = new URL(item.url!);
        return url.pathname === "/mcp" || url.pathname.endsWith("/mcp");
      } catch {
        return false;
      }
    }) ?? candidates[0]
  );
}

function withVersion(identifier: string, version?: string) {
  if (!version || identifier.includes("@")) {
    return identifier;
  }
  return `${identifier}@${version}`;
}

function normalizeEnv(env?: Record<string, string>) {
  if (!env) return {};
  return Object.fromEntries(
    Object.entries(env).map(([key, value]) => [
      key,
      /^\$\{[^}]+}$/.test(value) ? "" : value,
    ]),
  );
}

function inputParamsFromEnv(env?: Record<string, string>) {
  if (!env) return undefined;
  return Object.fromEntries(
    Object.entries(env).map(([key, value]) => [
      key,
      {
        type: "string" as const,
        title: key,
        required: /^\$\{[^}]+}$/.test(value),
        default: /^\$\{[^}]+}$/.test(value) ? "" : value,
      },
    ]),
  );
}

function createId() {
  return globalThis.crypto?.randomUUID?.() ?? `registry-${Date.now()}`;
}

// ─── Smithery ───

interface SmitheryServer {
  id: string;
  qualifiedName: string;
  namespace: string;
  slug?: string;
  displayName: string;
  description: string;
  iconUrl?: string;
  verified?: boolean;
  useCount?: number;
  remote?: boolean;
  isDeployed?: boolean;
  homepage?: string;
  bySmithery?: boolean;
}

interface SmitheryResponse {
  servers: SmitheryServer[];
  pagination?: {
    currentPage?: number;
    pageSize?: number;
    totalPages?: number;
    totalCount?: number;
  };
}

function smitheryServerToEntry(server: SmitheryServer): RegistryServerEntry {
  const remoteUrl = server.remote
    ? `https://server.smithery.ai/${server.qualifiedName}/mcp`
    : undefined;

  return {
    server: {
      name: server.qualifiedName,
      title: server.displayName,
      description: server.description,
      version: undefined,
      websiteUrl: server.homepage,
      repository: undefined,
      remotes: remoteUrl
        ? [{ type: "streamable-http", url: remoteUrl }]
        : undefined,
      packages: undefined,
    },
    _meta: {
      "smithery/iconUrl": server.iconUrl,
      "smithery/useCount": server.useCount,
      "smithery/verified": server.verified,
    },
  };
}

export async function fetchSmitheryServers(options: {
  search?: string;
  page?: number;
  pageSize?: number;
}): Promise<RegistryResponse> {
  const params = new URLSearchParams();
  if (options.search) params.set("q", options.search);
  params.set("pageSize", String(options.pageSize ?? 100));
  if (options.page) params.set("page", String(options.page));

  const response = await fetch(
    `https://api.smithery.ai/servers?${params.toString()}`,
    { cache: "no-store" },
  );
  if (!response.ok) {
    throw new Error(`Smithery HTTP ${response.status}`);
  }

  const data: SmitheryResponse = await response.json();
  const servers = (data.servers ?? []).map(smitheryServerToEntry);

  return {
    servers,
    metadata: {
      count: data.pagination?.totalCount ?? servers.length,
      fetchedCount: servers.length,
      currentPage: data.pagination?.currentPage ?? options.page ?? 1,
      pages: data.pagination?.totalPages,
      totalPages: data.pagination?.totalPages,
      hasMore:
        data.pagination?.currentPage != null &&
        data.pagination.totalPages != null &&
        data.pagination.currentPage < data.pagination.totalPages,
    },
  };
}

// ─── MCPs Live ───

interface McpsLiveServer {
  id: number | string;
  name: string;
  description?: string;
  link?: string;
  instructions?: string;
  keywords?: string;
  categories?: {
    name?: string;
  };
  category?: {
    name?: string;
  };
}

interface McpsLiveResponse {
  servers?: McpsLiveServer[];
}

function mcpsLiveServerToEntry(server: McpsLiveServer): RegistryServerEntry {
  const repositoryUrl = server.link?.match(/^https?:\/\/github\.com\//i)
    ? server.link
    : undefined;
  const category = server.categories?.name ?? server.category?.name;
  const keywords =
    server.keywords
      ?.split(/[,，]/)
      .map((item) => item.trim())
      .filter(Boolean) ?? [];

  return {
    server: {
      name: String(server.name || server.id),
      title: server.name,
      description: server.description,
      websiteUrl: repositoryUrl ? undefined : server.link,
      repository: repositoryUrl
        ? {
            url: repositoryUrl,
            source: "github",
          }
        : undefined,
      remotes: undefined,
      packages: undefined,
    },
    _meta: {
      "mcps-live/id": server.id,
      "mcps-live/category": category,
      "mcps-live/keywords": keywords,
      "mcps-live/instructions": server.instructions,
    },
  };
}

export async function fetchMcpsLiveServers(options: {
  search?: string;
  page?: number;
  maxPages?: number;
}): Promise<RegistryResponse> {
  const search = options.search?.trim().toLowerCase() ?? "";
  const startPage = Math.max(1, options.page ?? 1);
  const maxPages = Math.max(1, options.maxPages ?? 1);
  const endPage = startPage + maxPages - 1;
  const servers: RegistryServerEntry[] = [];
  let fetchedCount = 0;
  let currentPage = startPage - 1;
  let hasMore = false;

  for (let page = startPage; page <= endPage; page += 1) {
    const data: McpsLiveResponse = await proxyFetchJson(
      `https://mcps.live/api/servers?page=${page}`,
    );
    const items = data.servers ?? [];
    fetchedCount += items.length;
    currentPage = page;
    hasMore = items.length > 0;

    for (const item of items) {
      const entry = mcpsLiveServerToEntry(item);
      if (!search || mcpsLiveMatches(entry, search)) {
        servers.push(entry);
      }
    }

    if (items.length === 0) {
      hasMore = false;
      break;
    }
  }

  return {
    servers,
    metadata: {
      count: servers.length,
      fetchedCount,
      currentPage,
      hasMore,
      pages: currentPage,
    },
  };
}

// ─── MCPM ───

interface McpmInstallation {
  type?: string;
  command?: string;
  args?: string[];
  env?: Record<string, string>;
  description?: string;
}

interface McpmServer {
  name?: string;
  display_name?: string;
  description?: string;
  repository?: {
    type?: string;
    url?: string;
  };
  homepage?: string;
  author?: {
    name?: string;
  };
  license?: string;
  categories?: string[];
  tags?: string[];
  installations?: Record<string, McpmInstallation>;
  arguments?: Record<
    string,
    {
      description?: string;
      required?: boolean;
      example?: string;
    }
  >;
  is_official?: boolean;
}

type McpmResponse = Record<string, McpmServer>;

function mcpmServerToEntry(
  key: string,
  server: McpmServer,
): RegistryServerEntry {
  const installation = selectMcpmInstallation(server.installations);
  const repoUrl = server.repository?.url?.match(/^https?:\/\//i)
    ? server.repository.url
    : undefined;

  return {
    server: {
      name: server.name || key,
      title: server.display_name || server.name || key,
      description: server.description,
      websiteUrl:
        server.homepage && server.homepage !== repoUrl
          ? server.homepage
          : undefined,
      repository: repoUrl
        ? {
            url: repoUrl,
            source: server.repository?.type,
          }
        : undefined,
      packages: installation
        ? [
            {
              registryType: installation.type,
              command: installation.command,
              args: installation.args,
              env: mergeMcpmEnv(installation.env, server.arguments),
            },
          ]
        : undefined,
    },
    _meta: {
      "mcpm/categories": server.categories,
      "mcpm/tags": server.tags,
      "mcpm/license": server.license,
      "mcpm/official": server.is_official,
    },
  };
}

export async function fetchMcpmServers(options: {
  search?: string;
}): Promise<RegistryResponse> {
  const data = await proxyFetchJson<McpmResponse>(
    "https://mcpm.sh/api/servers.json",
  );
  const search = options.search?.trim().toLowerCase() ?? "";
  const servers = Object.entries(data)
    .map(([key, server]) => mcpmServerToEntry(key, server))
    .filter((entry) => !search || mcpmMatches(entry, search));

  return {
    servers,
    metadata: {
      count: servers.length,
      fetchedCount: Object.keys(data).length,
      hasMore: false,
      pages: 1,
    },
  };
}

function selectMcpmInstallation(
  installations?: Record<string, McpmInstallation>,
) {
  if (!installations) return undefined;
  const priority = ["npm", "uvx", "docker", "pip", "python"];
  for (const key of priority) {
    const installation = installations[key];
    if (installation?.command) return installation;
  }
  return Object.values(installations).find(
    (installation) => installation.command,
  );
}

function mergeMcpmEnv(
  env?: Record<string, string>,
  args?: McpmServer["arguments"],
) {
  const merged = { ...(env ?? {}) };
  for (const key of Object.keys(args ?? {})) {
    if (!(key in merged)) {
      merged[key] = "";
    }
  }
  return merged;
}

async function proxyFetchJson<T>(url: string): Promise<T> {
  if (isTauriRuntime()) {
    return await invoke<T>("platform_call", {
      method: "proxyFetch",
      args: [url],
    });
  }

  return await callHttpPlatform<T>("proxyFetch", [url]);
}

function mcpsLiveMatches(entry: RegistryServerEntry, search: string) {
  const server = entry.server;
  const keywords = entry._meta?.["mcps-live/keywords"];
  return [
    server.name,
    server.title,
    server.description,
    server.websiteUrl,
    server.repository?.url,
    entry._meta?.["mcps-live/category"],
    ...(Array.isArray(keywords) ? keywords : []),
  ].some((value) =>
    typeof value === "string" ? value.toLowerCase().includes(search) : false,
  );
}

function mcpmMatches(entry: RegistryServerEntry, search: string) {
  const server = entry.server;
  const tags = entry._meta?.["mcpm/tags"];
  const categories = entry._meta?.["mcpm/categories"];
  return [
    server.name,
    server.title,
    server.description,
    server.websiteUrl,
    server.repository?.url,
    ...(Array.isArray(tags) ? tags : []),
    ...(Array.isArray(categories) ? categories : []),
  ].some((value) =>
    typeof value === "string" ? value.toLowerCase().includes(search) : false,
  );
}
