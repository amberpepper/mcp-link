import type {
  Skill,
  SkillWithContent,
  CreateSkillInput,
  UpdateSkillInput,
} from "../../skill-types";
import type { AgentSkillTarget, SkillInstallation } from "../../agent-types";

/**
 * Skills management API
 */
export interface SkillsAPI {
  // CRUD operations
  list: () => Promise<SkillWithContent[]>;
  create: (input: CreateSkillInput) => Promise<Skill>;
  update: (id: string, updates: UpdateSkillInput) => Promise<Skill>;
  delete: (id: string) => Promise<void>;

  // Actions
  openFolder: (id?: string) => Promise<void>;
  import: () => Promise<Skill>;
  listTargets: () => Promise<AgentSkillTarget[]>;
  setInstallation: (input: {
    skillId: string;
    agentId: string;
    targetId: string;
    projectPath?: string;
    mode?: "copy" | "symlink" | "native";
  }) => Promise<SkillInstallation>;
  removeInstallation: (id: string) => Promise<boolean>;
}
