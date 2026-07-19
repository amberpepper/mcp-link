import React, {
  useState,
  useEffect,
  useCallback,
  useMemo,
  useRef,
} from "react";
import { useTranslation } from "react-i18next";
import {
  Button,
  Input,
  Badge,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  Label,
  Textarea,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@mcp_link/ui";
import {
  IconDownload,
  IconFolderOpen,
  IconPlus,
  IconSearch,
  IconTrash,
  IconLink,
} from "@tabler/icons-react";
import { usePlatformAPI } from "@/renderer/platform-api";
import type {
  AgentPluginDescriptor,
  AgentSkillTarget,
  SkillInstallation,
  SkillWithContent,
} from "@mcp_link/shared";
import { toast } from "sonner";
import PageLayout from "@/renderer/components/layout/PageLayout";
import EmbeddedSkillPage from "./EmbeddedSkillPage";
import { isTauriRuntime } from "@/renderer/platform-api/tauri-platform-api";

interface SkillsManagerProps {
  embedded?: boolean;
  toolbarEnd?: React.ReactNode;
  targetAgentId?: string;
}

const SkillsManager: React.FC<SkillsManagerProps> = ({
  embedded = false,
  toolbarEnd,
  targetAgentId,
}) => {
  const { t } = useTranslation();
  const platformAPI = usePlatformAPI();
  const saveTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const pendingSaveRef = useRef<{ skillId: string; content: string } | null>(
    null,
  );

  const [skills, setSkills] = useState<SkillWithContent[]>([]);
  const [skillTargets, setSkillTargets] = useState<AgentSkillTarget[]>([]);
  const [agents, setAgents] = useState<AgentPluginDescriptor[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [searchQuery, setSearchQuery] = useState("");

  // Selected skill state
  const [selectedSkillId, setSelectedSkillId] = useState<string | null>(null);
  const [skillMdContent, setSkillMdContent] = useState("");
  const [saveStatus, setSaveStatus] = useState<
    "idle" | "saving" | "saved" | "error"
  >("idle");

  // New skill dialog state
  const [isNewDialogOpen, setIsNewDialogOpen] = useState(false);
  const [newSkillName, setNewSkillName] = useState("");
  const [dialogError, setDialogError] = useState<string | null>(null);
  const [installTarget, setInstallTarget] = useState<AgentSkillTarget | null>(
    null,
  );
  const [installProjectPath, setInstallProjectPath] = useState("");
  const [installMode, setInstallMode] = useState<"copy" | "symlink" | "native">(
    "copy",
  );
  const [isInstalling, setIsInstalling] = useState(false);

  const loadSkills = useCallback(async () => {
    try {
      const [skillsList, targets, agentList] = await Promise.all([
        platformAPI.skills.list(),
        platformAPI.skills.listTargets(),
        platformAPI.agents.list(),
      ]);
      setSkills(skillsList);
      setSkillTargets(
        targetAgentId
          ? targets.filter((target) => target.agentId === targetAgentId)
          : targets,
      );
      setAgents(agentList);
    } catch (error) {
      console.error("Failed to load skills:", error);
      toast.error(t("skills.loadError"));
    } finally {
      setIsLoading(false);
    }
  }, [platformAPI, t, targetAgentId]);

  useEffect(() => {
    loadSkills();
  }, [loadSkills]);

  // Load content from selected skill
  useEffect(() => {
    if (selectedSkillId) {
      const skill = skills.find((s) => s.id === selectedSkillId);
      setSkillMdContent(skill?.content || "");
    }
    setSaveStatus("idle");
  }, [selectedSkillId]);

  // Auto-save with debounce
  const saveSkill = useCallback(
    async (skillId: string, content: string) => {
      setSaveStatus("saving");
      try {
        await platformAPI.skills.update(skillId, { content });
        setSkills((current) =>
          current.map((skill) =>
            skill.id === skillId ? { ...skill, content } : skill,
          ),
        );
        if (!pendingSaveRef.current) setSaveStatus("saved");
        return true;
      } catch (error) {
        console.error("Failed to auto-save:", error);
        setSaveStatus("error");
        toast.error(t("skills.saveError"));
        return false;
      }
    },
    [platformAPI, t],
  );

  const flushPendingSave = useCallback(async () => {
    if (saveTimeoutRef.current) {
      clearTimeout(saveTimeoutRef.current);
      saveTimeoutRef.current = null;
    }
    const pending = pendingSaveRef.current;
    if (!pending) return true;
    pendingSaveRef.current = null;
    const saved = await saveSkill(pending.skillId, pending.content);
    if (!saved && !pendingSaveRef.current) pendingSaveRef.current = pending;
    return saved;
  }, [saveSkill]);

  const handleContentChange = (e: React.ChangeEvent<HTMLTextAreaElement>) => {
    const newContent = e.target.value;
    setSkillMdContent(newContent);
    setSaveStatus("idle");

    // Clear previous timeout
    if (saveTimeoutRef.current) {
      clearTimeout(saveTimeoutRef.current);
    }

    // Set new timeout for auto-save (500ms debounce)
    if (selectedSkillId) {
      pendingSaveRef.current = {
        skillId: selectedSkillId,
        content: newContent,
      };
      saveTimeoutRef.current = setTimeout(() => {
        void flushPendingSave();
      }, 500);
    }
  };

  // Start the final save instead of silently discarding a pending edit.
  useEffect(() => {
    return () => {
      if (saveTimeoutRef.current) {
        clearTimeout(saveTimeoutRef.current);
      }
      const pending = pendingSaveRef.current;
      if (pending) {
        pendingSaveRef.current = null;
        void platformAPI.skills.update(pending.skillId, {
          content: pending.content,
        });
      }
    };
  }, [platformAPI]);

  // Filter skills by search query
  const filteredSkills = useMemo(() => {
    if (!searchQuery.trim()) {
      return skills;
    }
    const query = searchQuery.toLowerCase();
    return skills.filter((skill) => skill.name.toLowerCase().includes(query));
  }, [skills, searchQuery]);

  const handleSelectSkill = async (skillId: string) => {
    if (skillId === selectedSkillId) return;
    if (!(await flushPendingSave())) return;
    setSelectedSkillId(skillId);
  };

  const handleCreateSkill = async () => {
    if (!newSkillName.trim()) {
      setDialogError(t("skills.nameRequired"));
      return;
    }

    setDialogError(null);
    try {
      const skill = await platformAPI.skills.create({
        name: newSkillName.trim(),
      });
      toast.success(t("skills.createSuccess"));
      setIsNewDialogOpen(false);
      setNewSkillName("");
      await loadSkills();
      setSelectedSkillId(skill.id);
    } catch (error: any) {
      setDialogError(error.message || t("skills.createError"));
    }
  };

  const handleCloseNewDialog = () => {
    setIsNewDialogOpen(false);
    setNewSkillName("");
    setDialogError(null);
  };

  const handleImport = async () => {
    try {
      const skill = await platformAPI.skills.import();
      toast.success(t("skills.importSuccess"));
      await loadSkills();
      setSelectedSkillId(skill.id);
    } catch (error: any) {
      // Don't show error for cancel
      if (error.message !== "No folder selected") {
        toast.error(error.message || t("skills.importError"));
      }
    }
  };

  const handleDeleteSkill = async (e: React.MouseEvent, id: string) => {
    e.stopPropagation();
    try {
      if (pendingSaveRef.current?.skillId === id) {
        if (!(await flushPendingSave())) return;
      }
      await platformAPI.skills.delete(id);
      toast.success(t("skills.deleteSuccess"));
      if (selectedSkillId === id) {
        setSelectedSkillId(null);
        setSkillMdContent("");
      }
      loadSkills();
    } catch (error: any) {
      toast.error(error.message || t("skills.deleteError"));
    }
  };

  const handleOpenSkillsFolder = async () => {
    try {
      await platformAPI.skills.openFolder();
    } catch (error) {
      console.error("Failed to open folder:", error);
    }
  };

  const removeInstallation = async (installation: SkillInstallation) => {
    try {
      await platformAPI.skills.removeInstallation(installation.id);
      toast.success(t("skills.installationRemoved"));
      await loadSkills();
    } catch (error: any) {
      toast.error(error.message || t("skills.installationRemoveError"));
    }
  };

  const openInstallDialog = (target: AgentSkillTarget) => {
    setInstallTarget(target);
    setInstallProjectPath("");
    setInstallMode(target.mode);
  };

  const installSkillTarget = async () => {
    if (!selectedSkillId || !installTarget) return;
    if (installTarget.projectPathRequired && !installProjectPath.trim()) {
      setDialogError(t("skills.projectPathRequired"));
      return;
    }
    setIsInstalling(true);
    setDialogError(null);
    try {
      if (!(await flushPendingSave())) return;
      const installation = await platformAPI.skills.setInstallation({
        skillId: selectedSkillId,
        agentId: installTarget.agentId,
        targetId: installTarget.id,
        projectPath: installProjectPath.trim() || undefined,
        mode: installMode,
      });
      if (installation.status !== "synced") {
        toast.error(installation.error || t("skills.installationError"));
      } else {
        toast.success(t("skills.installationSuccess"));
        setInstallTarget(null);
      }
      await loadSkills();
    } catch (error: any) {
      toast.error(error.message || t("skills.installationError"));
    } finally {
      setIsInstalling(false);
    }
  };

  const browseProjectPath = async () => {
    const result = await platformAPI.servers.selectFile({
      title: t("skills.chooseProject"),
      mode: "directory",
    });
    if (result.success && result.path) setInstallProjectPath(result.path);
  };

  const selectedSkill =
    skills.find((skill) => skill.id === selectedSkillId) ?? null;

  const Shell = embedded ? EmbeddedSkillPage : PageLayout;

  if (isLoading) {
    return (
      <Shell
        title={t("skills.title")}
        contentClassName="flex items-center justify-center"
      >
        <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-primary" />
      </Shell>
    );
  }

  return (
    <Shell
      title={t("skills.title")}
      toolbar={
        <div className="flex min-w-0 flex-1 flex-wrap items-center justify-end gap-2">
          <div className="relative min-w-[180px] flex-1 max-w-md">
            <IconSearch className="absolute left-3 top-1/2 transform -translate-y-1/2 w-4 h-4 text-muted-foreground" />
            <Input
              placeholder={t("skills.searchPlaceholder")}
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              className="h-8 pl-10"
            />
          </div>
          {isTauriRuntime() && (
            <>
              <Button
                variant="outline"
                size="sm"
                className="h-8 gap-2"
                onClick={handleOpenSkillsFolder}
              >
                <IconFolderOpen className="w-4 h-4" />
                {t("skills.openFolder")}
              </Button>
              <Button
                variant="outline"
                size="sm"
                className="h-8 gap-2"
                onClick={handleImport}
              >
                <IconDownload className="w-4 h-4" />
                {t("skills.import")}
              </Button>
            </>
          )}
          <Button
            size="sm"
            className="h-8 gap-2"
            onClick={() => setIsNewDialogOpen(true)}
          >
            <IconPlus className="w-4 h-4" />
            {t("skills.new")}
          </Button>
          {toolbarEnd}
        </div>
      }
      contentClassName="flex flex-col overflow-hidden"
    >
      {/* Main Content */}
      <div className="flex flex-1 min-h-0 overflow-hidden rounded-md border">
        {/* Left: Skill List */}
        <div className="w-64 border-r overflow-y-auto">
          {filteredSkills.length === 0 ? (
            <div className="p-4 text-center text-muted-foreground">
              {searchQuery ? t("skills.noResults") : t("skills.empty")}
            </div>
          ) : (
            <div className="py-2">
              {filteredSkills.map((skill) => (
                <div
                  key={skill.id}
                  onClick={() => void handleSelectSkill(skill.id)}
                  className={`px-3 py-2 cursor-pointer hover:bg-muted/50 ${
                    selectedSkillId === skill.id ? "bg-muted" : ""
                  }`}
                >
                  <div className="flex items-center justify-between">
                    <span className="font-medium truncate flex-1">
                      {skill.name}
                    </span>
                    <div className="flex items-center gap-1 ml-2">
                      {skill.installations.length > 0 && (
                        <Badge
                          variant="secondary"
                          className="h-5 px-1.5 text-[10px]"
                        >
                          {
                            skill.installations.filter(
                              (item) => item.status === "synced",
                            ).length
                          }
                        </Badge>
                      )}
                      <Button
                        variant="ghost"
                        size="icon"
                        className="h-6 w-6 text-destructive hover:text-destructive"
                        onClick={(e) => handleDeleteSkill(e, skill.id)}
                      >
                        <IconTrash className="w-3.5 h-3.5" />
                      </Button>
                    </div>
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>

        {/* Right: Skill Content */}
        <div className="flex-1 flex flex-col overflow-hidden">
          {selectedSkillId && selectedSkill ? (
            <div className="flex min-h-0 flex-1 flex-col">
              <Textarea
                value={skillMdContent}
                onChange={handleContentChange}
                onBlur={() => void flushPendingSave()}
                className="min-h-[260px] flex-1 resize-none rounded-none border-0 p-4 font-mono text-sm focus-visible:ring-0"
                placeholder="# Skill Name..."
              />
              <div className="flex h-8 shrink-0 items-center justify-end border-t px-4 text-xs text-muted-foreground">
                {saveStatus === "saving" && t("skills.saving")}
                {saveStatus === "saved" && t("skills.saved")}
                {saveStatus === "error" && (
                  <span className="text-destructive">
                    {t("skills.saveError")}
                  </span>
                )}
              </div>
              <div className="max-h-[42%] min-h-[210px] overflow-y-auto border-t bg-muted/10 p-4">
                <div className="mb-3 flex items-center justify-between">
                  <div>
                    <h3 className="text-sm font-semibold">
                      {t("skills.agentInstallations")}
                    </h3>
                    <p className="text-xs text-muted-foreground">
                      {t("skills.agentInstallationsDescription")}
                    </p>
                  </div>
                  <Badge variant="outline">
                    {t("skills.installedCount", {
                      count: selectedSkill.installations.filter(
                        (item) => item.status === "synced",
                      ).length,
                    })}
                  </Badge>
                </div>
                <div className="grid gap-3 xl:grid-cols-2">
                  {skillTargets.map((target) => {
                    const targetInstallations =
                      selectedSkill.installations.filter(
                        (installation) =>
                          installation.agentId === target.agentId &&
                          installation.targetId === target.id,
                      );
                    return (
                      <div
                        key={`${target.agentId}:${target.id}`}
                        className="rounded-md border bg-background p-3"
                      >
                        <div className="flex items-start justify-between gap-3">
                          <div className="min-w-0">
                            <p className="text-sm font-medium">
                              {agentName(agents, target.agentId)} ·{" "}
                              {targetLabel(t, target)}
                            </p>
                            <p className="mt-0.5 break-all font-mono text-[11px] text-muted-foreground">
                              {target.resolvedPath || target.pathTemplate}
                            </p>
                          </div>
                          <Button
                            size="sm"
                            variant="outline"
                            className="h-7 shrink-0"
                            onClick={() => openInstallDialog(target)}
                          >
                            <IconLink className="h-3.5 w-3.5" />
                            {target.projectPathRequired
                              ? t("skills.installToProject")
                              : targetInstallations.length > 0
                                ? t("skills.reinstall")
                                : t("skills.install")}
                          </Button>
                        </div>
                        {targetInstallations.length > 0 ? (
                          <div className="mt-3 space-y-2">
                            {targetInstallations.map((installation) => (
                              <div
                                key={installation.id}
                                className="flex items-center justify-between gap-2 rounded border px-2 py-1.5 text-xs"
                              >
                                <div className="min-w-0">
                                  <div className="flex items-center gap-1.5">
                                    <Badge
                                      variant={
                                        installation.status === "synced"
                                          ? "default"
                                          : "destructive"
                                      }
                                      className="h-5 text-[10px]"
                                    >
                                      {t(
                                        `skills.installationStatus.${installation.status}`,
                                      )}
                                    </Badge>
                                    <span>
                                      {t(`skills.modes.${installation.mode}`)}
                                    </span>
                                  </div>
                                  <p className="mt-0.5 truncate font-mono text-muted-foreground">
                                    {installation.installedPath}
                                  </p>
                                  {installation.error && (
                                    <p className="mt-0.5 text-destructive">
                                      {installation.error}
                                    </p>
                                  )}
                                </div>
                                <Button
                                  size="icon"
                                  variant="ghost"
                                  className="h-7 w-7 shrink-0 text-destructive"
                                  onClick={() =>
                                    void removeInstallation(installation)
                                  }
                                >
                                  <IconTrash className="h-3.5 w-3.5" />
                                </Button>
                              </div>
                            ))}
                          </div>
                        ) : (
                          <p className="mt-3 text-xs text-muted-foreground">
                            {t("skills.notInstalledForTarget")}
                          </p>
                        )}
                      </div>
                    );
                  })}
                </div>
              </div>
            </div>
          ) : (
            <div className="flex items-center justify-center h-full text-muted-foreground">
              {t("skills.selectToEdit")}
            </div>
          )}
        </div>
      </div>

      {/* New Skill Dialog */}
      <Dialog open={isNewDialogOpen} onOpenChange={setIsNewDialogOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t("skills.newDialog.title")}</DialogTitle>
            <DialogDescription>
              {t("skills.newDialog.description")}
            </DialogDescription>
          </DialogHeader>
          <div className="grid gap-4 py-4">
            <div className="grid gap-2">
              <Label htmlFor="skill-name">{t("skills.name")}</Label>
              <Input
                id="skill-name"
                value={newSkillName}
                onChange={(e) => {
                  setNewSkillName(e.target.value);
                  setDialogError(null);
                }}
                placeholder={t("skills.namePlaceholder")}
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    handleCreateSkill();
                  }
                }}
              />
              <p className="text-xs text-muted-foreground">
                {t("skills.nameHint")}
              </p>
              {dialogError && (
                <p className="text-xs text-destructive">{dialogError}</p>
              )}
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={handleCloseNewDialog}>
              {t("common.cancel")}
            </Button>
            <Button onClick={handleCreateSkill}>{t("skills.create")}</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog
        open={Boolean(installTarget)}
        onOpenChange={(open) => {
          if (!open) {
            setInstallTarget(null);
            setDialogError(null);
          }
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t("skills.installDialogTitle")}</DialogTitle>
            <DialogDescription>
              {installTarget
                ? t("skills.installDialogDescription", {
                    agent: agentName(agents, installTarget.agentId),
                    target: targetLabel(t, installTarget),
                  })
                : ""}
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-4 py-2">
            {installTarget?.projectPathRequired && (
              <div className="space-y-2">
                <Label>{t("skills.projectPath")}</Label>
                <div className="flex gap-2">
                  <Input
                    value={installProjectPath}
                    onChange={(event) =>
                      setInstallProjectPath(event.target.value)
                    }
                    placeholder={t("skills.projectPathPlaceholder")}
                    className="font-mono text-xs"
                  />
                  {isTauriRuntime() && (
                    <Button
                      type="button"
                      variant="outline"
                      onClick={() => void browseProjectPath()}
                    >
                      <IconFolderOpen className="h-4 w-4" />
                      {t("skills.chooseProject")}
                    </Button>
                  )}
                </div>
              </div>
            )}
            <div className="space-y-2">
              <Label>{t("skills.installMode")}</Label>
              <Select
                value={installMode}
                onValueChange={(value) =>
                  setInstallMode(value as typeof installMode)
                }
              >
                <SelectTrigger>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {installTarget?.mode === "native" ? (
                    <SelectItem value="native">
                      {t("skills.modes.native")}
                    </SelectItem>
                  ) : (
                    <>
                      <SelectItem value="copy">
                        {t("skills.modes.copy")}
                      </SelectItem>
                      <SelectItem value="symlink">
                        {t("skills.modes.symlink")}
                      </SelectItem>
                    </>
                  )}
                </SelectContent>
              </Select>
              <p className="text-xs text-muted-foreground">
                {installMode === "symlink"
                  ? t("skills.symlinkDescription")
                  : installMode === "native"
                    ? t("skills.nativeDescription")
                    : t("skills.copyDescription")}
              </p>
            </div>
            {dialogError && (
              <p className="text-sm text-destructive">{dialogError}</p>
            )}
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setInstallTarget(null)}>
              {t("common.cancel")}
            </Button>
            <Button
              disabled={isInstalling}
              onClick={() => void installSkillTarget()}
            >
              {isInstalling ? t("common.loading") : t("skills.install")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </Shell>
  );
};

function agentName(agents: AgentPluginDescriptor[], id: string) {
  return agents.find((agent) => agent.id === id)?.name ?? id;
}

function targetLabel(
  t: ReturnType<typeof useTranslation>["t"],
  target: AgentSkillTarget,
) {
  const translatedTargets = new Set([
    "codex-project",
    "claude-global",
    "claude-project",
    "opencode-global",
    "opencode-project",
  ]);
  return translatedTargets.has(target.id)
    ? t(`agents.builtinTargets.${target.id}`)
    : target.label;
}

export default SkillsManager;
