import type { Theme } from "./ui";

export type AppLanguage = "en" | "zh" | "ja";
export type CloseBehavior = "exit" | "minimizeToTray";
export type SessionTerminal =
  | "auto"
  | "windowsTerminal"
  | "powershell7"
  | "windowsPowerShell"
  | "cmd";

export interface SessionTerminalOption {
  id: SessionTerminal;
}

export interface AppSettings {
  showWindowOnStartup?: boolean;
  closeBehavior?: CloseBehavior;
  sessionTerminal?: SessionTerminal;
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
  showWindowOnStartup: false,
  closeBehavior: "exit",
  sessionTerminal: "auto",
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
