import type {
  GatewayProtocol,
  GatewayProviderDraft,
  GatewayRouteDraft,
} from "@mcp_link/shared";

export const emptyProviderDraft = (): GatewayProviderDraft => ({
  name: "",
  protocol: "openai-compatible",
  baseUrl: "",
  apiKey: "",
  models: [],
  enabled: true,
});

export const emptyRouteDraft = (providerId = ""): GatewayRouteDraft => ({
  alias: "",
  providerId,
  upstreamModel: "",
});

export function protocolLabel(protocol: GatewayProtocol) {
  if (protocol === "anthropic") return "Anthropic";
  return protocol === "openai-responses"
    ? "OpenAI Responses"
    : "OpenAI Compatible";
}

export function errorMessage(error: unknown, fallback: string) {
  return error instanceof Error && error.message ? error.message : fallback;
}
