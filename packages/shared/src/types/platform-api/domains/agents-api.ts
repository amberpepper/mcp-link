import type {
  AgentConfigDocument,
  AgentConfigFileSummary,
  AgentManagementDescriptor,
  AgentManagementMutation,
  AgentManagementMutationResult,
  AgentManagementSection,
  AgentManagementSectionId,
  AgentInstance,
  AgentInstanceInput,
  AgentPluginDescriptor,
  AgentSession,
  AgentSessionAttachment,
  AgentSessionAttachmentData,
  AgentSessionSummary,
  SessionStats,
  UserMessageNavItem,
  SessionExportOptions,
  SessionExportResult,
  SessionExportSaveResult,
  SessionImportOptions,
  SessionListOptions,
  SessionMessagePageOptions,
  SessionOperationResult,
} from "../../agent-types";

export interface AgentsAPI {
  list: () => Promise<AgentPluginDescriptor[]>;
  instances: {
    create: (input: AgentInstanceInput) => Promise<AgentInstance>;
    remove: (id: string) => Promise<boolean>;
  };
  configs: {
    list: (instanceId: string) => Promise<AgentConfigFileSummary[]>;
    read: (instanceId: string, fileId: string) => Promise<AgentConfigDocument>;
    save: (
      instanceId: string,
      fileId: string,
      content: string,
      expectedRevision?: string,
    ) => Promise<AgentConfigDocument>;
  };
  management: {
    describe: (instanceId: string) => Promise<AgentManagementDescriptor>;
    getSection: (
      instanceId: string,
      section: AgentManagementSectionId,
    ) => Promise<AgentManagementSection>;
    apply: (
      instanceId: string,
      mutation: AgentManagementMutation,
    ) => Promise<AgentManagementMutationResult>;
  };
  sessions: {
    list: (options?: SessionListOptions) => Promise<AgentSessionSummary[]>;
    get: (
      agentId: string,
      nativeId: string,
      page?: SessionMessagePageOptions,
    ) => Promise<AgentSession>;
    listUserMessages: (
      agentId: string,
      nativeId: string,
    ) => Promise<UserMessageNavItem[]>;
    getStats: (
      agentId: string,
      nativeId: string,
    ) => Promise<SessionStats | null>;
    getAttachment: (
      agentId: string,
      nativeId: string,
      messageId: string,
      attachment: AgentSessionAttachment,
    ) => Promise<AgentSessionAttachmentData>;
    resume: (
      agentId: string,
      nativeId: string,
    ) => Promise<SessionOperationResult>;
    duplicate: (
      agentId: string,
      nativeId: string,
      untilMessage?: number,
    ) => Promise<SessionOperationResult>;
    delete: (
      agentId: string,
      nativeId: string,
    ) => Promise<SessionOperationResult>;
    rename: (
      agentId: string,
      nativeId: string,
      title: string,
    ) => Promise<SessionOperationResult>;
    export: (
      agentId: string,
      nativeId: string,
      options: SessionExportOptions,
    ) => Promise<SessionExportResult>;
    exportToFile: (
      agentId: string,
      nativeId: string,
      options: SessionExportOptions,
    ) => Promise<SessionExportSaveResult>;
    importToAgent: (
      sourceAgentId: string,
      nativeId: string,
      options: SessionImportOptions,
    ) => Promise<SessionOperationResult>;
  };
  plugins: {
    import: () => Promise<AgentPluginImportResult | null>;
    install: (bytes: number[]) => Promise<AgentPluginDescriptor>;
    remove: (id: string) => Promise<boolean>;
    setEnabled: (id: string, enabled: boolean) => Promise<boolean>;
  };
}

interface AgentPluginImportResult {
  installed: AgentPluginDescriptor[];
  failed: Array<{
    fileName: string;
    error: string;
  }>;
}
