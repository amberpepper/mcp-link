import React, { useEffect, useMemo, useRef, useState } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { useTranslation } from "react-i18next";
import {
  IconChevronDown,
  IconDots,
  IconFolder,
  IconLoader2,
  IconPencil,
  IconRefresh,
  IconSearch,
  IconTrash,
} from "@tabler/icons-react";
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
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
  SidebarMenuSub,
  SidebarMenuSubButton,
  SidebarMenuSubItem,
  Skeleton,
  Input,
  StatusDot,
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@mcp_link/ui";
import { useSessionStore } from "@/renderer/stores";
import { resolveAgentIconSource } from "@/renderer/components/agents/AgentAvatar";
import { cn } from "@/renderer/utils/tailwind-utils";
import type { AgentSessionSummary } from "@mcp_link/shared";
import { toast } from "sonner";
import {
  buildAgentGroups,
  formatSessionDate,
} from "@/renderer/components/sessions/session-utils";

const agentKey = (agentId: string) => `a:${agentId}`;
const workspaceKey = (agentId: string, groupKey: string) =>
  `w:${agentId}:${groupKey}`;

const SessionSidebarTree: React.FC = () => {
  const { t } = useTranslation();
  const location = useLocation();
  const navigate = useNavigate();
  const sessions = useSessionStore((s) => s.sessions);
  const selectedKey = useSessionStore((s) => s.selectedKey);
  const isLoading = useSessionStore((s) => s.isLoading);
  const plugins = useSessionStore((s) => s.plugins);
  const query = useSessionStore((s) => s.query);
  const selectSession = useSessionStore((s) => s.selectSession);
  const deleteSession = useSessionStore((s) => s.deleteSession);
  const renameSession = useSessionStore((s) => s.renameSession);
  const setQuery = useSessionStore((s) => s.setQuery);
  const loadPlugins = useSessionStore((s) => s.loadPlugins);
  const loadAgentSessions = useSessionStore((s) => s.loadAgentSessions);
  const loadedAgentIds = useSessionStore((s) => s.loadedAgentIds);
  const loadingAgentIds = useSessionStore((s) => s.loadingAgentIds);

  const agentNodes = useMemo(
    () => buildAgentGroups(sessions, plugins, t("sessions.noWorkspace")),
    [sessions, plugins, t],
  );

  const [openKeys, setOpenKeys] = useState<Set<string>>(() => new Set());
  const [isRefreshingSources, setIsRefreshingSources] = useState(false);
  const manuallyClosedKeysRef = useRef<Set<string>>(new Set());

  const refreshSources = async () => {
    if (isRefreshingSources) return;
    setIsRefreshingSources(true);
    try {
      await loadPlugins();
    } finally {
      setIsRefreshingSources(false);
    }
  };

  useEffect(() => {
    if (!selectedKey) return;
    for (const agent of agentNodes) {
      for (const group of agent.groups) {
        if (group.sessions.some((session) => session.id === selectedKey)) {
          const aKey = agentKey(agent.key);
          const wKey = workspaceKey(agent.key, group.key);
          setOpenKeys((prev) => {
            const shouldOpenAgent =
              !prev.has(aKey) && !manuallyClosedKeysRef.current.has(aKey);
            const shouldOpenWorkspace =
              !prev.has(wKey) && !manuallyClosedKeysRef.current.has(wKey);
            if (!shouldOpenAgent && !shouldOpenWorkspace) return prev;
            const next = new Set(prev);
            if (shouldOpenAgent) next.add(aKey);
            if (shouldOpenWorkspace) next.add(wKey);
            return next;
          });
          return;
        }
      }
    }
  }, [selectedKey, agentNodes]);

  const setKeyOpen = (key: string, open: boolean) => {
    if (open) manuallyClosedKeysRef.current.delete(key);
    else manuallyClosedKeysRef.current.add(key);

    setOpenKeys((prev) => {
      if (prev.has(key) === open) return prev;
      const next = new Set(prev);
      if (open) next.add(key);
      else next.delete(key);
      return next;
    });
  };

  const setAgentOpen = (agentId: string, open: boolean) => {
    const key = agentKey(agentId);
    setKeyOpen(key, open);
    if (open && !loadedAgentIds.includes(agentId)) {
      void loadAgentSessions(agentId, true);
    }
  };

  const handleSelect = (session: AgentSessionSummary) => {
    if (location.pathname !== "/sessions") navigate("/sessions");
    void selectSession(session);
  };

  // Per-session rename / delete
  const [renameTarget, setRenameTarget] = useState<AgentSessionSummary | null>(
    null,
  );
  const [draftTitle, setDraftTitle] = useState("");
  const [deleteTarget, setDeleteTarget] = useState<AgentSessionSummary | null>(
    null,
  );

  const capabilitiesOf = (agentId: string) => {
    const plugin = plugins.find((item) => item.id === agentId);
    return {
      canRename: plugin?.capabilities.includes("sessions.rename") ?? false,
      canDelete: plugin?.capabilities.includes("sessions.delete") ?? false,
    };
  };

  const startRename = (session: AgentSessionSummary) => {
    setDraftTitle(session.title);
    setRenameTarget(session);
  };
  const commitRename = async () => {
    const session = renameTarget;
    if (!session) return;
    const title = draftTitle.trim();
    setRenameTarget(null);
    if (!title || title === session.title) return;
    try {
      await renameSession(session, title);
      toast.success(t("sessions.renameSuccess"));
    } catch (error) {
      toast.error(error instanceof Error ? error.message : t("common.error"));
    }
  };

  const confirmDelete = async () => {
    const target = deleteTarget;
    setDeleteTarget(null);
    if (!target) return;
    try {
      const result = await deleteSession(target);
      toast.success(
        result.backupPath
          ? t("sessions.deleteSuccessWithBackup", { path: result.backupPath })
          : t("sessions.deleteSuccess"),
      );
    } catch (error) {
      toast.error(error instanceof Error ? error.message : t("common.error"));
    }
  };

  const renderSession = (session: AgentSessionSummary, agentId: string) => {
    const isActive = selectedKey === session.id;
    const { canRename, canDelete } = capabilitiesOf(agentId);
    const hasActions = canRename || canDelete;
    return (
      <SidebarMenuSubItem key={session.id} className="group/session relative">
        <Tooltip>
          <TooltipTrigger asChild>
            <SidebarMenuSubButton
              isActive={isActive}
              className="cursor-pointer pr-7"
              onClick={() => handleSelect(session)}
            >
              <StatusDot
                tone={isActive || session.active ? "running" : "stopped"}
                aria-hidden="true"
              />
              <span className="min-w-0 flex-1 truncate">{session.title}</span>
              <span className="shrink-0 text-[10px] tabular-nums text-muted-foreground">
                {formatSessionDate(session.updatedAt ?? session.createdAt)}
              </span>
            </SidebarMenuSubButton>
          </TooltipTrigger>
          <TooltipContent>{session.title}</TooltipContent>
        </Tooltip>
        {hasActions && (
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <button
                type="button"
                aria-label={t("sessions.moreActions")}
                className="absolute right-1 top-1/2 z-10 flex h-5 w-5 -translate-y-1/2 items-center justify-center rounded opacity-0 transition-opacity hover:bg-accent group-hover/session:opacity-100 focus-visible:opacity-100"
              >
                <IconDots className="h-3.5 w-3.5" />
              </button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="w-40">
              {canRename && (
                <DropdownMenuItem onClick={() => startRename(session)}>
                  <IconPencil className="h-4 w-4" />
                  {t("sessions.rename")}
                </DropdownMenuItem>
              )}
              {canDelete && (
                <DropdownMenuItem
                  className="text-destructive focus:text-destructive"
                  onClick={() => setDeleteTarget(session)}
                >
                  <IconTrash className="h-4 w-4" />
                  {t("common.delete")}
                </DropdownMenuItem>
              )}
            </DropdownMenuContent>
          </DropdownMenu>
        )}
      </SidebarMenuSubItem>
    );
  };

  return (
    <>
      <Dialog
        open={Boolean(renameTarget)}
        onOpenChange={(open) => !open && setRenameTarget(null)}
      >
        <DialogContent className="max-w-md">
          <DialogHeader>
            <DialogTitle>{t("sessions.renameTitle")}</DialogTitle>
          </DialogHeader>
          <Input
            autoFocus
            value={draftTitle}
            onChange={(event) => setDraftTitle(event.target.value)}
            onKeyDown={(event) => event.key === "Enter" && void commitRename()}
          />
          <DialogFooter>
            <Button variant="outline" onClick={() => setRenameTarget(null)}>
              {t("common.cancel")}
            </Button>
            <Button
              disabled={!draftTitle.trim()}
              onClick={() => void commitRename()}
            >
              {t("common.save")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
      {/* Search + refresh */}
      <div className="group-data-[collapsible=icon]:hidden space-y-1.5 px-1 pb-1.5 pt-2">
        <div className="flex gap-1.5">
          <div className="relative min-w-0 flex-1">
            <input
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder={t("sessions.searchPlaceholder")}
              className="h-8 w-full rounded-md border border-border bg-background pl-7 pr-2 text-xs focus:outline-none focus:ring-1 focus:ring-primary"
            />
            <IconSearch className="absolute left-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground" />
          </div>
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                variant="outline"
                size="icon"
                className="h-8 w-8 shrink-0"
                aria-label={t("common.refresh")}
                disabled={isRefreshingSources}
                onClick={() => void refreshSources()}
              >
                <IconRefresh
                  className={
                    isRefreshingSources
                      ? "h-3.5 w-3.5 animate-spin"
                      : "h-3.5 w-3.5"
                  }
                />
              </Button>
            </TooltipTrigger>
            <TooltipContent>{t("common.refresh")}</TooltipContent>
          </Tooltip>
        </div>
      </div>

      {/* Tree: agent -> workspace -> session */}
      <SidebarMenuSub className="ml-3 mr-0 min-w-0 max-w-full pr-0">
        {isLoading && plugins.length === 0 ? (
          Array.from({ length: 4 }).map((_, index) => (
            <SidebarMenuSubItem key={index}>
              <Skeleton className="h-6 w-full rounded-md" />
            </SidebarMenuSubItem>
          ))
        ) : agentNodes.length === 0 ? (
          <p className="px-2 py-1 text-xs text-muted-foreground">
            {t("sessions.empty")}
          </p>
        ) : (
          agentNodes.map((agent) => {
            const aOpen = openKeys.has(agentKey(agent.key));
            const agentLoading = loadingAgentIds.includes(agent.key);
            const iconSource = resolveAgentIconSource(agent.plugin.icon);
            const total = agent.groups.reduce(
              (sum, group) => sum + group.sessions.length,
              0,
            );
            return (
              <SidebarMenuSubItem key={agent.key}>
                <Collapsible
                  open={aOpen}
                  onOpenChange={(open) => setAgentOpen(agent.key, open)}
                >
                  <CollapsibleTrigger asChild>
                    <SidebarMenuSubButton className="cursor-pointer">
                      {agentLoading ? (
                        <IconLoader2 className="h-3.5 w-3.5 shrink-0 animate-spin text-muted-foreground" />
                      ) : (
                        <IconChevronDown
                          className={cn(
                            "h-3.5 w-3.5 shrink-0 text-muted-foreground transition-transform",
                            !aOpen && "-rotate-90",
                          )}
                        />
                      )}
                      {iconSource ? (
                        <img
                          src={iconSource}
                          className="h-3.5 w-3.5 shrink-0 rounded-sm object-contain"
                          alt=""
                        />
                      ) : (
                        <IconFolder className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
                      )}
                      <span className="min-w-0 flex-1 truncate font-medium">
                        {agent.plugin.name}
                      </span>
                      <Badge
                        variant="secondary"
                        className="shrink-0 px-1.5 py-0 text-[10px] tabular-nums"
                      >
                        {total}
                      </Badge>
                    </SidebarMenuSubButton>
                  </CollapsibleTrigger>
                  <CollapsibleContent>
                    <SidebarMenuSub className="ml-3 mr-0 min-w-0 max-w-full pr-0">
                      {agent.groups.length === 0 && !agentLoading ? (
                        <p className="px-2 py-1 text-xs text-muted-foreground">
                          {t("sessions.empty")}
                        </p>
                      ) : (
                        agent.groups.map((group) => {
                          const gOpen = openKeys.has(
                            workspaceKey(agent.key, group.key),
                          );
                          return (
                            <SidebarMenuSubItem key={group.key}>
                              <Collapsible
                                open={gOpen}
                                onOpenChange={(open) =>
                                  setKeyOpen(
                                    workspaceKey(agent.key, group.key),
                                    open,
                                  )
                                }
                              >
                                <Tooltip>
                                  <TooltipTrigger asChild>
                                    <CollapsibleTrigger asChild>
                                      <SidebarMenuSubButton className="cursor-pointer">
                                        <IconChevronDown
                                          className={cn(
                                            "h-3.5 w-3.5 shrink-0 text-muted-foreground transition-transform",
                                            !gOpen && "-rotate-90",
                                          )}
                                        />
                                        <IconFolder className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
                                        <span className="min-w-0 flex-1 truncate font-medium">
                                          {group.label}
                                        </span>
                                        <Badge
                                          variant="secondary"
                                          className="shrink-0 px-1.5 py-0 text-[10px] tabular-nums"
                                        >
                                          {group.sessions.length}
                                        </Badge>
                                      </SidebarMenuSubButton>
                                    </CollapsibleTrigger>
                                  </TooltipTrigger>
                                  {group.path && (
                                    <TooltipContent>
                                      {group.path}
                                    </TooltipContent>
                                  )}
                                </Tooltip>
                                <CollapsibleContent>
                                  <SidebarMenuSub className="ml-3 mr-0 min-w-0 max-w-full pr-0">
                                    {group.sessions.map((session) =>
                                      renderSession(session, agent.key),
                                    )}
                                  </SidebarMenuSub>
                                </CollapsibleContent>
                              </Collapsible>
                            </SidebarMenuSubItem>
                          );
                        })
                      )}
                    </SidebarMenuSub>
                  </CollapsibleContent>
                </Collapsible>
              </SidebarMenuSubItem>
            );
          })
        )}
      </SidebarMenuSub>

      <AlertDialog
        open={Boolean(deleteTarget)}
        onOpenChange={(open) => {
          if (!open) setDeleteTarget(null);
        }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("sessions.deleteTitle")}</AlertDialogTitle>
            <AlertDialogDescription>
              {t("sessions.deleteConfirmDescription")}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("common.cancel")}</AlertDialogCancel>
            <AlertDialogAction onClick={() => void confirmDelete()}>
              {t("common.delete")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
};

export default SessionSidebarTree;
