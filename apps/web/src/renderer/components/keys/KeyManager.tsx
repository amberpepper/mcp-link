import React, { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  Badge,
  Button,
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  Input,
  Switch,
} from "@mcp_link/ui";
import type { AccessKeySummary, ServerAccessMap } from "@mcp_link/shared";
import { Check, Copy, KeyRound, RefreshCw, Trash2 } from "lucide-react";
import { toast } from "sonner";

import PageLayout from "@/renderer/components/layout/PageLayout";
import { usePlatformAPI } from "@/renderer/platform-api";
import { getHttpApiBase } from "@/renderer/platform-api/http-platform-api";
import { isTauriRuntime } from "@/renderer/platform-api/tauri-platform-api";
import { useServerStore } from "@/renderer/stores";

const KeyManager: React.FC = () => {
  const { t } = useTranslation();
  const platformAPI = usePlatformAPI();
  const servers = useServerStore((state) => state.servers);
  const refreshServers = useServerStore((state) => state.refreshServers);

  const [keys, setKeys] = useState<AccessKeySummary[]>([]);
  const [newKeyName, setNewKeyName] = useState("");
  const [newKeyAccess, setNewKeyAccess] = useState<ServerAccessMap>({});
  const [generatedKey, setGeneratedKey] = useState<string | null>(null);
  const [mcpEndpoint, setMcpEndpoint] = useState("http://127.0.0.1:3284/mcp");
  const [isLoading, setIsLoading] = useState(false);
  const [isCreating, setIsCreating] = useState(false);
  const [revokeTarget, setRevokeTarget] = useState<AccessKeySummary | null>(
    null,
  );

  const sortedServers = useMemo(
    () => [...servers].sort((a, b) => a.name.localeCompare(b.name)),
    [servers],
  );

  const loadKeys = useCallback(async () => {
    setIsLoading(true);
    try {
      const [loadedKeys, , settings] = await Promise.all([
        platformAPI.accessKeys.list(),
        refreshServers(),
        platformAPI.settings.get(),
      ]);
      setKeys(loadedKeys);
      setMcpEndpoint(resolveMcpEndpoint(settings));
    } catch (error) {
      console.error("Failed to load access keys:", error);
      toast.error(t("keys.loadError"));
    } finally {
      setIsLoading(false);
    }
  }, [platformAPI, refreshServers, t]);

  useEffect(() => {
    loadKeys();
  }, [loadKeys]);

  useEffect(() => {
    setNewKeyAccess((current) => {
      const next: ServerAccessMap = {};
      for (const server of sortedServers) {
        next[server.id] = current[server.id] ?? true;
      }
      return next;
    });
  }, [sortedServers]);

  const selectedServerCount =
    Object.values(newKeyAccess).filter(Boolean).length;

  const formatDateTime = (value?: string | null) => {
    if (!value) return t("keys.neverUsed");
    return new Date(value).toLocaleString();
  };

  const setAllNewKeyServers = (allowed: boolean) => {
    setNewKeyAccess(
      Object.fromEntries(sortedServers.map((server) => [server.id, allowed])),
    );
  };

  const handleCreateKey = async () => {
    setIsCreating(true);
    try {
      const token = await platformAPI.accessKeys.generate({
        name: newKeyName.trim() || t("keys.defaultName"),
        serverAccess: newKeyAccess,
      });
      setGeneratedKey(token);
      setNewKeyName("");
      await loadKeys();
      toast.success(t("keys.createSuccess"));
    } catch (error) {
      console.error("Failed to create access key:", error);
      toast.error(t("keys.createError"));
    } finally {
      setIsCreating(false);
    }
  };

  const handleCopyKey = async () => {
    if (!generatedKey) return;
    await copyText(generatedKey, t("keys.copySuccess"), t("keys.copyError"));
  };

  const handleCopyEndpoint = async () => {
    await copyText(
      mcpEndpoint,
      t("keys.copyEndpointSuccess"),
      t("keys.copyError"),
    );
  };

  const handleCopyConfig = async () => {
    await copyText(
      mcpClientConfig(mcpEndpoint),
      t("keys.copyConfigSuccess"),
      t("keys.copyError"),
    );
  };

  const copyText = async (
    value: string,
    successMessage: string,
    errorMessage: string,
  ) => {
    try {
      await navigator.clipboard.writeText(value);
      toast.success(successMessage);
    } catch (error) {
      console.error("Failed to copy text:", error);
      toast.error(errorMessage);
    }
  };

  const handleUpdateKeyServer = async (
    key: AccessKeySummary,
    serverId: string,
    allowed: boolean,
  ) => {
    const serverAccess = { ...key.serverAccess, [serverId]: allowed };
    setKeys((current) =>
      current.map((item) =>
        item.id === key.id ? { ...item, serverAccess } : item,
      ),
    );

    try {
      await platformAPI.accessKeys.updateServerAccess(key.id, serverAccess);
    } catch (error) {
      console.error("Failed to update access key server access:", error);
      toast.error(t("keys.updateAccessError"));
      await loadKeys();
    }
  };

  const handleRevokeKey = async () => {
    if (!revokeTarget) return;
    try {
      await platformAPI.accessKeys.revoke(revokeTarget.id);
      setRevokeTarget(null);
      await loadKeys();
      toast.success(t("keys.revokeSuccess"));
    } catch (error) {
      console.error("Failed to revoke access key:", error);
      toast.error(t("keys.revokeError"));
    }
  };

  const allowedServerCount = (serverAccess: ServerAccessMap) =>
    sortedServers.filter((server) => serverAccess[server.id] === true).length;

  return (
    <PageLayout
      title={t("keys.title")}
      contentClassName="flex flex-col gap-6"
      toolbar={
        <Button
          type="button"
          variant="outline"
          size="sm"
          onClick={loadKeys}
          disabled={isLoading}
          title={t("keys.refresh")}
        >
          <RefreshCw className="h-4 w-4" />
          {t("keys.refresh")}
        </Button>
      }
    >
      <Card>
        <CardHeader>
          <CardTitle className="text-xl">{t("keys.usageTitle")}</CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          <p className="text-sm text-muted-foreground">
            {t("keys.usageDescription")}
          </p>

          <div className="grid gap-3 lg:grid-cols-[minmax(0,1fr)_auto]">
            <div className="space-y-1">
              <label className="text-sm font-medium">
                {t("keys.mcpEndpoint")}
              </label>
              <Input
                value={mcpEndpoint}
                readOnly
                className="font-mono text-xs"
              />
            </div>
            <Button
              type="button"
              variant="outline"
              className="self-end"
              onClick={handleCopyEndpoint}
            >
              <Copy className="h-4 w-4" />
              {t("keys.copyEndpoint")}
            </Button>
          </div>

          <div className="rounded-md border">
            <div className="flex flex-wrap items-center justify-between gap-2 border-b px-3 py-2">
              <span className="text-sm font-medium">
                {t("keys.clientConfigTitle")}
              </span>
              <Button
                type="button"
                variant="ghost"
                size="sm"
                onClick={handleCopyConfig}
              >
                <Copy className="h-4 w-4" />
                {t("keys.copyConfig")}
              </Button>
            </div>
            <pre className="overflow-x-auto p-3 text-xs">
              <code>{mcpClientConfig(mcpEndpoint)}</code>
            </pre>
          </div>

          <div className="grid gap-2 text-sm text-muted-foreground md:grid-cols-2">
            <div className="rounded-md border p-3">
              <p className="font-medium text-foreground">
                {t("keys.usageStepsTitle")}
              </p>
              <ol className="mt-2 list-decimal space-y-1 pl-4">
                <li>{t("keys.usageStepCreateKey")}</li>
                <li>{t("keys.usageStepConfigureClient")}</li>
                <li>{t("keys.usageStepStartServers")}</li>
              </ol>
            </div>
            <div className="rounded-md border p-3">
              <p className="font-medium text-foreground">
                {t("keys.authHeaderTitle")}
              </p>
              <code className="mt-2 block break-all rounded bg-muted px-2 py-1 text-xs text-foreground">
                Authorization: Bearer &lt;YOUR_ACCESS_KEY&gt;
              </code>
              <p className="mt-2 text-xs">{t("keys.authHeaderHelp")}</p>
            </div>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="text-xl">{t("keys.createTitle")}</CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          {generatedKey && (
            <div className="rounded-md border bg-muted/30 p-3">
              <label className="text-sm font-medium">
                {t("keys.createdKey")}
              </label>
              <div className="mt-2 flex gap-2">
                <Input
                  value={generatedKey}
                  readOnly
                  className="font-mono text-xs"
                />
                <Button
                  type="button"
                  variant="outline"
                  size="icon"
                  onClick={handleCopyKey}
                  title={t("keys.copy")}
                >
                  <Copy className="h-4 w-4" />
                </Button>
              </div>
            </div>
          )}

          <div className="grid gap-3 lg:grid-cols-[minmax(0,1fr)_auto]">
            <Input
              value={newKeyName}
              onChange={(event) => setNewKeyName(event.target.value)}
              placeholder={t("keys.namePlaceholder")}
              onKeyDown={(event) => {
                if (event.key === "Enter" && !isCreating) {
                  handleCreateKey();
                }
              }}
            />
            <Button
              type="button"
              onClick={handleCreateKey}
              disabled={isCreating}
            >
              <KeyRound className="h-4 w-4" />
              {t("keys.create")}
            </Button>
          </div>

          <div className="rounded-md border">
            <div className="flex flex-wrap items-center justify-between gap-2 border-b px-3 py-2">
              <div className="flex items-center gap-2">
                <span className="text-sm font-medium">
                  {t("keys.serverAccess")}
                </span>
                <Badge variant="secondary">
                  {t("keys.selectedCount", {
                    selected: selectedServerCount,
                    total: sortedServers.length,
                  })}
                </Badge>
              </div>
              {sortedServers.length > 0 && (
                <div className="flex gap-2">
                  <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    onClick={() => setAllNewKeyServers(true)}
                  >
                    {t("keys.selectAll")}
                  </Button>
                  <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    onClick={() => setAllNewKeyServers(false)}
                  >
                    {t("keys.clearAll")}
                  </Button>
                </div>
              )}
            </div>
            {sortedServers.length === 0 ? (
              <div className="p-3 text-sm text-muted-foreground">
                {t("keys.noServers")}
              </div>
            ) : (
              <div className="grid gap-0 divide-y sm:grid-cols-2 sm:divide-x sm:divide-y-0 lg:grid-cols-3">
                {sortedServers.map((server) => (
                  <label
                    key={server.id}
                    className="flex min-w-0 items-center justify-between gap-3 p-3"
                  >
                    <span className="min-w-0 truncate text-sm">
                      {server.name}
                    </span>
                    <Switch
                      checked={newKeyAccess[server.id] === true}
                      onCheckedChange={(checked) =>
                        setNewKeyAccess((current) => ({
                          ...current,
                          [server.id]: checked,
                        }))
                      }
                    />
                  </label>
                ))}
              </div>
            )}
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="text-xl">{t("keys.listTitle")}</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="overflow-hidden rounded-md border">
            {isLoading ? (
              <div className="p-4 text-sm text-muted-foreground">
                {t("common.loading")}
              </div>
            ) : keys.length === 0 ? (
              <div className="p-4 text-sm text-muted-foreground">
                {t("keys.empty")}
              </div>
            ) : (
              <div className="divide-y">
                {keys.map((key) => (
                  <div key={key.id} className="p-4">
                    <div className="flex flex-col gap-3 lg:flex-row lg:items-start lg:justify-between">
                      <div className="min-w-0 space-y-1">
                        <div className="flex min-w-0 flex-wrap items-center gap-2">
                          <span className="max-w-full truncate font-medium">
                            {key.name}
                          </span>
                          <Badge variant="outline" className="font-mono">
                            {key.keyPrefix}...
                          </Badge>
                          <Badge variant="secondary">
                            {t("keys.selectedCount", {
                              selected: allowedServerCount(key.serverAccess),
                              total: sortedServers.length,
                            })}
                          </Badge>
                        </div>
                        <div className="text-xs text-muted-foreground">
                          {t("keys.createdAt", {
                            value: formatDateTime(key.createdAt),
                          })}
                          {" / "}
                          {t("keys.lastUsedAt", {
                            value: formatDateTime(key.lastUsedAt),
                          })}
                        </div>
                      </div>
                      <Button
                        type="button"
                        variant="ghost"
                        size="icon"
                        className="self-end text-destructive hover:text-destructive lg:self-auto"
                        onClick={() => setRevokeTarget(key)}
                        title={t("keys.revoke")}
                      >
                        <Trash2 className="h-4 w-4" />
                      </Button>
                    </div>

                    <div className="mt-3 rounded-md border">
                      {sortedServers.length === 0 ? (
                        <div className="p-3 text-sm text-muted-foreground">
                          {t("keys.noServers")}
                        </div>
                      ) : (
                        <div className="grid gap-0 divide-y sm:grid-cols-2 sm:divide-x sm:divide-y-0 lg:grid-cols-3">
                          {sortedServers.map((server) => (
                            <label
                              key={server.id}
                              className="flex min-w-0 items-center justify-between gap-3 p-3"
                            >
                              <span className="min-w-0 truncate text-sm">
                                {server.name}
                              </span>
                              <Switch
                                checked={key.serverAccess[server.id] === true}
                                onCheckedChange={(checked) =>
                                  handleUpdateKeyServer(key, server.id, checked)
                                }
                              />
                            </label>
                          ))}
                        </div>
                      )}
                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>
        </CardContent>
      </Card>

      <AlertDialog
        open={Boolean(revokeTarget)}
        onOpenChange={(open) => !open && setRevokeTarget(null)}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("keys.confirmRevokeTitle")}</AlertDialogTitle>
            <AlertDialogDescription>
              {t("keys.confirmRevokeDescription", {
                name: revokeTarget?.name ?? "",
              })}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("common.cancel")}</AlertDialogCancel>
            <AlertDialogAction
              className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
              onClick={handleRevokeKey}
            >
              <Trash2 className="h-4 w-4" />
              {t("keys.revoke")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </PageLayout>
  );
};

function resolveMcpEndpoint(settings: {
  desktopMcpListenHost?: string;
  desktopMcpListenPort?: number;
}) {
  if (!isTauriRuntime()) {
    return `${getHttpApiBase()}/mcp`;
  }

  const host = settings.desktopMcpListenHost || "127.0.0.1";
  const port = settings.desktopMcpListenPort || 3284;
  const displayHost =
    host.includes(":") && !host.startsWith("[") ? `[${host}]` : host;
  return `http://${displayHost}:${port}/mcp`;
}

function mcpClientConfig(endpoint: string) {
  return JSON.stringify(
    {
      mcpServers: {
        "mcp-link": {
          url: endpoint,
          headers: {
            Authorization: "Bearer <YOUR_ACCESS_KEY>",
          },
        },
      },
    },
    null,
    2,
  );
}

export default KeyManager;
