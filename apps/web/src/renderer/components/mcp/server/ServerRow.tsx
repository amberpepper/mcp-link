import React from "react";
import type { MCPServer } from "@mcp_link/shared";
import {
  Button,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
  StatusDot,
  Switch,
} from "@mcp_link/ui";
import {
  AlertCircle,
  Copy,
  Download,
  MoreVertical,
  Trash2,
} from "lucide-react";
import { useTranslation } from "react-i18next";

import { cn } from "@/renderer/utils/tailwind-utils";
import { hasUnsetRequiredParams } from "@/renderer/utils/server-validation-utils";

interface ServerRowProps {
  server: MCPServer;
  onToggle: (checked: boolean) => void | Promise<void>;
  onClick: () => void;
  onDelete: () => void;
  onError: () => void;
  onDuplicate: () => void;
  onExport: () => void;
}

export const ServerRow: React.FC<ServerRowProps> = ({
  server,
  onToggle,
  onClick,
  onDelete,
  onError,
  onDuplicate,
  onExport,
}) => {
  const { t } = useTranslation();
  const requiresConfig = hasUnsetRequiredParams(server);
  const toolCount = server.tools?.length ?? 0;
  const tone = requiresConfig ? "config" : server.status;

  return (
    <div
      className={cn(
        "group flex cursor-pointer items-center gap-4 p-4 transition-colors hover:bg-muted/50",
        server.status === "error" &&
          "border-l-2 border-l-destructive bg-destructive/5",
        requiresConfig && "border-l-2 border-l-amber-500 bg-amber-50/50",
      )}
      onClick={onClick}
    >
      <StatusDot tone={tone} className="shrink-0" />
      <div className="min-w-0 flex-1">
        <div className="truncate text-sm font-medium">{server.name}</div>
        <div className="mt-0.5 text-xs text-muted-foreground">
          {t("serverList.toolCount", { count: toolCount })}
          {requiresConfig && (
            <>
              {" · "}
              <span className="text-amber-700">
                {t("serverList.configRequired")}
              </span>
            </>
          )}
        </div>
      </div>
      <span className="w-24 shrink-0 text-right text-sm text-muted-foreground">
        {t(`serverList.status.${server.status}`)}
      </span>
      <div
        className="flex shrink-0 items-center gap-2"
        onClick={(event) => event.stopPropagation()}
      >
        <Switch
          checked={server.status === "running"}
          disabled={
            server.status === "starting" ||
            server.status === "stopping" ||
            requiresConfig
          }
          title={
            requiresConfig ? t("serverList.requiredParamsNotSet") : undefined
          }
          onCheckedChange={onToggle}
        />
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button
              variant="ghost"
              size="icon"
              className="h-8 w-8 opacity-0 transition-opacity group-hover:opacity-100"
              title={t("common.settings")}
            >
              <MoreVertical className="h-4 w-4" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            {server.status === "error" && (
              <>
                <DropdownMenuItem onClick={onError}>
                  <AlertCircle className="h-4 w-4" />
                  {t("serverList.errorDetails")}
                </DropdownMenuItem>
                <DropdownMenuSeparator />
              </>
            )}
            <DropdownMenuItem onClick={onDuplicate}>
              <Copy className="h-4 w-4" />
              {t("serverList.duplicate")}
            </DropdownMenuItem>
            <DropdownMenuItem onClick={onExport}>
              <Download className="h-4 w-4" />
              {t("serverList.exportConfig")}
            </DropdownMenuItem>
            <DropdownMenuSeparator />
            <DropdownMenuItem
              onClick={onDelete}
              className="text-destructive focus:text-destructive"
            >
              <Trash2 className="h-4 w-4" />
              {t("serverSettings.delete", { defaultValue: "Delete" })}
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
    </div>
  );
};
