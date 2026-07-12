import type {
  AccessKeyAPI,
  LogAPI,
  PlatformAPI,
  ServerAPI,
  SettingsAPI,
  SkillsAPI,
  WorkflowAPI,
} from "@mcp_link/shared";

export type PlatformCall = <T>(method: string, args?: unknown[]) => Promise<T>;

export function createPlatformAPI(callPlatform: PlatformCall): PlatformAPI {
  const accessKeys: AccessKeyAPI = {
    list: () => callPlatform("listAccessKeys"),
    generate: (options) => callPlatform("generateAccessKey", [options]),
    revoke: (id) => callPlatform("revokeAccessKey", [id]),
    updateServerAccess: (id, serverAccess) =>
      callPlatform("updateAccessKeyServerAccess", [id, serverAccess]),
  };

  const servers: ServerAPI = {
    list: () => callPlatform("listMcpServers"),
    listTools: (id) => callPlatform("listMcpServerTools", [id]),
    get: async (id) => {
      const servers = await callPlatform<any[]>("listMcpServers");
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
      const servers = await callPlatform<any[]>("listMcpServers");
      return (
        servers.find((server) => server.id === id)?.status ?? {
          type: "stopped",
        }
      );
    },
    selectFile: (options) => callPlatform("serverSelectFile", [options]),
  };

  const settings: SettingsAPI = {
    get: () => callPlatform("getSettings"),
    save: (settings) => callPlatform("saveSettings", [settings]),
    listNetworkInterfaces: () => callPlatform("listNetworkInterfaces"),
    restartDesktopMcpEndpoint: () => callPlatform("restartDesktopMcpEndpoint"),
  };

  const logs: LogAPI = {
    query: async (options) => {
      const result = await callPlatform<any>("getRequestLogs", [options]);
      return {
        ...result,
        items: result.logs ?? result.items ?? [],
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
      execute: (id, context) =>
        callPlatform("executeHookModule", [id, context]),
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
  };

  return {
    accessKeys,
    servers,
    settings,
    logs,
    workflows,
    skills,
  };
}
