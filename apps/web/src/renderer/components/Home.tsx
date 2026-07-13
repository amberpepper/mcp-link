import React, { useMemo, useState } from "react";
import type { MCPServer, MCPServerConfig } from "@mcp_link/shared";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  Button,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@mcp_link/ui";
import { IconPlus, IconSearch, IconServer } from "@tabler/icons-react";
import { Download, Grid3X3, List } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Link, useNavigate } from "react-router-dom";
import { toast } from "sonner";
import { v4 as uuidv4 } from "uuid";

import EmptyState from "@/renderer/components/common/EmptyState";
import { ServerErrorModal } from "@/renderer/components/common/ServerErrorModal";
import { showServerError } from "@/renderer/components/common";
import PageLayout from "@/renderer/components/layout/PageLayout";
import ServerListView from "@/renderer/components/mcp/server/ServerListView";
import { localPlatformAPI as platformAPI } from "@/renderer/platform-api/runtime-platform-api";
import { isTauriRuntime } from "@/renderer/platform-api/tauri-platform-api";
import { useServerStore, useViewPreferencesStore } from "@/renderer/stores";
import { cn } from "@/renderer/utils/tailwind-utils";

const Home: React.FC = () => {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const {
    servers,
    searchQuery,
    setSearchQuery,
    startServer,
    stopServer,
    deleteServer,
    createServer,
  } = useServerStore();
  const { serverViewMode, setServerViewMode } = useViewPreferencesStore();

  const [errorModalOpen, setErrorModalOpen] = useState(false);
  const [errorServer, setErrorServer] = useState<MCPServer | null>(null);
  const [deleteDialogOpen, setDeleteDialogOpen] = useState(false);
  const [serverToDelete, setServerToDelete] = useState<MCPServer | null>(null);

  const filteredServers = useMemo(
    () =>
      servers
        .filter((server) =>
          server.name.toLowerCase().includes(searchQuery.toLowerCase()),
        )
        .sort((a, b) => a.name.localeCompare(b.name)),
    [servers, searchQuery],
  );

  const handleToggleServer = async (server: MCPServer, checked: boolean) => {
    try {
      if (checked) {
        await startServer(server.id);
        toast.success(t("serverList.serverStarted"));
      } else {
        await stopServer(server.id);
        toast.success(t("serverList.serverStopped"));
      }
    } catch (error) {
      showServerError(
        error instanceof Error ? error : new Error(String(error)),
        server.name,
      );
    }
  };

  const confirmDeleteServer = async () => {
    if (!serverToDelete) return;
    try {
      await deleteServer(serverToDelete.id);
      toast.success(t("serverDetails.removeSuccess"));
    } catch (error) {
      console.error("Failed to delete server:", error);
      toast.error(t("serverDetails.removeFailed"));
    } finally {
      setDeleteDialogOpen(false);
      setServerToDelete(null);
    }
  };

  const openErrorModal = (server: MCPServer) => {
    setErrorServer(server);
    setErrorModalOpen(true);
  };

  const duplicateServer = async (server: MCPServer) => {
    try {
      await createServer(toDuplicateConfig(server));
      toast.success(t("serverList.duplicateSuccess"));
    } catch (error) {
      toast.error(
        error instanceof Error
          ? error.message
          : t("serverList.duplicateFailed"),
      );
    }
  };

  const exportServersToFile = async (
    items: MCPServer[],
    filePrefix: string,
  ) => {
    const mcpServers = Object.fromEntries(
      items.map((server) => [server.name, toExportConfig(server)]),
    );
    const content = JSON.stringify({ mcpServers }, null, 2);
    const filename = `${safeFilename(filePrefix)}-${new Date().toISOString().split("T")[0]}.json`;

    try {
      if (isTauriRuntime()) {
        const saved = await platformAPI.settings.exportMcpConfig(
          filename,
          content,
        );
        if (saved) {
          toast.success(t("serverList.exportSuccess", { filename }));
        }
        return;
      }

      const didDownload = downloadJson({ mcpServers }, filename);
      if (didDownload) {
        toast.success(t("serverList.exportSuccess", { filename }));
        return;
      }

      await navigator.clipboard.writeText(content);
      toast.success(t("serverList.exportCopied"));
    } catch (error) {
      console.error("Failed to export server config:", error);
      toast.error(t("serverList.exportFailed"));
    }
  };

  const renderBody = () => {
    if (servers.length === 0) {
      return (
        <EmptyState
          icon={IconServer}
          title={t("serverList.noServers")}
          description={t("serverList.addFirstServer")}
          action={
            <Button asChild>
              <Link to="/servers/add">
                <IconPlus className="h-4 w-4" />
                {t("serverList.addServer")}
              </Link>
            </Button>
          }
        />
      );
    }

    if (filteredServers.length === 0) {
      return (
        <EmptyState
          icon={IconSearch}
          title={t("serverList.noMatches")}
          description={t("serverList.noMatchesDescription", {
            query: searchQuery,
          })}
        />
      );
    }

    return (
      <ServerListView
        servers={filteredServers}
        view={serverViewMode}
        onToggle={handleToggleServer}
        onClick={(server) => navigate(`/servers/${server.id}`)}
        onDelete={(server) => {
          setServerToDelete(server);
          setDeleteDialogOpen(true);
        }}
        onError={openErrorModal}
        onDuplicate={duplicateServer}
        onExport={(server) => {
          void exportServersToFile([server], server.name);
        }}
      />
    );
  };

  return (
    <PageLayout
      title={t("serverList.title")}
      toolbar={
        <div className="flex min-w-0 flex-1 flex-wrap items-center justify-end gap-2">
          <div className="relative min-w-[220px] flex-1 max-w-md">
            <input
              type="text"
              value={searchQuery}
              onChange={(event) => setSearchQuery(event.target.value)}
              placeholder={t("common.search")}
              className="w-full rounded-md border border-border bg-background py-1.5 pl-8 pr-3 text-sm focus:outline-none focus:ring-1 focus:ring-primary"
            />
            <IconSearch className="absolute left-2.5 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
          </div>

          <div className="flex rounded-md bg-muted p-0.5">
            <Button
              variant={serverViewMode === "list" ? "secondary" : "ghost"}
              size="icon"
              className="h-7 w-7"
              onClick={() => setServerViewMode("list")}
              title={t("serverList.listView")}
            >
              <List className="h-4 w-4" />
            </Button>
            <Button
              variant={serverViewMode === "grid" ? "secondary" : "ghost"}
              size="icon"
              className="h-7 w-7"
              onClick={() => setServerViewMode("grid")}
              title={t("serverList.gridView")}
            >
              <Grid3X3 className="h-4 w-4" />
            </Button>
          </div>

          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button variant="outline" size="sm" className="h-8">
                <Download className="h-4 w-4" />
                {t("serverList.export")}
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end">
              <DropdownMenuItem
                onClick={() => {
                  void exportServersToFile(servers, "mcp-servers");
                }}
              >
                <Download className="h-4 w-4" />
                {t("serverList.exportAll")}
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>

          <Button asChild size="sm" className="h-8">
            <Link to="/servers/add">
              <IconPlus className="h-4 w-4" />
              {t("serverList.addServer")}
            </Link>
          </Button>
        </div>
      }
      contentClassName="flex flex-col overflow-hidden"
    >
      <div
        className={cn(
          "min-h-0 flex-1",
          serverViewMode === "list" && "overflow-hidden rounded-md border",
        )}
      >
        {renderBody()}
      </div>

      {errorServer && (
        <ServerErrorModal
          isOpen={errorModalOpen}
          onClose={() => setErrorModalOpen(false)}
          serverName={errorServer.name}
          errorMessage={errorServer.errorMessage}
        />
      )}

      <AlertDialog open={deleteDialogOpen} onOpenChange={setDeleteDialogOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              {t("serverSettings.confirmDeleteTitle")}
            </AlertDialogTitle>
            <AlertDialogDescription>
              {t("serverSettings.confirmDeleteDescription", {
                serverName: serverToDelete?.name ?? "",
              })}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>
              {t("common.cancel", { defaultValue: "Cancel" })}
            </AlertDialogCancel>
            <AlertDialogAction
              onClick={confirmDeleteServer}
              className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
            >
              {t("serverSettings.delete", { defaultValue: "Delete" })}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </PageLayout>
  );
};

function toExportConfig(server: MCPServer): Record<string, unknown> {
  const config: Record<string, unknown> =
    server.serverType === "local"
      ? {
          command: server.command,
          args: server.args || [],
        }
      : {
          url: server.remoteUrl,
          headers: server.bearerToken
            ? { Authorization: `Bearer ${server.bearerToken}` }
            : undefined,
        };

  return compactObject({
    ...config,
    env:
      server.env && Object.keys(server.env).length > 0 ? server.env : undefined,
    startupTimeoutSec: server.startupTimeoutSec,
    capabilityTimeoutSec: server.capabilityTimeoutSec,
    disabled: server.disabled || undefined,
  });
}

function compactObject(
  value: Record<string, unknown>,
): Record<string, unknown> {
  return Object.fromEntries(
    Object.entries(value).filter(([, item]) => {
      if (item === undefined || item === null) return false;
      if (typeof item === "string" && item.trim() === "") return false;
      return true;
    }),
  );
}

function downloadJson(value: unknown, filename: string): boolean {
  try {
    const blob = new Blob([JSON.stringify(value, null, 2)], {
      type: "application/json",
    });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = filename;
    link.style.display = "none";
    document.body.appendChild(link);
    link.click();
    link.remove();
    window.setTimeout(() => URL.revokeObjectURL(url), 1000);
    return true;
  } catch (error) {
    console.error("Failed to trigger JSON download:", error);
    return false;
  }
}

function safeFilename(value: string): string {
  return value.replace(/[\\/:*?"<>|]+/g, "-").trim() || "mcp-servers";
}

function toDuplicateConfig(server: MCPServer): MCPServerConfig {
  return {
    id: uuidv4(),
    name: `${server.name} Copy`,
    env: { ...(server.env || {}) },
    autoStart: server.autoStart,
    disabled: server.disabled,
    description: server.description,
    serverType: server.serverType,
    command: server.command,
    args: [...(server.args || [])],
    remoteUrl: server.remoteUrl,
    bearerToken: server.bearerToken,
    inputParams: server.inputParams
      ? JSON.parse(JSON.stringify(server.inputParams))
      : undefined,
    required: server.required ? [...server.required] : undefined,
    setupInstructions: server.setupInstructions,
    verificationStatus: server.verificationStatus,
    version: server.version,
    latestVersion: server.latestVersion,
    toolPermissions: server.toolPermissions
      ? { ...server.toolPermissions }
      : undefined,
  };
}

export default Home;
