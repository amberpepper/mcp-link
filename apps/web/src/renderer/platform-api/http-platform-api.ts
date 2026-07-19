import { createPlatformAPI } from "./create-platform-api";
import type { AgentPluginDescriptor } from "@mcp_link/shared";

export const HTTP_AUTH_CHANGED_EVENT = "mcp-link:http-auth-changed";
const LEGACY_HTTP_ACCESS_TOKEN_KEY = "mcp-link:access-token";
const HTTP_API_BASE_KEY = "mcp-link:http-api-base";

function notifyHttpAuthChanged(authenticated: boolean) {
  window.dispatchEvent(
    new CustomEvent(HTTP_AUTH_CHANGED_EVENT, { detail: { authenticated } }),
  );
}

export async function verifyHttpSession() {
  window.localStorage.removeItem(LEGACY_HTTP_ACCESS_TOKEN_KEY);
  try {
    const response = await fetch(`${getHttpApiBase()}/api/auth/session`, {
      method: "GET",
      cache: "no-store",
      credentials: "include",
    });
    return response.ok;
  } catch {
    return false;
  }
}

export async function loginHttpSession(password: string) {
  const response = await fetch(`${getHttpApiBase()}/api/auth/login`, {
    method: "POST",
    cache: "no-store",
    credentials: "include",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ password }),
  });
  const result = await readHttpResponse<{ authenticated: boolean }>(response);
  notifyHttpAuthChanged(true);
  return result;
}

export async function logoutHttpSession() {
  try {
    await fetch(`${getHttpApiBase()}/api/auth/logout`, {
      method: "POST",
      cache: "no-store",
      credentials: "include",
    });
  } finally {
    notifyHttpAuthChanged(false);
  }
}

function getViteEnv(name: string) {
  const meta = import.meta as unknown as {
    env?: Record<string, string | undefined>;
  };
  return meta.env?.[name];
}

export function getHttpApiBase() {
  const configured =
    getViteEnv("VITE_MCP_LINK_API_BASE") ??
    window.localStorage.getItem(HTTP_API_BASE_KEY);
  if (configured) return configured.replace(/\/+$/, "");
  if (import.meta.env.DEV) {
    const hostname = window.location.hostname || "127.0.0.1";
    const host = hostname.includes(":") ? `[${hostname}]` : hostname;
    return `http://${host}:3284`;
  }
  return window.location.origin.replace(/\/+$/, "");
}

export async function callHttpPlatform<T>(
  method: string,
  args: unknown[] = [],
): Promise<T> {
  const request = () =>
    fetch(`${getHttpApiBase()}/api/platform/${encodeURIComponent(method)}`, {
      method: "POST",
      cache: "no-store",
      credentials: "include",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ args }),
    });

  // The Rust server may still be compiling when Vite becomes ready.
  const maxAttempts = import.meta.env.DEV ? 120 : 1;
  let response: Response | undefined;
  for (let attempt = 1; attempt <= maxAttempts; attempt += 1) {
    try {
      response = await request();
      if (response.status < 500 || attempt === maxAttempts) break;
    } catch (error) {
      if (attempt === maxAttempts) throw error;
    }
    await new Promise((resolve) => setTimeout(resolve, 1000));
  }

  if (!response) throw new Error("Platform API request failed");

  return readHttpResponse<T>(response);
}

async function installHttpAgentPlugin(
  bytes: number[],
): Promise<AgentPluginDescriptor> {
  const response = await fetch(
    `${getHttpApiBase()}/api/agent-plugins/install`,
    {
      method: "POST",
      cache: "no-store",
      credentials: "include",
      headers: {
        "Content-Type": "application/octet-stream",
      },
      body: new Uint8Array(bytes),
    },
  );
  return readHttpResponse<AgentPluginDescriptor>(response);
}

async function readHttpResponse<T>(response: Response): Promise<T> {
  const body = (await response.json().catch(() => null)) as {
    ok?: boolean;
    result?: T;
    error?: string;
  } | null;
  if (response.status === 401 || response.status === 403) {
    notifyHttpAuthChanged(false);
    if (!window.location.hash.startsWith("#/login")) {
      window.location.hash = "#/login";
    }
    throw new Error(body?.error ?? "Unauthorized");
  }

  if (!response.ok || !body?.ok) {
    throw new Error(body?.error ?? `HTTP ${response.status}`);
  }

  return body.result as T;
}

const defaultHttpPlatformAPI = createPlatformAPI(callHttpPlatform);

export const httpPlatformAPI = {
  ...defaultHttpPlatformAPI,
  agents: {
    ...defaultHttpPlatformAPI.agents,
    plugins: {
      ...defaultHttpPlatformAPI.agents.plugins,
      install: installHttpAgentPlugin,
    },
  },
};
