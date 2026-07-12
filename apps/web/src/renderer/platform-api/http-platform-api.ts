import { createPlatformAPI } from "./create-platform-api";

export const HTTP_ACCESS_TOKEN_KEY = "mcp-link:access-token";
const HTTP_API_BASE_KEY = "mcp-link:http-api-base";

function getViteEnv(name: string) {
  const meta = import.meta as unknown as {
    env?: Record<string, string | undefined>;
  };
  return meta.env?.[name];
}

export function getHttpApiBase() {
  return (
    getViteEnv("VITE_MCP_LINK_API_BASE") ??
    window.localStorage.getItem(HTTP_API_BASE_KEY) ??
    window.location.origin
  ).replace(/\/+$/, "");
}

export async function callHttpPlatform<T>(
  method: string,
  args: unknown[] = [],
): Promise<T> {
  const token = window.localStorage.getItem(HTTP_ACCESS_TOKEN_KEY);
  const response = await fetch(
    `${getHttpApiBase()}/api/platform/${encodeURIComponent(method)}`,
    {
      method: "POST",
      cache: "no-store",
      headers: {
        "Content-Type": "application/json",
        ...(token ? { Authorization: `Bearer ${token}` } : {}),
      },
      body: JSON.stringify({ args }),
    },
  );

  const body = await response.json().catch(() => null);
  if (response.status === 401 || response.status === 403) {
    window.localStorage.removeItem(HTTP_ACCESS_TOKEN_KEY);
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

export const httpPlatformAPI = createPlatformAPI(callHttpPlatform);
