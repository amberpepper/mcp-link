/**
 * Platform API interface with consolidated domain structure
 */

import { AccessKeyAPI } from "./domains/access-key-api";
import { ServerAPI } from "./domains/server-api";
import { SettingsAPI } from "./domains/settings-api";
import { LogAPI } from "./domains/log-api";
import { WorkflowAPI } from "./domains/workflow-api";
import { SkillsAPI } from "./domains/skills-api";
import { AgentsAPI } from "./domains/agents-api";
import { GatewayAPI } from "./domains/gateway-api";

/**
 * Main Platform API interface with domain-driven structure
 * Consolidates related functionality into logical domains
 */
export interface PlatformAPI {
  capabilities: {
    get(): Promise<PlatformCapabilities>;
  };
  // Access key management domain
  accessKeys: AccessKeyAPI;

  // Server management domain
  servers: ServerAPI;

  // Settings management domain
  settings: SettingsAPI;

  // Log management domain
  logs: LogAPI;

  // Workflow and Hook Module management domain
  workflows: WorkflowAPI;

  // Skills management domain
  skills: SkillsAPI;

  // AI CLI adapters and local session management domain
  agents: AgentsAPI;

  // OpenAI and Anthropic compatible model API gateway
  gateway: GatewayAPI;
}

export interface PlatformCapabilities {
  platform: "desktop" | "server";
  capabilities: {
    desktopDialogs: boolean;
    autostart: boolean;
    mcpHttpEndpoint: boolean;
    agentPlugins: boolean;
    gateway: boolean;
    workflows: boolean;
  };
}
