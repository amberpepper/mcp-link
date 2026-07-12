import React from "react";
import type { MCPServer } from "@mcp_link/shared";
import {
  Button,
  Card,
  CardContent,
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

interface ServerCardProps {
  server: MCPServer;
  onToggle: (checked: boolean) => void | Promise<void>;
  onClick: () => void;
  onDelete: () => void;
  onError: () => void;
  onDuplicate: () => void;
  onExport: () => void;
}

export const ServerCard: React.FC<ServerCardProps> = ({
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
    <Card
      className={cn(
        "group cursor-pointer transition-colors hover:border-primary/50",
        server.status === "error" && "border-l-2 border-l-destructive",
        requiresConfig && "border-l-2 border-l-amber-500",
      )}
      onClick={onClick}
    >
      <CardContent className="p-4">
        <div className="flex items-start justify-between gap-3">
          <div className="min-w-0 flex-1">
            <div className="mb-1 flex items-center gap-2">
              <StatusDot tone={tone} />
              <h3 className="truncate text-sm font-medium">{server.name}</h3>
            </div>
            <div className="text-xs text-muted-foreground">
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
                requiresConfig
                  ? t("serverList.requiredParamsNotSet")
                  : undefined
              }
              onCheckedChange={onToggle}
            />
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button
                  variant="ghost"
                  size="icon"
                  className="h-8 w-8"
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
      </CardContent>
    </Card>
  );
};
