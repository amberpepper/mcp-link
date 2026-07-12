import React, { useEffect, useMemo, useState } from "react";
import type { MCPServer, MCPTool } from "@mcp_link/shared";
import { Switch } from "@mcp_link/ui";
import { Info, RefreshCw } from "lucide-react";
import { useTranslation } from "react-i18next";

import { usePlatformAPI } from "@/renderer/platform-api";

interface ToolPermissionsProps {
  server: MCPServer;
  permissions: Record<string, boolean>;
  onChange: (permissions: Record<string, boolean>) => void;
}

const ToolPermissions: React.FC<ToolPermissionsProps> = ({
  server,
  permissions,
  onChange,
}) => {
  const { t } = useTranslation();
  const platformAPI = usePlatformAPI();
  const [tools, setTools] = useState<MCPTool[]>(server.tools ?? []);
  const [isLoading, setIsLoading] = useState(false);
  const [needsServerRunning, setNeedsServerRunning] = useState(false);
  const permissionKey = useMemo(
    () => stablePermissionsKey(server.toolPermissions),
    [server.toolPermissions],
  );

  useEffect(() => {
    let cancelled = false;
    const loadTools = async () => {
      setIsLoading(true);
      setNeedsServerRunning(false);
      try {
        const toolList = await platformAPI.servers.listTools(server.id);
        if (cancelled) return;
        setTools(toolList);
        onChange(deriveToolPermissions(toolList, server.toolPermissions));
      } catch (error) {
        if (cancelled) return;
        const message = error instanceof Error ? error.message : String(error);
        if (/must be running/i.test(message)) {
          setNeedsServerRunning(true);
        } else {
          console.error("Failed to load tools", error);
        }
      } finally {
        if (!cancelled) {
          setIsLoading(false);
        }
      }
    };

    const cachedTools = server.tools ?? [];
    setTools(cachedTools);
    onChange(deriveToolPermissions(cachedTools, server.toolPermissions));
    loadTools();

    return () => {
      cancelled = true;
    };
  }, [onChange, platformAPI, server.id, server.status]);

  useEffect(() => {
    onChange(deriveToolPermissions(tools, server.toolPermissions));
  }, [onChange, permissionKey, tools]);

  const toggleTool = (toolName: string, enabled: boolean) => {
    onChange({ ...permissions, [toolName]: enabled });
  };

  if (isLoading) {
    return (
      <div className="flex items-center gap-2 text-sm text-muted-foreground">
        <RefreshCw className="h-4 w-4 animate-spin" />
        {t("serverDetails.toolsLoading")}
      </div>
    );
  }

  if (needsServerRunning) {
    return (
      <div className="flex items-start gap-2 text-sm text-muted-foreground">
        <Info className="h-4 w-4" />
        <span>{t("serverDetails.toolsRequireRunning")}</span>
      </div>
    );
  }

  if (tools.length === 0) {
    return (
      <div className="flex items-center gap-2 text-sm text-muted-foreground">
        <Info className="h-4 w-4" />
        {t("serverDetails.toolsEmpty")}
      </div>
    );
  }

  return (
    <div className="space-y-3">
      {tools.map((tool) => {
        const enabled = permissions[tool.name] ?? true;
        return (
          <div
            key={tool.name}
            className="flex cursor-pointer items-start justify-between gap-4 rounded-md border p-3 transition-colors hover:border-primary/50"
            role="switch"
            aria-checked={enabled}
            tabIndex={0}
            onClick={() => toggleTool(tool.name, !enabled)}
            onKeyDown={(event) => {
              if (event.key === "Enter" || event.key === " ") {
                event.preventDefault();
                toggleTool(tool.name, !enabled);
              }
            }}
          >
            <div className="space-y-1">
              <p className="text-sm font-medium">{tool.name}</p>
              {tool.description && (
                <p className="text-xs text-muted-foreground">
                  {tool.description}
                </p>
              )}
            </div>
            <Switch
              onClick={(event) => event.stopPropagation()}
              checked={enabled}
              onCheckedChange={(checked) => toggleTool(tool.name, checked)}
              aria-label={
                enabled
                  ? t("serverDetails.toolEnabled")
                  : t("serverDetails.toolDisabled")
              }
            />
          </div>
        );
      })}
    </div>
  );
};

function deriveToolPermissions(
  toolList: MCPTool[] | undefined | null,
  serverPermissions: Record<string, boolean> = {},
): Record<string, boolean> {
  if (!toolList || toolList.length === 0) {
    return { ...serverPermissions };
  }

  const next: Record<string, boolean> = {};
  for (const tool of toolList) {
    if (serverPermissions[tool.name] !== undefined) {
      next[tool.name] = serverPermissions[tool.name] !== false;
    } else if (tool.enabled !== undefined) {
      next[tool.name] = !!tool.enabled;
    } else {
      next[tool.name] = true;
    }
  }
  return next;
}

function stablePermissionsKey(
  permissions: Record<string, boolean> | undefined,
): string {
  return JSON.stringify(
    Object.entries(permissions ?? {}).sort(([left], [right]) =>
      left.localeCompare(right),
    ),
  );
}

export default ToolPermissions;
