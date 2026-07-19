import React, {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
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
  Input,
  Label,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  Switch,
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@mcp_link/ui";
import {
  IconFolder,
  IconPlug,
  IconPlus,
  IconRefresh,
  IconTrash,
  IconUpload,
} from "@tabler/icons-react";
import type {
  AgentInstanceEntry,
  AgentInstanceInput,
  AgentPluginDescriptor,
} from "@mcp_link/shared";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router-dom";
import { toast } from "sonner";

import PageLayout from "@/renderer/components/layout/PageLayout";
import { usePlatformAPI } from "@/renderer/platform-api";
import { isTauriRuntime } from "@/renderer/platform-api/tauri-platform-api";
import AgentAvatar from "./AgentAvatar";

const AgentsPage: React.FC = () => {
  const { t } = useTranslation();
  const platformAPI = usePlatformAPI();
  const navigate = useNavigate();
  const [plugins, setPlugins] = useState<AgentPluginDescriptor[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [instanceOpen, setInstanceOpen] = useState(false);
  const [pluginManagerOpen, setPluginManagerOpen] = useState(false);
  const [instanceForm, setInstanceForm] = useState<Partial<AgentInstanceInput>>(
    {},
  );
  const [instanceBusy, setInstanceBusy] = useState(false);
  const [instanceRemoveTarget, setInstanceRemoveTarget] =
    useState<AgentInstanceEntry | null>(null);
  const [pluginRemoveTarget, setPluginRemoveTarget] =
    useState<AgentPluginDescriptor | null>(null);
  const pluginUploadRef = useRef<HTMLInputElement>(null);
  const selectedPlugin = plugins.find(
    (plugin) => plugin.id === instanceForm.agentId,
  );

  const load = useCallback(async () => {
    setIsLoading(true);
    try {
      setPlugins(await platformAPI.agents.list());
    } catch (error) {
      toast.error(errorMessage(error, t("agents.loadFailed")));
    } finally {
      setIsLoading(false);
    }
  }, [platformAPI, t]);

  useEffect(() => {
    void load();
  }, [load]);

  useEffect(() => {
    const refresh = () => void load();
    window.addEventListener("agent-plugins-changed", refresh);
    return () => window.removeEventListener("agent-plugins-changed", refresh);
  }, [load]);

  const instances = useMemo<AgentInstanceEntry[]>(
    () =>
      plugins.flatMap((plugin) =>
        plugin.instances.map((instance) => ({ plugin, instance })),
      ),
    [plugins],
  );
  const selectablePlugins = plugins.filter((plugin) => plugin.enabled);
  const installedPlugins = plugins;

  const openAdd = () => {
    setInstanceForm({
      agentId: selectablePlugins[0]?.id ?? "",
      configRoot: "",
    });
    setInstanceOpen(true);
  };

  const browseConfigRoot = async () => {
    const result = await platformAPI.servers.selectFile({
      title: t("agents.chooseCliConfigDirectory"),
      mode: "directory",
    });
    if (result.success && result.path) {
      setInstanceForm((current) => ({ ...current, configRoot: result.path }));
    }
  };

  const saveInstance = async () => {
    if (!instanceForm.agentId || !instanceForm.configRoot) return;
    setInstanceBusy(true);
    const input: AgentInstanceInput = {
      agentId: instanceForm.agentId,
      configRoot: instanceForm.configRoot,
    };
    try {
      await platformAPI.agents.instances.create(input);
      toast.success(t("agents.instanceCreated"));
      setInstanceOpen(false);
      await load();
    } catch (error) {
      toast.error(errorMessage(error, t("agents.instanceSaveFailed")));
    } finally {
      setInstanceBusy(false);
    }
  };

  const removeInstance = async () => {
    if (!instanceRemoveTarget) return;
    try {
      await platformAPI.agents.instances.remove(
        instanceRemoveTarget.instance.id,
      );
      toast.success(
        t("agents.instanceRemoved", { name: instanceRemoveTarget.plugin.name }),
      );
      setInstanceRemoveTarget(null);
      await load();
    } catch (error) {
      toast.error(errorMessage(error, t("agents.instanceRemoveFailed")));
    }
  };

  const togglePlugin = async (
    plugin: AgentPluginDescriptor,
    enabled: boolean,
  ) => {
    try {
      await platformAPI.agents.plugins.setEnabled(plugin.id, enabled);
      await load();
    } catch (error) {
      toast.error(errorMessage(error, t("agents.updateFailed")));
    }
  };

  const importPlugin = async () => {
    if (!isTauriRuntime()) {
      pluginUploadRef.current?.click();
      return;
    }
    try {
      const result = await platformAPI.agents.plugins.import();
      if (!result) return;
      if (result.installed.length === 1) {
        toast.success(
          t("agents.importSuccess", { name: result.installed[0].name }),
        );
      } else if (result.installed.length > 1) {
        toast.success(
          t("agents.importBatchSuccess", {
            count: result.installed.length,
            names: result.installed.map((plugin) => plugin.name).join("、"),
          }),
        );
      }
      if (result.failed.length > 0) {
        toast.error(
          t("agents.importBatchFailed", {
            count: result.failed.length,
            details: result.failed
              .map(
                (failure) =>
                  `${failure.fileName || "Unknown"}: ${failure.error}`,
              )
              .join("\n"),
          }),
        );
      }
      if (result.installed.length > 0) await load();
    } catch (error) {
      toast.error(errorMessage(error, t("agents.importFailed")));
    }
  };

  const uploadPlugins = async (files: FileList | null) => {
    if (!files?.length) return;
    const installed: AgentPluginDescriptor[] = [];
    const failed: Array<{ fileName: string; error: string }> = [];
    for (const file of Array.from(files)) {
      try {
        const bytes = Array.from(new Uint8Array(await file.arrayBuffer()));
        installed.push(await platformAPI.agents.plugins.install(bytes));
      } catch (error) {
        failed.push({
          fileName: file.name,
          error: errorMessage(error, t("agents.importFailed")),
        });
      }
    }
    if (installed.length === 1) {
      toast.success(t("agents.importSuccess", { name: installed[0].name }));
    } else if (installed.length > 1) {
      toast.success(
        t("agents.importBatchSuccess", {
          count: installed.length,
          names: installed.map((plugin) => plugin.name).join("、"),
        }),
      );
    }
    if (failed.length > 0) {
      toast.error(
        t("agents.importBatchFailed", {
          count: failed.length,
          details: failed
            .map((item) => `${item.fileName}: ${item.error}`)
            .join("\n"),
        }),
      );
    }
    if (pluginUploadRef.current) pluginUploadRef.current.value = "";
    await load();
  };

  const removePlugin = async () => {
    if (!pluginRemoveTarget) return;
    try {
      await platformAPI.agents.plugins.remove(pluginRemoveTarget.id);
      toast.success(
        t("agents.removeSuccess", { name: pluginRemoveTarget.name }),
      );
      setPluginRemoveTarget(null);
      await load();
    } catch (error) {
      toast.error(errorMessage(error, t("agents.removeFailed")));
    }
  };

  return (
    <PageLayout
      title={t("agents.title")}
      toolbar={
        <div className="flex items-center gap-2">
          <Button
            variant="outline"
            size="sm"
            className="h-8"
            onClick={() => void load()}
          >
            <IconRefresh
              className={isLoading ? "h-4 w-4 animate-spin" : "h-4 w-4"}
            />
            {t("common.refresh")}
          </Button>
          <Button
            variant="outline"
            size="sm"
            className="h-8"
            onClick={() => setPluginManagerOpen(true)}
          >
            <IconPlug className="h-4 w-4" />
            {t("agents.managePlugins")}
          </Button>
          <Button
            size="sm"
            className="h-8"
            disabled={selectablePlugins.length === 0}
            onClick={openAdd}
          >
            <IconPlus className="h-4 w-4" />
            {t("agents.addCli")}
          </Button>
        </div>
      }
    >
      <input
        ref={pluginUploadRef}
        type="file"
        accept=".mclagent,.zip,application/zip"
        multiple
        className="hidden"
        onChange={(event) => void uploadPlugins(event.target.files)}
      />
      {isLoading ? (
        <div className="p-6 text-sm text-muted-foreground">
          {t("common.loading")}
        </div>
      ) : instances.length === 0 ? (
        <div className="flex min-h-[360px] items-center justify-center">
          <Button
            size="lg"
            disabled={selectablePlugins.length === 0}
            onClick={openAdd}
          >
            <IconPlus className="h-5 w-5" />
            {t("agents.addCli")}
          </Button>
        </div>
      ) : (
        <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-3">
          {instances.map((entry) => (
            <Card
              key={entry.instance.id}
              className="group cursor-pointer overflow-hidden transition-colors hover:bg-muted/30"
              role="link"
              tabIndex={0}
              onClick={() => navigate(`/agents/${entry.instance.id}`)}
              onKeyDown={(event) => {
                if (event.key === "Enter" || event.key === " ") {
                  event.preventDefault();
                  navigate(`/agents/${entry.instance.id}`);
                }
              }}
            >
              <CardContent className="p-0">
                <div className="flex items-center justify-between gap-3 p-4">
                  <div className="flex min-w-0 items-center gap-3">
                    <AgentAvatar plugin={entry.plugin} size="lg" />
                    <div className="min-w-0">
                      <h3 className="truncate font-medium">
                        {entry.instance.label}
                      </h3>
                    </div>
                  </div>
                  <div className="flex shrink-0 opacity-70 transition-opacity group-hover:opacity-100">
                    <Tooltip>
                      <TooltipTrigger asChild>
                        <Button
                          size="icon"
                          variant="ghost"
                          aria-label={t("common.delete")}
                          onClick={(event) => {
                            event.stopPropagation();
                            setInstanceRemoveTarget(entry);
                          }}
                        >
                          <IconTrash className="h-4 w-4" />
                        </Button>
                      </TooltipTrigger>
                      <TooltipContent>{t("common.delete")}</TooltipContent>
                    </Tooltip>
                  </div>
                </div>
              </CardContent>
            </Card>
          ))}
        </div>
      )}

      <Dialog open={instanceOpen} onOpenChange={setInstanceOpen}>
        <DialogContent className="max-w-lg">
          <DialogHeader>
            <DialogTitle>{t("agents.addCli")}</DialogTitle>
            <DialogDescription>
              {t("agents.chooseCliDescription")}
            </DialogDescription>
            {selectedPlugin?.instanceConfig.pathHints?.length ? (
              <div className="rounded-md border bg-muted/30 p-3 text-xs text-muted-foreground">
                <p className="mb-1 font-medium text-foreground">
                  {t("agents.pathHintsTitle")}
                </p>
                {selectedPlugin.instanceConfig.pathHints.map((hint) => (
                  <p key={hint} className="break-all font-mono">
                    {hint}
                  </p>
                ))}
              </div>
            ) : null}
          </DialogHeader>
          <div className="space-y-4 py-2">
            <div className="space-y-2">
              <Label>{t("agents.selectPlugin")}</Label>
              <Select
                value={instanceForm.agentId ?? ""}
                onValueChange={(agentId) =>
                  setInstanceForm({ agentId, configRoot: "" })
                }
              >
                <SelectTrigger>
                  <SelectValue
                    placeholder={t("agents.selectPluginPlaceholder")}
                  />
                </SelectTrigger>
                <SelectContent>
                  {selectablePlugins.map((plugin) => (
                    <SelectItem key={plugin.id} value={plugin.id}>
                      {plugin.name}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div className="space-y-2">
              <Label>{t("agents.cliConfigDirectory")}</Label>
              <div className="flex gap-2">
                <Input
                  className="font-mono text-xs"
                  value={instanceForm.configRoot ?? ""}
                  onChange={(event) =>
                    setInstanceForm((current) => ({
                      ...current,
                      configRoot: event.target.value,
                    }))
                  }
                  placeholder={t("agents.cliConfigDirectoryPlaceholder")}
                />
                {isTauriRuntime() && (
                  <Button
                    type="button"
                    variant="outline"
                    onClick={() => void browseConfigRoot()}
                  >
                    <IconFolder className="h-4 w-4" />
                  </Button>
                )}
              </div>
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setInstanceOpen(false)}>
              {t("common.cancel")}
            </Button>
            <Button
              disabled={
                !instanceForm.agentId ||
                !instanceForm.configRoot ||
                instanceBusy
              }
              onClick={() => void saveInstance()}
            >
              {t("common.add")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog open={pluginManagerOpen} onOpenChange={setPluginManagerOpen}>
        <DialogContent className="max-w-2xl">
          <DialogHeader>
            <DialogTitle>{t("agents.managePlugins")}</DialogTitle>
            <DialogDescription>
              {t("agents.manageInstalledPluginsDescription")}
            </DialogDescription>
          </DialogHeader>
          <div className="max-h-[60vh] space-y-2 overflow-auto py-2">
            {installedPlugins.map((plugin) => (
              <div
                key={plugin.id}
                className="flex items-center justify-between gap-3 rounded-lg border p-3"
              >
                <div className="min-w-0">
                  <div className="flex items-center gap-2">
                    <span className="font-medium">{plugin.name}</span>
                    <Badge variant="outline">v{plugin.version}</Badge>
                  </div>
                </div>
                <div className="flex shrink-0 items-center gap-2">
                  <Switch
                    checked={plugin.enabled}
                    onCheckedChange={(checked) =>
                      void togglePlugin(plugin, checked)
                    }
                  />
                  <Button
                    size="icon"
                    variant="ghost"
                    onClick={() => setPluginRemoveTarget(plugin)}
                  >
                    <IconTrash className="h-4 w-4" />
                  </Button>
                </div>
              </div>
            ))}
            {installedPlugins.length === 0 && (
              <div className="py-10 text-center text-sm text-muted-foreground">
                {t("agents.noInstalledPlugins")}
              </div>
            )}
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => void importPlugin()}>
              <IconUpload className="h-4 w-4" />
              {t("agents.importPlugin")}
            </Button>
            <Button onClick={() => setPluginManagerOpen(false)}>
              {t("common.close")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <AlertDialog
        open={Boolean(instanceRemoveTarget)}
        onOpenChange={(open) => !open && setInstanceRemoveTarget(null)}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              {t("agents.removeInstanceTitle")}
            </AlertDialogTitle>
            <AlertDialogDescription>
              {t("agents.removeInstanceDescription", {
                name: instanceRemoveTarget?.plugin.name,
              })}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("common.cancel")}</AlertDialogCancel>
            <AlertDialogAction onClick={() => void removeInstance()}>
              {t("common.delete")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      <AlertDialog
        open={Boolean(pluginRemoveTarget)}
        onOpenChange={(open) => !open && setPluginRemoveTarget(null)}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("agents.removeTitle")}</AlertDialogTitle>
            <AlertDialogDescription>
              {t("agents.removeDescription", {
                name: pluginRemoveTarget?.name,
              })}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("common.cancel")}</AlertDialogCancel>
            <AlertDialogAction onClick={() => void removePlugin()}>
              {t("common.delete")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </PageLayout>
  );
};

function errorMessage(error: unknown, fallback: string) {
  if (error instanceof Error && error.message) return error.message;
  if (typeof error === "string" && error.trim()) return error;
  if (error && typeof error === "object" && "message" in error) {
    const message = String(
      (error as { message?: unknown }).message ?? "",
    ).trim();
    if (message) return message;
  }
  return fallback;
}

export default AgentsPage;
