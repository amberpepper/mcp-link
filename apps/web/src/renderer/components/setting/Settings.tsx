import React, { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  Button,
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  Input,
} from "@mcp_link/ui";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@mcp_link/ui";
import { Switch } from "@mcp_link/ui";
import {
  Download,
  Eye,
  EyeOff,
  FolderOpen,
  Plus,
  RefreshCw,
  Trash2,
} from "lucide-react";
import type { CloseBehavior, NetworkInterfaceAddress } from "@mcp_link/shared";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { toast } from "sonner";
import { useThemeStore } from "@/renderer/stores";
import { localPlatformAPI as platformAPI } from "../../platform-api/runtime-platform-api";
import { isTauriRuntime } from "../../platform-api/tauri-platform-api";
import { normalizeAppLanguage } from "../../utils/i18n";
import PageLayout from "@/renderer/components/layout/PageLayout";

const Settings: React.FC = () => {
  const { t, i18n } = useTranslation();
  const [loadExternalMCPConfigs, setLoadExternalMCPConfigs] =
    useState<boolean>(false);
  const [showWindowOnStartup, setShowWindowOnStartup] =
    useState<boolean>(false);
  const [closeBehavior, setCloseBehavior] = useState<CloseBehavior>("exit");
  const [skillAgentPaths, setSkillAgentPaths] = useState<string[]>([]);
  const [newSkillAgentPath, setNewSkillAgentPath] = useState("");
  const [networkInterfaces, setNetworkInterfaces] = useState<
    NetworkInterfaceAddress[]
  >([]);
  const [desktopMcpListenHost, setDesktopMcpListenHost] = useState("127.0.0.1");
  const [desktopMcpListenPort, setDesktopMcpListenPort] = useState("3284");
  const [serverPassword, setServerPassword] = useState("admin");
  const [showServerPassword, setShowServerPassword] = useState(false);
  const [isSavingSettings, setIsSavingSettings] = useState(false);
  const [availableUpdate, setAvailableUpdate] = useState<Update | null>(null);
  const [updateStatus, setUpdateStatus] = useState<
    "idle" | "checking" | "available" | "latest" | "installing" | "error"
  >("idle");
  const [updateProgress, setUpdateProgress] = useState(0);
  const isDesktopRuntime = isTauriRuntime();

  // Zustand stores
  const { theme, setTheme } = useThemeStore();

  const networkOptions = useMemo(() => {
    if (
      networkInterfaces.some((item) => item.address === desktopMcpListenHost) ||
      !desktopMcpListenHost
    ) {
      return networkInterfaces;
    }

    return [
      {
        name: "Configured",
        address: desktopMcpListenHost,
        family: "ipv4" as const,
        isLoopback: desktopMcpListenHost === "127.0.0.1",
        label: `${desktopMcpListenHost} (Configured)`,
      },
      ...networkInterfaces,
    ];
  }, [desktopMcpListenHost, networkInterfaces]);

  const restartDesktopMcpEndpoint = async () => {
    if (!isDesktopRuntime) return;
    await platformAPI.settings.restartDesktopMcpEndpoint();
  };

  const handleCheckForUpdates = async (showLatestMessage = true) => {
    if (!isDesktopRuntime || updateStatus === "checking") return;

    setUpdateStatus("checking");
    try {
      const update = await check({ timeout: 15_000 });
      if (update) {
        setAvailableUpdate(update);
        setUpdateStatus("available");
        toast.success(
          t("settings.updateAvailable", { version: update.version }),
        );
      } else {
        setAvailableUpdate(null);
        setUpdateStatus("latest");
        if (showLatestMessage) toast.success(t("settings.updateLatest"));
      }
    } catch (error) {
      console.error("Failed to check for updates:", error);
      setUpdateStatus(showLatestMessage ? "error" : "idle");
      if (showLatestMessage) toast.error(t("settings.updateCheckFailed"));
    }
  };

  const handleInstallUpdate = async () => {
    if (!availableUpdate || updateStatus === "installing") return;

    setUpdateStatus("installing");
    setUpdateProgress(0);
    let downloaded = 0;
    let total = 0;

    try {
      await availableUpdate.downloadAndInstall((event) => {
        if (event.event === "Started") {
          total = event.data.contentLength ?? 0;
        } else if (event.event === "Progress") {
          downloaded += event.data.chunkLength;
          if (total > 0) {
            setUpdateProgress(
              Math.min(100, Math.round((downloaded / total) * 100)),
            );
          }
        } else if (event.event === "Finished") {
          setUpdateProgress(100);
        }
      });
      await relaunch();
    } catch (error) {
      console.error("Failed to install update:", error);
      setUpdateStatus("available");
      toast.error(t("settings.updateInstallFailed"));
    }
  };

  const handleLanguageChange = async (value: string) => {
    const language = normalizeAppLanguage(value);
    const previousLanguage = normalizeAppLanguage(i18n.language);

    setIsSavingSettings(true);
    try {
      await i18n.changeLanguage(language);
      const currentSettings = await platformAPI.settings.get();
      const saved = await platformAPI.settings.save({
        ...currentSettings,
        language,
      });

      if (!saved) {
        throw new Error("settings.save returned false");
      }
    } catch (error) {
      console.error("Failed to save language settings:", error);
      await i18n.changeLanguage(previousLanguage);
    } finally {
      setIsSavingSettings(false);
    }
  };

  // Get normalized language code for select
  const getCurrentLanguage = () => {
    return normalizeAppLanguage(i18n.language);
  };

  // Load settings on mount
  useEffect(() => {
    const loadSettings = async () => {
      try {
        const [settings, interfaces] = await Promise.all([
          platformAPI.settings.get(),
          platformAPI.settings.listNetworkInterfaces().catch(() => []),
        ]);
        setLoadExternalMCPConfigs(settings.loadExternalMCPConfigs ?? false);
        setShowWindowOnStartup(settings.showWindowOnStartup ?? false);
        setCloseBehavior(settings.closeBehavior ?? "exit");
        setSkillAgentPaths(settings.skillAgentPaths ?? []);
        setDesktopMcpListenHost(settings.desktopMcpListenHost ?? "127.0.0.1");
        setDesktopMcpListenPort(String(settings.desktopMcpListenPort ?? 3284));
        setServerPassword(settings.serverPassword ?? "admin");
        setNetworkInterfaces(interfaces);
      } catch {
        console.log("Failed to load settings, using defaults");
      }
    };
    loadSettings();
  }, []);

  useEffect(() => {
    if (isDesktopRuntime) void handleCheckForUpdates(false);
    // Only check automatically once when the desktop settings page opens.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isDesktopRuntime]);

  // Handle external MCP configs toggle
  const handleExternalMCPConfigsToggle = async (checked: boolean) => {
    setLoadExternalMCPConfigs(checked);
    setIsSavingSettings(true);
    try {
      const currentSettings = await platformAPI.settings.get();
      await platformAPI.settings.save({
        ...currentSettings,
        loadExternalMCPConfigs: checked,
      });
    } catch (error) {
      console.error("Failed to save settings:", error);
      setLoadExternalMCPConfigs(!checked);
    } finally {
      setIsSavingSettings(false);
    }
  };

  // Handle startup visibility toggle
  const handleStartupVisibilityToggle = async (checked: boolean) => {
    setShowWindowOnStartup(checked);
    setIsSavingSettings(true);
    try {
      const currentSettings = await platformAPI.settings.get();
      await platformAPI.settings.save({
        ...currentSettings,
        showWindowOnStartup: checked,
      });
    } catch (error) {
      console.error("Failed to save startup visibility settings:", error);
      setShowWindowOnStartup(!checked);
    } finally {
      setIsSavingSettings(false);
    }
  };

  const handleCloseBehaviorChange = async (value: string) => {
    const nextBehavior = value as CloseBehavior;
    const previousBehavior = closeBehavior;
    setCloseBehavior(nextBehavior);
    setIsSavingSettings(true);
    try {
      const currentSettings = await platformAPI.settings.get();
      await platformAPI.settings.save({
        ...currentSettings,
        closeBehavior: nextBehavior,
      });
    } catch (error) {
      console.error("Failed to save close behavior:", error);
      setCloseBehavior(previousBehavior);
    } finally {
      setIsSavingSettings(false);
    }
  };

  const handleDesktopMcpHostChange = async (value: string) => {
    const previous = desktopMcpListenHost;
    setDesktopMcpListenHost(value);
    setIsSavingSettings(true);
    try {
      const currentSettings = await platformAPI.settings.get();
      await platformAPI.settings.save({
        ...currentSettings,
        desktopMcpListenHost: value,
      });
      await restartDesktopMcpEndpoint();
    } catch (error) {
      console.error("Failed to save MCP listen host:", error);
      setDesktopMcpListenHost(previous);
    } finally {
      setIsSavingSettings(false);
    }
  };

  const handleDesktopMcpPortSave = async () => {
    const port = Number(desktopMcpListenPort);
    if (!Number.isInteger(port) || port < 1 || port > 65535) {
      setDesktopMcpListenPort("3284");
      return;
    }

    setIsSavingSettings(true);
    try {
      const currentSettings = await platformAPI.settings.get();
      await platformAPI.settings.save({
        ...currentSettings,
        desktopMcpListenPort: port,
      });
      setDesktopMcpListenPort(String(port));
      await restartDesktopMcpEndpoint();
    } catch (error) {
      console.error("Failed to save MCP listen port:", error);
    } finally {
      setIsSavingSettings(false);
    }
  };

  const handleServerPasswordSave = async () => {
    const trimmed = serverPassword.trim();
    if (!trimmed) {
      setServerPassword("admin");
      return;
    }
    setIsSavingSettings(true);
    try {
      const currentSettings = await platformAPI.settings.get();
      await platformAPI.settings.save({
        ...currentSettings,
        serverPassword: trimmed,
      });
      setServerPassword(trimmed);
    } catch (error) {
      console.error("Failed to save server password:", error);
    } finally {
      setIsSavingSettings(false);
    }
  };

  const saveSkillAgentPaths = async (paths: string[]) => {
    const normalized = Array.from(
      new Set(paths.map((path) => path.trim()).filter(Boolean)),
    );
    const previous = skillAgentPaths;
    setSkillAgentPaths(normalized);
    setIsSavingSettings(true);
    try {
      const currentSettings = await platformAPI.settings.get();
      await platformAPI.settings.save({
        ...currentSettings,
        skillAgentPaths: normalized,
      });
    } catch (error) {
      console.error("Failed to save Skill agent paths:", error);
      setSkillAgentPaths(previous);
    } finally {
      setIsSavingSettings(false);
    }
  };

  const handleAddSkillAgentPath = async () => {
    if (!newSkillAgentPath.trim()) return;
    await saveSkillAgentPaths([...skillAgentPaths, newSkillAgentPath]);
    setNewSkillAgentPath("");
  };

  const handleBrowseSkillAgentPath = async () => {
    const result = await platformAPI.servers.selectFile({
      title: t("settings.skillAgentPathsBrowse"),
      mode: "directory",
    });
    if (result.success && result.path) {
      await saveSkillAgentPaths([...skillAgentPaths, result.path]);
    }
  };

  return (
    <PageLayout
      title={t("common.settings")}
      contentClassName="flex flex-col gap-6"
    >
      {/* Preferences Section */}
      <Card>
        <CardHeader>
          <CardTitle className="text-xl">{t("settings.preferences")}</CardTitle>
        </CardHeader>
        <CardContent className="space-y-6">
          {/* Language */}
          <div className="flex items-center justify-between">
            <div className="space-y-0.5">
              <label className="text-sm font-medium">
                {t("common.language")}
              </label>
            </div>
            <Select
              value={getCurrentLanguage()}
              onValueChange={handleLanguageChange}
              disabled={isSavingSettings}
            >
              <SelectTrigger className="w-[180px]">
                <SelectValue placeholder={t("common.language")} />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="en">English</SelectItem>
                <SelectItem value="zh">中文</SelectItem>
                <SelectItem value="ja">日本語</SelectItem>
              </SelectContent>
            </Select>
          </div>

          {/* Theme */}
          <div className="flex items-center justify-between">
            <div className="space-y-0.5">
              <label className="text-sm font-medium">
                {t("settings.theme")}
              </label>
            </div>
            <Select
              value={theme}
              onValueChange={(value: "light" | "dark" | "system") =>
                setTheme(value)
              }
            >
              <SelectTrigger className="w-[180px]">
                <SelectValue placeholder={t("settings.theme")} />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="light">
                  {t("settings.themeLight")}
                </SelectItem>
                <SelectItem value="dark">{t("settings.themeDark")}</SelectItem>
                <SelectItem value="system">
                  {t("settings.themeSystem")}
                </SelectItem>
              </SelectContent>
            </Select>
          </div>

          <div className="flex items-center justify-between gap-4">
            <label className="text-sm font-medium">
              {t("settings.closeBehavior")}
            </label>
            <Select
              value={closeBehavior}
              onValueChange={handleCloseBehaviorChange}
              disabled={isSavingSettings}
            >
              <SelectTrigger className="w-44">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="exit">
                  {t("settings.closeBehaviorExit")}
                </SelectItem>
                <SelectItem value="minimizeToTray">
                  {t("settings.closeBehaviorTray")}
                </SelectItem>
              </SelectContent>
            </Select>
          </div>

          {/* Show Window on Startup */}
          <div className="flex items-center justify-between">
            <label className="text-sm font-medium">
              {t("settings.showWindowOnStartup")}
            </label>
            <Switch
              checked={showWindowOnStartup}
              onCheckedChange={handleStartupVisibilityToggle}
              disabled={isSavingSettings}
            />
          </div>

          {/* Load External MCP Configs */}
          <div className="flex items-center justify-between">
            <label className="text-sm font-medium">
              {t("settings.loadExternalMCPConfigs")}
            </label>
            <Switch
              checked={loadExternalMCPConfigs}
              onCheckedChange={handleExternalMCPConfigsToggle}
              disabled={isSavingSettings}
            />
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="text-xl">
            {t("settings.skillAgentPaths")}
          </CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          <p className="text-xs text-muted-foreground">
            {t("settings.skillAgentPathsDescription")}
          </p>
          {skillAgentPaths.length === 0 && (
            <p className="rounded-md border bg-muted/30 p-3 text-sm text-muted-foreground">
              {t("settings.skillAgentPathsDefault")}
            </p>
          )}
          <div className="space-y-2">
            {skillAgentPaths.map((path) => (
              <div key={path} className="flex items-center gap-2">
                <Input value={path} readOnly className="font-mono text-xs" />
                <Button
                  type="button"
                  variant="outline"
                  size="icon"
                  disabled={isSavingSettings}
                  onClick={() =>
                    saveSkillAgentPaths(
                      skillAgentPaths.filter((item) => item !== path),
                    )
                  }
                  aria-label={t("common.delete")}
                >
                  <Trash2 className="h-4 w-4" />
                </Button>
              </div>
            ))}
          </div>
          <div className="flex gap-2">
            <Input
              value={newSkillAgentPath}
              onChange={(event) => setNewSkillAgentPath(event.target.value)}
              placeholder={t("settings.skillAgentPathsPlaceholder")}
              className="font-mono text-xs"
              disabled={isSavingSettings}
              onKeyDown={(event) => {
                if (event.key === "Enter") void handleAddSkillAgentPath();
              }}
            />
            <Button
              type="button"
              variant="outline"
              size="icon"
              disabled={isSavingSettings || !newSkillAgentPath.trim()}
              onClick={handleAddSkillAgentPath}
              aria-label={t("common.add")}
            >
              <Plus className="h-4 w-4" />
            </Button>
            {isDesktopRuntime && (
              <Button
                type="button"
                variant="outline"
                onClick={handleBrowseSkillAgentPath}
                disabled={isSavingSettings}
              >
                <FolderOpen className="h-4 w-4" />
                {t("settings.skillAgentPathsBrowse")}
              </Button>
            )}
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="text-xl">
            {t("settings.desktopMcpEndpoint")}
          </CardTitle>
        </CardHeader>
        <CardContent className="grid gap-4 md:grid-cols-[minmax(0,1fr)_180px]">
          <div className="space-y-2">
            <label className="text-sm font-medium">
              {t("settings.listenAddress")}
            </label>
            <Select
              value={desktopMcpListenHost}
              onValueChange={handleDesktopMcpHostChange}
              disabled={isSavingSettings || networkOptions.length === 0}
            >
              <SelectTrigger>
                <SelectValue placeholder={t("settings.listenAddress")} />
              </SelectTrigger>
              <SelectContent>
                {networkOptions.map((item) => (
                  <SelectItem key={item.address} value={item.address}>
                    {item.label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>

          <div className="space-y-2">
            <label className="text-sm font-medium">
              {t("settings.listenPort")}
            </label>
            <div className="flex gap-2">
              <Input
                type="number"
                min={1}
                max={65535}
                value={desktopMcpListenPort}
                onChange={(event) =>
                  setDesktopMcpListenPort(event.target.value)
                }
                disabled={isSavingSettings}
              />
              <Button
                type="button"
                variant="outline"
                onClick={handleDesktopMcpPortSave}
                disabled={isSavingSettings}
              >
                {t("common.save")}
              </Button>
            </div>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="text-xl">
            {t("settings.serverPasswordTitle")}
          </CardTitle>
        </CardHeader>
        <CardContent className="space-y-2">
          <p className="text-xs text-muted-foreground">
            {t("settings.serverPasswordDescription")}
          </p>
          <div className="flex gap-2">
            <div className="relative min-w-0 flex-1">
              <Input
                type={showServerPassword ? "text" : "password"}
                value={serverPassword}
                onChange={(event) => setServerPassword(event.target.value)}
                disabled={isSavingSettings}
                placeholder="admin"
                className="pr-10"
              />
              <Button
                type="button"
                variant="ghost"
                size="icon"
                className="absolute right-1 top-1/2 h-7 w-7 -translate-y-1/2"
                onClick={() => setShowServerPassword((value) => !value)}
                disabled={isSavingSettings}
                aria-label={
                  showServerPassword
                    ? t("common.hidePassword")
                    : t("common.showPassword")
                }
              >
                {showServerPassword ? (
                  <EyeOff className="h-4 w-4" />
                ) : (
                  <Eye className="h-4 w-4" />
                )}
              </Button>
            </div>
            <Button
              type="button"
              variant="outline"
              onClick={handleServerPasswordSave}
              disabled={isSavingSettings}
            >
              {t("common.save")}
            </Button>
          </div>
        </CardContent>
      </Card>

      {isDesktopRuntime && (
        <Card>
          <CardHeader>
            <CardTitle className="text-xl">{t("settings.appUpdate")}</CardTitle>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
              <div className="space-y-1">
                <p className="text-sm font-medium">
                  {availableUpdate
                    ? t("settings.updateVersion", {
                        current: availableUpdate.currentVersion,
                        latest: availableUpdate.version,
                      })
                    : updateStatus === "latest"
                      ? t("settings.updateLatest")
                      : updateStatus === "error"
                        ? t("settings.updateCheckFailed")
                        : t("settings.updateDescription")}
                </p>
                {availableUpdate?.body && (
                  <p className="whitespace-pre-wrap text-xs text-muted-foreground">
                    {availableUpdate.body}
                  </p>
                )}
                {updateStatus === "installing" && (
                  <p className="text-xs text-muted-foreground">
                    {t("settings.updateInstalling", {
                      progress: updateProgress,
                    })}
                  </p>
                )}
              </div>
              <div className="flex shrink-0 gap-2">
                <Button
                  type="button"
                  variant="outline"
                  onClick={() => handleCheckForUpdates()}
                  disabled={
                    updateStatus === "checking" || updateStatus === "installing"
                  }
                >
                  <RefreshCw
                    className={`h-4 w-4 ${updateStatus === "checking" ? "animate-spin" : ""}`}
                  />
                  {updateStatus === "checking"
                    ? t("settings.updateChecking")
                    : t("settings.updateCheck")}
                </Button>
                {availableUpdate && (
                  <Button
                    type="button"
                    onClick={handleInstallUpdate}
                    disabled={updateStatus === "installing"}
                  >
                    <Download className="h-4 w-4" />
                    {updateStatus === "installing"
                      ? t("settings.updateInstallingButton")
                      : t("settings.updateInstall")}
                  </Button>
                )}
              </div>
            </div>
          </CardContent>
        </Card>
      )}
    </PageLayout>
  );
};

export default Settings;
