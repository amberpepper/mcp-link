/**
 * Agent Skills type definitions
 * Skills are collections of instructions, scripts, and resources
 * that extend AI agent capabilities.
 */

/**
 * Skill entity
 */
export interface Skill {
  id: string;
  name: string; // Directory name (unique key)
  createdAt: number;
  updatedAt: number;
}

/**
 * Skill with content (for API responses)
 */
export interface SkillWithContent extends Skill {
  content: string | null; // SKILL.md content
  installations: SkillInstallation[];
}

/**
 * Input for creating a skill
 */
export interface CreateSkillInput {
  name: string;
  content?: string;
}

/**
 * Input for updating a skill
 */
export interface UpdateSkillInput {
  name?: string;
  enabled?: boolean;
  content?: string;
}
import type { SkillInstallation } from "./agent-types";
