import type { AppSettings } from "@mcp_link/shared";

type MarketSourceKind = "mcp" | "skill";

export interface MarketSourceDefinition<T extends string = string> {
  id: T;
  kind: MarketSourceKind;
  label: string;
  url: string;
  description: string;
  defaultEnabled: boolean;
}

export const MCP_MARKET_SOURCES = [
  {
    id: "official",
    kind: "mcp",
    label: "Official",
    url: "https://registry.modelcontextprotocol.io/v0/servers",
    description: "官方 MCP Registry，结构化程度最高，优先用于一键添加。",
    defaultEnabled: true,
  },
  {
    id: "smithery",
    kind: "mcp",
    label: "Smithery",
    url: "https://api.smithery.ai/servers",
    description: "Smithery MCP 市场，包含远程托管和社区 MCP 服务。",
    defaultEnabled: true,
  },
  {
    id: "mcps-live",
    kind: "mcp",
    label: "MCPs Live",
    url: "https://mcps.live/api/servers",
    description:
      "mcps.live 社区目录。部分条目只有仓库和说明，不一定能一键添加。",
    defaultEnabled: true,
  },
  {
    id: "mcpm",
    kind: "mcp",
    label: "MCPM",
    url: "https://mcpm.sh/api/servers.json",
    description: "MCPM 社区注册表，包含安装命令、参数说明和分类标签。",
    defaultEnabled: true,
  },
] as const satisfies readonly MarketSourceDefinition[];

export const SKILL_MARKET_SOURCES = [
  {
    id: "community",
    kind: "skill",
    label: "Community",
    url: "https://skillsllm.com/api/skills",
    description: "社区 Skill 市场。",
    defaultEnabled: true,
  },
  {
    id: "anthropic",
    kind: "skill",
    label: "Anthropic",
    url: "https://github.com/anthropics/skills",
    description: "Anthropic 官方 skills 仓库。",
    defaultEnabled: true,
  },
] as const satisfies readonly MarketSourceDefinition[];

export type McpMarketSourceId = (typeof MCP_MARKET_SOURCES)[number]["id"];
type SkillMarketSourceId = (typeof SKILL_MARKET_SOURCES)[number]["id"];

export function getMarketSourceEnabled(
  settings: AppSettings | null | undefined,
  source: MarketSourceDefinition,
) {
  return (
    settings?.marketSources?.[source.kind]?.[source.id] ?? source.defaultEnabled
  );
}

export function getEnabledMcpMarketSources(
  settings: AppSettings | null | undefined,
) {
  return MCP_MARKET_SOURCES.filter((source) =>
    getMarketSourceEnabled(settings, source),
  );
}

export function getEnabledSkillMarketSources(
  settings: AppSettings | null | undefined,
) {
  return SKILL_MARKET_SOURCES.filter((source) =>
    getMarketSourceEnabled(settings, source),
  );
}

export function setMarketSourceEnabled(
  settings: AppSettings,
  source: MarketSourceDefinition,
  enabled: boolean,
): AppSettings {
  return {
    ...settings,
    marketSources: {
      ...settings.marketSources,
      [source.kind]: {
        ...settings.marketSources?.[source.kind],
        [source.id]: enabled,
      },
    },
  };
}
