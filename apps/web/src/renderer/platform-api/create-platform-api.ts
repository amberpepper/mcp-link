import type {
  AccessKeyAPI,
  AgentsAPI,
  GatewayAPI,
  LogAPI,
  LogQueryResult,
  MCPServer,
  PlatformAPI,
  ServerAPI,
  SettingsAPI,
  SkillsAPI,
  WorkflowAPI,
  PlatformCapabilities,
} from "@mcp_link/shared";
import type { PlatformMethod } from "./platform-methods";

export type PlatformCall = <T>(
  method: PlatformMethod,
  args?: unknown[],
) => Promise<T>;

export function createPlatformAPI(callPlatform: PlatformCall): PlatformAPI {
  const accessKeys: AccessKeyAPI = {
    list: () => callPlatform("listAccessKeys"),
    generate: (options) => callPlatform("generateAccessKey", [options]),
    revoke: (id) => callPlatform("revokeAccessKey", [id]),
    updateServerAccess: (id, serverAccess) =>
      callPlatform("updateAccessKeyServerAccess", [id, serverAccess]),
  };
  const capabilities = {
    get: () => callPlatform<PlatformCapabilities>("getPlatformCapabilities"),
  };

  const servers: ServerAPI = {
    list: () => callPlatform("listMcpServers"),
    listTools: (id) => callPlatform("listMcpServerTools", [id]),
    get: async (id) => {
      const servers = await callPlatform<MCPServer[]>("listMcpServers");
      return servers.find((server) => server.id === id) ?? null;
    },
    create: (input) => callPlatform("addMcpServer", [input]),
    update: (id, updates) =>
      callPlatform("updateMcpServerConfig", [id, updates]),
    updateToolPermissions: (id, permissions) =>
      callPlatform("updateToolPermissions", [id, permissions]),
    delete: (id) => callPlatform("removeMcpServer", [id]),
    start: (id) => callPlatform("startMcpServer", [id]),
    stop: (id) => callPlatform("stopMcpServer", [id]),
    getStatus: async (id) => {
      const servers = await callPlatform<MCPServer[]>("listMcpServers");
      const server = servers.find((candidate) => candidate.id === id);
      return server
        ? { type: server.status, error: server.errorMessage }
        : { type: "stopped" };
    },
    selectFile: (options) => callPlatform("serverSelectFile", [options]),
  };

  const settings: SettingsAPI = {
    get: () => callPlatform("getSettings"),
    getMcpEndpoint: () => callPlatform("getMcpEndpoint"),
    getMcpEndpointStatus: () => callPlatform("getMcpEndpointStatus"),
    save: (settings) => callPlatform("saveSettings", [settings]),
    exportMcpConfig: (fileName, content) =>
      callPlatform("exportMcpConfig", [fileName, content]),
    listNetworkInterfaces: () => callPlatform("listNetworkInterfaces"),
    listSessionTerminals: () => callPlatform("listSessionTerminals"),
    restartDesktopMcpEndpoint: () => callPlatform("restartDesktopMcpEndpoint"),
  };

  const logs: LogAPI = {
    query: async (options) => {
      const result = await callPlatform<Partial<LogQueryResult>>(
        "getRequestLogs",
        [options],
      );
      const items = result.logs ?? result.items ?? [];
      return {
        ...result,
        items,
        logs: result.logs ?? items,
        total: result.total ?? items.length,
        hasMore: result.hasMore ?? false,
      };
    },
  };

  const workflows: WorkflowAPI = {
    workflows: {
      list: () => callPlatform("listWorkflows"),
      get: (id) => callPlatform("getWorkflow", [id]),
      create: (workflow) => callPlatform("createWorkflow", [workflow]),
      update: (id, updates) => callPlatform("updateWorkflow", [id, updates]),
      delete: (id) => callPlatform("deleteWorkflow", [id]),
      setActive: (id) => callPlatform("setActiveWorkflow", [id]),
      disable: (id) => callPlatform("disableWorkflow", [id]),
      execute: (id, context) => callPlatform("executeWorkflow", [id, context]),
      listEnabled: () => callPlatform("getEnabledWorkflows"),
      listByType: (workflowType) =>
        callPlatform("getWorkflowsByType", [workflowType]),
    },
    hooks: {
      list: () => callPlatform("listHookModules"),
      get: (id) => callPlatform("getHookModule", [id]),
      create: (module) => callPlatform("createHookModule", [module]),
      update: (id, updates) => callPlatform("updateHookModule", [id, updates]),
      delete: (id) => callPlatform("deleteHookModule", [id]),
      import: (module) => callPlatform("importHookModule", [module]),
      validate: (script) => callPlatform("validateHookScript", [script]),
    },
  };

  const skills: SkillsAPI = {
    list: () => callPlatform("listSkills"),
    create: (input) => callPlatform("createSkill", [input]),
    update: (id, updates) => callPlatform("updateSkill", [id, updates]),
    delete: (id) => callPlatform("deleteSkill", [id]),
    openFolder: (id) => callPlatform("openSkillFolder", [id]),
    import: () => callPlatform("importSkill"),
    listTargets: () => callPlatform("listSkillTargets"),
    setInstallation: (input) => callPlatform("setSkillInstallation", [input]),
    removeInstallation: (id) => callPlatform("removeSkillInstallation", [id]),
  };

  const agents: AgentsAPI = {
    list: () => callPlatform("listAgentPlugins"),
    instances: {
      create: (input) => callPlatform("createAgentInstance", [input]),
      remove: (id) => callPlatform("removeAgentInstance", [id]),
    },
    configs: {
      list: (instanceId) => callPlatform("listAgentConfigFiles", [instanceId]),
      read: (instanceId, fileId) =>
        callPlatform("readAgentConfigFile", [instanceId, fileId]),
      save: (instanceId, fileId, content, expectedRevision) =>
        callPlatform("saveAgentConfigFile", [
          instanceId,
          fileId,
          content,
          expectedRevision ?? null,
        ]),
    },
    management: {
      describe: (instanceId) =>
        callPlatform("getAgentManagementDescriptor", [instanceId]),
      getSection: (instanceId, section) =>
        callPlatform("getAgentManagementSection", [instanceId, section]),
      apply: (instanceId, mutation) =>
        callPlatform("applyAgentManagementMutation", [instanceId, mutation]),
    },
    sessions: {
      list: (options) => callPlatform("listAgentSessions", [options ?? {}]),
      get: (agentId, nativeId, page) =>
        callPlatform("getAgentSession", [agentId, nativeId, page ?? null]),
      listUserMessages: (agentId, nativeId) =>
        callPlatform("getAgentSessionUserMessages", [agentId, nativeId]),
      getStats: (agentId, nativeId) =>
        callPlatform("getAgentSessionStats", [agentId, nativeId]),
      getAttachment: (agentId, nativeId, messageId, attachment) =>
        callPlatform("getAgentSessionAttachment", [
          agentId,
          nativeId,
          messageId,
          attachment,
        ]),
      resume: (agentId, nativeId) =>
        callPlatform("resumeAgentSession", [agentId, nativeId]),
      duplicate: (agentId, nativeId, untilMessage) =>
        callPlatform("duplicateAgentSession", [
          agentId,
          nativeId,
          untilMessage ?? null,
        ]),
      delete: (agentId, nativeId) =>
        callPlatform("deleteAgentSession", [agentId, nativeId]),
      rename: (agentId, nativeId, title) =>
        callPlatform("renameAgentSession", [agentId, nativeId, title]),
      export: (agentId, nativeId, options) =>
        callPlatform("exportAgentSession", [agentId, nativeId, options]),
      exportToFile: (agentId, nativeId, options) =>
        callPlatform("exportAgentSessionToFile", [agentId, nativeId, options]),
      importToAgent: (sourceAgentId, nativeId, options) =>
        callPlatform("importAgentSession", [sourceAgentId, nativeId, options]),
    },
    plugins: {
      import: () => callPlatform("importAgentPlugin"),
      install: (bytes) => callPlatform("installAgentPluginBytes", [bytes]),
      remove: (id) => callPlatform("removeAgentPlugin", [id]),
      setEnabled: (id, enabled) =>
        callPlatform("setAgentPluginEnabled", [id, enabled]),
    },
  };

  const gateway: GatewayAPI = {
    getSettings: () => callPlatform("getGatewaySettings"),
    saveSettings: (settings) => callPlatform("saveGatewaySettings", [settings]),
    regenerateAccessKey: () => callPlatform("regenerateGatewayAccessKey"),
    listProviders: () => callPlatform("listGatewayProviders"),
    createProvider: (input) => callPlatform("createGatewayProvider", [input]),
    updateProvider: (id, updates) =>
      callPlatform("updateGatewayProvider", [id, updates]),
    fetchProviderModels: (input) =>
      callPlatform("fetchGatewayProviderModels", [input]),
    setActiveProvider: (id) => callPlatform("setActiveGatewayProvider", [id]),
    removeProvider: (id) => callPlatform("removeGatewayProvider", [id]),
    listRoutes: () => callPlatform("listGatewayRoutes"),
    createRoute: (input) => callPlatform("createGatewayRoute", [input]),
    updateRoute: (id, updates) =>
      callPlatform("updateGatewayRoute", [id, updates]),
    removeRoute: (id) => callPlatform("removeGatewayRoute", [id]),
    listCallLogs: (query) => callPlatform("listGatewayCallLogs", [query]),
    clearCallLogs: () => callPlatform("clearGatewayCallLogs"),
  };

  return {
    capabilities,
    accessKeys,
    servers,
    settings,
    logs,
    workflows,
    skills,
    agents,
    gateway,
  };
}
