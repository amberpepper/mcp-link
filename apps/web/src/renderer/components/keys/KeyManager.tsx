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
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
  Input,
  Label,
  Switch,
} from "@mcp_link/ui";
import type { AccessKeySummary, ServerAccessMap } from "@mcp_link/shared";
import {
  Check,
  Clock3,
  Copy,
  KeyRound,
  MoreHorizontal,
  Plus,
  RefreshCw,
  Settings2,
  ShieldCheck,
  Trash2,
} from "lucide-react";
import { toast } from "sonner";

import PageLayout from "@/renderer/components/layout/PageLayout";
import { usePlatformAPI } from "@/renderer/platform-api";
import { useServerStore } from "@/renderer/stores";
import { usableMcpEndpoint } from "@/renderer/utils/mcp-endpoint";
import { cn } from "@/renderer/utils/tailwind-utils";

const KeyManager: React.FC = () => {
  const { t } = useTranslation();
  const platformAPI = usePlatformAPI();
  const servers = useServerStore((state) => state.servers);
  const refreshServers = useServerStore((state) => state.refreshServers);

  const [keys, setKeys] = useState<AccessKeySummary[]>([]);
  const [mcpEndpoint, setMcpEndpoint] = useState("");
  const [isLoading, setIsLoading] = useState(false);

  const [createOpen, setCreateOpen] = useState(false);
  const [newKeyName, setNewKeyName] = useState("");
  const [newKeyAccess, setNewKeyAccess] = useState<ServerAccessMap>({});
  const [isCreating, setIsCreating] = useState(false);
  const [generatedKey, setGeneratedKey] = useState<{
    name: string;
    token: string;
  } | null>(null);

  const [setupOpen, setSetupOpen] = useState(false);
  const [accessTarget, setAccessTarget] = useState<AccessKeySummary | null>(
    null,
  );
  const [accessDraft, setAccessDraft] = useState<ServerAccessMap>({});
  const [isSavingAccess, setIsSavingAccess] = useState(false);
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
      const [loadedKeys, , endpoint] = await Promise.all([
        platformAPI.accessKeys.list(),
        refreshServers(),
        platformAPI.settings.getMcpEndpoint(),
      ]);
      setKeys(loadedKeys);
      setMcpEndpoint(usableMcpEndpoint(endpoint));
    } catch (error) {
      console.error("Failed to load access keys:", error);
      toast.error(t("keys.loadError"));
    } finally {
      setIsLoading(false);
    }
  }, [platformAPI, refreshServers, t]);

  useEffect(() => {
    void loadKeys();
  }, [loadKeys]);

  useEffect(() => {
    setNewKeyAccess((current) => normalizeAccess(sortedServers, current, true));
  }, [sortedServers]);

  const copyText = async (value: string, successMessage: string) => {
    try {
      await navigator.clipboard.writeText(value);
      toast.success(successMessage);
    } catch (error) {
      console.error("Failed to copy text:", error);
      toast.error(t("keys.copyError"));
    }
  };

  const openCreate = () => {
    setNewKeyName("");
    setNewKeyAccess(
      Object.fromEntries(sortedServers.map((server) => [server.id, true])),
    );
    setCreateOpen(true);
  };

  const handleCreateKey = async () => {
    const name = newKeyName.trim() || t("keys.defaultName");
    setIsCreating(true);
    try {
      const token = await platformAPI.accessKeys.generate({
        name,
        serverAccess: newKeyAccess,
      });
      setCreateOpen(false);
      setGeneratedKey({ name, token });
      await loadKeys();
      toast.success(t("keys.createSuccess"));
    } catch (error) {
      console.error("Failed to create access key:", error);
      toast.error(t("keys.createError"));
    } finally {
      setIsCreating(false);
    }
  };

  const openAccessEditor = (key: AccessKeySummary) => {
    setAccessTarget(key);
    setAccessDraft(normalizeAccess(sortedServers, key.serverAccess, false));
  };

  const handleSaveAccess = async () => {
    if (!accessTarget) return;
    setIsSavingAccess(true);
    try {
      const updated = await platformAPI.accessKeys.updateServerAccess(
        accessTarget.id,
        accessDraft,
      );
      setKeys((current) =>
        current.map((key) => (key.id === updated.id ? updated : key)),
      );
      setAccessTarget(null);
      toast.success(t("keys.accessSaved"));
    } catch (error) {
      console.error("Failed to update access key server access:", error);
      toast.error(t("keys.updateAccessError"));
    } finally {
      setIsSavingAccess(false);
    }
  };

  const handleRevokeKey = async () => {
    if (!revokeTarget) return;
    try {
      await platformAPI.accessKeys.revoke(revokeTarget.id);
      setKeys((current) => current.filter((key) => key.id !== revokeTarget.id));
      setRevokeTarget(null);
      toast.success(t("keys.revokeSuccess"));
    } catch (error) {
      console.error("Failed to revoke access key:", error);
      toast.error(t("keys.revokeError"));
    }
  };

  const allowedServers = (serverAccess: ServerAccessMap) =>
    sortedServers.filter((server) => serverAccess[server.id] === true);

  const formatDateTime = (value?: string | null) =>
    value ? new Date(value).toLocaleString() : t("keys.neverUsed");

  return (
    <PageLayout
      title={t("keys.title")}
      contentClassName="mx-auto flex w-full max-w-6xl flex-col gap-5"
      toolbar={
        <>
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={() => void loadKeys()}
            disabled={isLoading}
          >
            <RefreshCw
              className={isLoading ? "h-4 w-4 animate-spin" : "h-4 w-4"}
            />
            {t("common.refresh")}
          </Button>
          <Button type="button" size="sm" onClick={openCreate}>
            <Plus className="h-4 w-4" />
            {t("keys.create")}
          </Button>
        </>
      }
    >
      <Card>
        <CardContent className="flex flex-col gap-4 p-4 lg:flex-row lg:items-center">
          <div className="flex min-w-0 flex-1 items-start gap-3">
            <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-primary/10 text-primary">
              <ShieldCheck className="h-5 w-5" />
            </div>
            <div className="min-w-0 flex-1">
              <p className="font-medium">{t("keys.connectionTitle")}</p>
              <p className="mt-0.5 text-sm text-muted-foreground">
                {t("keys.connectionDescription")}
              </p>
              <code className="mt-2 block truncate rounded bg-muted px-2 py-1.5 text-xs">
                {mcpEndpoint || t("common.loading")}
              </code>
            </div>
          </div>
          <div className="flex shrink-0 flex-wrap gap-2 lg:justify-end">
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={() =>
                void copyText(mcpEndpoint, t("keys.copyEndpointSuccess"))
              }
              disabled={!mcpEndpoint}
            >
              <Copy className="h-4 w-4" />
              {t("keys.copyEndpoint")}
            </Button>
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={() => setSetupOpen(true)}
              disabled={!mcpEndpoint}
            >
              <Settings2 className="h-4 w-4" />
              {t("keys.setupGuide")}
            </Button>
          </div>
        </CardContent>
      </Card>

      <div className="flex items-center justify-between gap-3">
        <div>
          <h2 className="text-lg font-semibold">{t("keys.listTitle")}</h2>
          <p className="text-sm text-muted-foreground">
            {t("keys.listDescription", { count: keys.length })}
          </p>
        </div>
      </div>

      {isLoading && keys.length === 0 ? (
        <Card>
          <CardContent className="p-8 text-center text-sm text-muted-foreground">
            {t("common.loading")}
          </CardContent>
        </Card>
      ) : keys.length === 0 ? (
        <Card>
          <CardContent className="flex min-h-64 flex-col items-center justify-center p-8 text-center">
            <div className="flex h-12 w-12 items-center justify-center rounded-full bg-muted">
              <KeyRound className="h-6 w-6 text-muted-foreground" />
            </div>
            <h3 className="mt-4 font-medium">{t("keys.empty")}</h3>
            <p className="mt-1 max-w-md text-sm text-muted-foreground">
              {t("keys.emptyDescription")}
            </p>
            <Button className="mt-4" onClick={openCreate}>
              <Plus className="h-4 w-4" />
              {t("keys.create")}
            </Button>
          </CardContent>
        </Card>
      ) : (
        <div className="grid gap-4 xl:grid-cols-2">
          {keys.map((key) => {
            const permitted = allowedServers(key.serverAccess);
            return (
              <Card key={key.id} className="overflow-hidden">
                <CardContent className="p-0">
                  <div className="flex items-start gap-3 p-4">
                    <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg border bg-muted/40">
                      <KeyRound className="h-5 w-5 text-muted-foreground" />
                    </div>
                    <div className="min-w-0 flex-1">
                      <div className="flex items-center gap-2">
                        <h3 className="truncate font-medium">{key.name}</h3>
                        <Badge
                          variant="outline"
                          className="font-mono text-[11px]"
                        >
                          {key.keyPrefix}…
                        </Badge>
                      </div>
                      <div className="mt-1 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground">
                        <span className="inline-flex items-center gap-1">
                          <Check className="h-3.5 w-3.5" />
                          {t("keys.selectedCount", {
                            selected: permitted.length,
                            total: sortedServers.length,
                          })}
                        </span>
                        <span className="inline-flex items-center gap-1">
                          <Clock3 className="h-3.5 w-3.5" />
                          {t("keys.lastUsedAt", {
                            value: formatDateTime(key.lastUsedAt),
                          })}
                        </span>
                      </div>
                    </div>
                    <DropdownMenu>
                      <DropdownMenuTrigger asChild>
                        <Button
                          type="button"
                          variant="ghost"
                          size="icon"
                          className="h-8 w-8 shrink-0"
                          aria-label={t("keys.moreActions")}
                        >
                          <MoreHorizontal className="h-4 w-4" />
                        </Button>
                      </DropdownMenuTrigger>
                      <DropdownMenuContent align="end">
                        <DropdownMenuItem onClick={() => openAccessEditor(key)}>
                          <Settings2 className="h-4 w-4" />
                          {t("keys.editAccess")}
                        </DropdownMenuItem>
                        <DropdownMenuSeparator />
                        <DropdownMenuItem
                          className="text-destructive focus:text-destructive"
                          onClick={() => setRevokeTarget(key)}
                        >
                          <Trash2 className="h-4 w-4" />
                          {t("keys.revoke")}
                        </DropdownMenuItem>
                      </DropdownMenuContent>
                    </DropdownMenu>
                  </div>

                  <div className="border-t bg-muted/15 px-4 py-3">
                    <div className="flex items-center justify-between gap-3">
                      <div className="min-w-0">
                        <p className="text-xs font-medium text-muted-foreground">
                          {t("keys.allowedServers")}
                        </p>
                        <div className="mt-1.5 flex min-h-6 flex-wrap gap-1.5">
                          {permitted.length === 0 ? (
                            <span className="text-sm text-muted-foreground">
                              {t("keys.noServerAccess")}
                            </span>
                          ) : (
                            <>
                              {permitted.slice(0, 3).map((server) => (
                                <Badge key={server.id} variant="secondary">
                                  {server.name}
                                </Badge>
                              ))}
                              {permitted.length > 3 && (
                                <Badge variant="outline">
                                  {t("keys.additionalServers", {
                                    count: permitted.length - 3,
                                  })}
                                </Badge>
                              )}
                            </>
                          )}
                        </div>
                      </div>
                      <Button
                        type="button"
                        variant="outline"
                        size="sm"
                        className="shrink-0"
                        onClick={() => openAccessEditor(key)}
                      >
                        {t("keys.editAccess")}
                      </Button>
                    </div>
                    <p className="mt-2 text-xs text-muted-foreground">
                      {t("keys.createdAt", {
                        value: formatDateTime(key.createdAt),
                      })}
                    </p>
                  </div>
                </CardContent>
              </Card>
            );
          })}
        </div>
      )}

      <Dialog
        open={createOpen}
        onOpenChange={(open) => !isCreating && setCreateOpen(open)}
      >
        <DialogContent className="max-w-2xl">
          <DialogHeader>
            <DialogTitle>{t("keys.createTitle")}</DialogTitle>
            <DialogDescription>{t("keys.createDescription")}</DialogDescription>
          </DialogHeader>
          <div className="space-y-5 py-1">
            <div className="space-y-2">
              <Label htmlFor="access-key-name">{t("keys.keyName")}</Label>
              <Input
                id="access-key-name"
                autoFocus
                value={newKeyName}
                onChange={(event) => setNewKeyName(event.target.value)}
                placeholder={t("keys.namePlaceholder")}
                onKeyDown={(event) => {
                  if (event.key === "Enter" && !isCreating) {
                    void handleCreateKey();
                  }
                }}
              />
            </div>
            <PermissionPicker
              servers={sortedServers}
              access={newKeyAccess}
              onChange={setNewKeyAccess}
            />
          </div>
          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              disabled={isCreating}
              onClick={() => setCreateOpen(false)}
            >
              {t("common.cancel")}
            </Button>
            <Button
              type="button"
              disabled={isCreating}
              onClick={() => void handleCreateKey()}
            >
              <KeyRound className="h-4 w-4" />
              {isCreating ? t("keys.creating") : t("keys.create")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog
        open={Boolean(generatedKey)}
        onOpenChange={(open) => !open && setGeneratedKey(null)}
      >
        <DialogContent className="max-w-xl">
          <DialogHeader>
            <DialogTitle>{t("keys.generatedTitle")}</DialogTitle>
            <DialogDescription>
              {t("keys.generatedDescription", { name: generatedKey?.name })}
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-4">
            <div className="rounded-lg border border-amber-500/30 bg-amber-500/10 p-3 text-sm">
              {t("keys.oneTimeWarning")}
            </div>
            <div className="space-y-2">
              <Label>{t("keys.createdKey")}</Label>
              <div className="flex gap-2">
                <Input
                  value={generatedKey?.token ?? ""}
                  readOnly
                  className="font-mono text-xs"
                  onFocus={(event) => event.currentTarget.select()}
                />
                <Button
                  type="button"
                  variant="outline"
                  size="icon"
                  onClick={() =>
                    generatedKey &&
                    void copyText(generatedKey.token, t("keys.copySuccess"))
                  }
                  aria-label={t("keys.copy")}
                >
                  <Copy className="h-4 w-4" />
                </Button>
              </div>
            </div>
          </div>
          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() =>
                generatedKey &&
                void copyText(
                  mcpClientConfig(mcpEndpoint, generatedKey.token),
                  t("keys.copyConfigSuccess"),
                )
              }
            >
              <Copy className="h-4 w-4" />
              {t("keys.copyReadyConfig")}
            </Button>
            <Button type="button" onClick={() => setGeneratedKey(null)}>
              {t("keys.done")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog open={setupOpen} onOpenChange={setSetupOpen}>
        <DialogContent className="max-w-2xl">
          <DialogHeader>
            <DialogTitle>{t("keys.usageTitle")}</DialogTitle>
            <DialogDescription>{t("keys.usageDescription")}</DialogDescription>
          </DialogHeader>
          <div className="space-y-4">
            <div className="space-y-2">
              <Label>{t("keys.mcpEndpoint")}</Label>
              <div className="flex gap-2">
                <Input
                  value={mcpEndpoint}
                  readOnly
                  className="font-mono text-xs"
                />
                <Button
                  type="button"
                  variant="outline"
                  size="icon"
                  onClick={() =>
                    void copyText(mcpEndpoint, t("keys.copyEndpointSuccess"))
                  }
                >
                  <Copy className="h-4 w-4" />
                </Button>
              </div>
            </div>
            <div className="overflow-hidden rounded-lg border">
              <div className="flex items-center justify-between border-b px-3 py-2">
                <span className="text-sm font-medium">
                  {t("keys.clientConfigTitle")}
                </span>
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  onClick={() =>
                    void copyText(
                      mcpClientConfig(mcpEndpoint),
                      t("keys.copyConfigSuccess"),
                    )
                  }
                >
                  <Copy className="h-4 w-4" />
                  {t("keys.copyConfig")}
                </Button>
              </div>
              <pre className="max-h-72 overflow-auto bg-muted/20 p-3 text-xs">
                <code>{mcpClientConfig(mcpEndpoint)}</code>
              </pre>
            </div>
            <p className="text-xs text-muted-foreground">
              {t("keys.authHeaderHelp")}
            </p>
          </div>
          <DialogFooter>
            <Button type="button" onClick={() => setSetupOpen(false)}>
              {t("common.close")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog
        open={Boolean(accessTarget)}
        onOpenChange={(open) =>
          !isSavingAccess && !open && setAccessTarget(null)
        }
      >
        <DialogContent className="max-w-2xl">
          <DialogHeader>
            <DialogTitle>{t("keys.editAccessTitle")}</DialogTitle>
            <DialogDescription>
              {t("keys.editAccessDescription", { name: accessTarget?.name })}
            </DialogDescription>
          </DialogHeader>
          <PermissionPicker
            servers={sortedServers}
            access={accessDraft}
            onChange={setAccessDraft}
          />
          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              disabled={isSavingAccess}
              onClick={() => setAccessTarget(null)}
            >
              {t("common.cancel")}
            </Button>
            <Button
              type="button"
              disabled={isSavingAccess}
              onClick={() => void handleSaveAccess()}
            >
              {isSavingAccess ? t("keys.savingAccess") : t("common.save")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

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
              onClick={() => void handleRevokeKey()}
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

function PermissionPicker({
  servers,
  access,
  onChange,
}: {
  servers: Array<{ id: string; name: string }>;
  access: ServerAccessMap;
  onChange: React.Dispatch<React.SetStateAction<ServerAccessMap>>;
}) {
  const { t } = useTranslation();
  const selected = servers.filter(
    (server) => access[server.id] === true,
  ).length;

  const setAll = (allowed: boolean) => {
    onChange(Object.fromEntries(servers.map((server) => [server.id, allowed])));
  };

  return (
    <div className="overflow-hidden rounded-lg border">
      <div className="flex flex-wrap items-center justify-between gap-2 border-b bg-muted/20 px-3 py-2.5">
        <div className="flex items-center gap-2">
          <span className="text-sm font-medium">{t("keys.serverAccess")}</span>
          <Badge variant="secondary">
            {t("keys.selectedCount", { selected, total: servers.length })}
          </Badge>
        </div>
        {servers.length > 0 && (
          <div className="flex gap-1">
            <Button
              type="button"
              variant="ghost"
              size="sm"
              onClick={() => setAll(true)}
            >
              {t("keys.selectAll")}
            </Button>
            <Button
              type="button"
              variant="ghost"
              size="sm"
              onClick={() => setAll(false)}
            >
              {t("keys.clearAll")}
            </Button>
          </div>
        )}
      </div>
      {servers.length === 0 ? (
        <div className="p-4 text-sm text-muted-foreground">
          {t("keys.noServers")}
        </div>
      ) : (
        <div className="grid max-h-72 gap-2 overflow-y-auto p-3 sm:grid-cols-2">
          {servers.map((server) => {
            const checked = access[server.id] === true;
            return (
              <div
                key={server.id}
                className={cn(
                  "flex min-w-0 items-center gap-3 rounded-lg border px-3 py-3 transition-colors",
                  checked
                    ? "border-primary/40 bg-primary/5 text-foreground shadow-sm"
                    : "border-border/70 bg-background text-muted-foreground",
                )}
              >
                <span
                  className={cn(
                    "flex h-8 w-8 shrink-0 items-center justify-center rounded-md transition-colors",
                    checked
                      ? "bg-primary/10 text-primary"
                      : "bg-muted text-muted-foreground",
                  )}
                >
                  <ShieldCheck className="h-4 w-4" />
                </span>
                <span className="min-w-0 flex-1 truncate text-sm font-medium">
                  {server.name}
                </span>
                <Switch
                  checked={checked}
                  onCheckedChange={(allowed) =>
                    onChange((current) => ({
                      ...current,
                      [server.id]: allowed,
                    }))
                  }
                  aria-label={`${server.name} ${t("keys.serverAccess")}`}
                />
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}

function normalizeAccess(
  servers: Array<{ id: string; name: string }>,
  access: ServerAccessMap,
  defaultAllowed: boolean,
) {
  return Object.fromEntries(
    servers.map((server) => [
      server.id,
      access[server.id] === undefined
        ? defaultAllowed
        : access[server.id] === true,
    ]),
  );
}

function mcpClientConfig(endpoint: string, accessKey = "<YOUR_ACCESS_KEY>") {
  return JSON.stringify(
    {
      mcpServers: {
        "mcp-link": {
          url: endpoint,
          headers: {
            Authorization: `Bearer ${accessKey}`,
          },
        },
      },
    },
    null,
    2,
  );
}

export default KeyManager;
