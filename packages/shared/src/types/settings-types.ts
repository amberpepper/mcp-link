import type { Theme } from "./ui";

export type AppLanguage = "en" | "zh" | "ja";
export interface AppSettings {
  loadExternalMCPConfigs?: boolean;
  showWindowOnStartup?: boolean;
  skillAgentPaths?: string[];
  desktopMcpListenHost?: string;
  desktopMcpListenPort?: number;
  serverPassword?: string;
  theme?: Theme;
  language?: AppLanguage;
  marketSources?: {
    mcp?: Record<string, boolean>;
    skill?: Record<string, boolean>;
  };
}
export const DEFAULT_APP_SETTINGS: AppSettings = {
  loadExternalMCPConfigs: true,
  showWindowOnStartup: true,
  desktopMcpListenHost: "127.0.0.1",
  desktopMcpListenPort: 3284,
  theme: "system",
  marketSources: {
    mcp: {
      official: true,
      smithery: true,
      "mcps-live": true,
    },
    skill: {
      community: true,
      anthropic: true,
    },
  },
};
