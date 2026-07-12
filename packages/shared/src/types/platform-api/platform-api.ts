/**
 * Platform API interface with consolidated domain structure
 */

import { AccessKeyAPI } from "./domains/access-key-api";
import { ServerAPI } from "./domains/server-api";
import { SettingsAPI } from "./domains/settings-api";
import { LogAPI } from "./domains/log-api";
import { WorkflowAPI } from "./domains/workflow-api";
import { SkillsAPI } from "./domains/skills-api";

/**
 * Main Platform API interface with domain-driven structure
 * Consolidates related functionality into logical domains
 */
export interface PlatformAPI {
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
}
