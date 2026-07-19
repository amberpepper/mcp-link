import React, {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
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
  Label,
  ScrollArea,
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
  IconArrowsExchange,
  IconChartBar,
  IconCopy,
  IconDownload,
  IconDots,
  IconFolder,
  IconLoader2,
  IconMessages,
  IconPlayerPlay,
  IconTrash,
} from "@tabler/icons-react";
import type {
  AgentImportTarget,
  SessionExportFormat,
  SessionExportOptions,
  SessionStats,
  UserMessageNavItem,
} from "@mcp_link/shared";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";

import PageLayout from "@/renderer/components/layout/PageLayout";
import MessageGroup from "./SessionMessageGroup";
import UserMessageNavigator from "./UserMessageNavigator";
import {
  combinedSourceName,
  downloadResult,
  errorMessage,
  groupVisibleMessages,
  isInternalContextText,
  isToolMessage,
  sessionSourceName,
  sliceMessageGroups,
  warningMessage,
} from "./session-utils";
import { usePlatformAPI } from "@/renderer/platform-api";
import { isTauriRuntime } from "@/renderer/platform-api/tauri-platform-api";
import { useSessionStore } from "@/renderer/stores";

const MESSAGE_PAGE_SIZE = 50;

function compactNumber(value: number): string {
  return new Intl.NumberFormat(undefined, {
    notation: "compact",
    maximumFractionDigits: 1,
  }).format(value);
}

function exactNumber(value: number): string {
  return new Intl.NumberFormat(undefined, {
    maximumFractionDigits: 8,
  }).format(value);
}

function compactCost(value: number): string {
  return `$${new Intl.NumberFormat(undefined, {
    minimumFractionDigits: 2,
    maximumFractionDigits: value > 0 && value < 0.01 ? 4 : 2,
  }).format(value)}`;
}

const SessionStatsSummary: React.FC<{ stats: SessionStats }> = ({ stats }) => {
  const { t } = useTranslation();
  const entries = [
    ["total", t("sessions.stats.total"), stats.totalTokens, false],
    ["input", t("sessions.stats.input"), stats.inputTokens, false],
    ["output", t("sessions.stats.output"), stats.outputTokens, false],
    ["cached", t("sessions.stats.cached"), stats.cachedInputTokens, false],
    [
      "cacheWrite",
      t("sessions.stats.cacheWrite"),
      stats.cacheWriteTokens,
      false,
    ],
    ["reasoning", t("sessions.stats.reasoning"), stats.reasoningTokens, false],
    ["cost", t("sessions.stats.cost"), stats.cost, true],
    ["context", t("sessions.stats.context"), stats.contextWindow, false],
  ].filter((entry): entry is [string, string, number, boolean] =>
    Number.isFinite(entry[2]),
  );

  if (entries.length === 0) return null;
  return (
    <div className="mt-2 flex min-w-0 flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground">
      <IconChartBar className="h-3.5 w-3.5 shrink-0" aria-hidden="true" />
      {entries.map(([key, label, value, cost], index) => (
        <Tooltip key={key}>
          <TooltipTrigger asChild>
            <span
              className={`inline-flex items-baseline gap-1 tabular-nums ${
                index > 0 ? "border-l pl-3" : ""
              }`}
            >
              <span>{label}</span>
              <span
                className={key === "total" ? "font-medium text-foreground" : ""}
              >
                {cost ? compactCost(value) : compactNumber(value)}
              </span>
            </span>
          </TooltipTrigger>
          <TooltipContent>
            {label}: {cost ? `$${exactNumber(value)}` : exactNumber(value)}
          </TooltipContent>
        </Tooltip>
      ))}
    </div>
  );
};

const SessionStatsSkeleton: React.FC = () => {
  const { t } = useTranslation();
  const widths = ["w-16", "w-12", "w-12", "w-16", "w-16", "w-12"];

  return (
    <div
      className="mt-2 flex min-w-0 animate-pulse flex-wrap items-center gap-x-3 gap-y-1"
      role="status"
      aria-label={t("common.loading")}
    >
      <IconChartBar
        className="h-3.5 w-3.5 shrink-0 text-muted-foreground/40"
        aria-hidden="true"
      />
      {widths.map((width, index) => (
        <span
          key={`${width}-${index}`}
          className={`inline-flex h-4 items-center ${
            index > 0 ? "border-l pl-3" : ""
          }`}
          aria-hidden="true"
        >
          <span className={`h-3 rounded-sm bg-muted ${width}`} />
        </span>
      ))}
    </div>
  );
};

const SessionsPage: React.FC = () => {
  const { t } = useTranslation();
  const platformAPI = usePlatformAPI();
  const isDesktopRuntime = isTauriRuntime();
  const {
    plugins,
    selected,
    isLoadingSession,
    error: sessionError,
    refreshSessions,
    loadOlderMessages: loadOlderSessionMessages,
    loadMessagesThrough,
    clearSelected,
    clearError,
  } = useSessionStore();
  const [visibleMessageStart, setVisibleMessageStart] = useState(0);
  const [isLoadingOlderMessages, setIsLoadingOlderMessages] = useState(false);
  const [conversationOnly, setConversationOnly] = useState(false);
  const [busyAction, setBusyAction] = useState<string | null>(null);
  const [messageIndex, setMessageIndex] = useState<{
    sessionId: string;
    items: UserMessageNavItem[];
  }>({ sessionId: "", items: [] });
  const [statsState, setStatsState] = useState<{
    sessionId: string;
    value: SessionStats | null;
    loading: boolean;
  }>({ sessionId: "", value: null, loading: false });
  const [deleteOpen, setDeleteOpen] = useState(false);
  const [convertOpen, setConvertOpen] = useState(false);
  const [exportOpen, setExportOpen] = useState(false);
  const [targetValue, setTargetValue] = useState("");
  const [exportOptions, setExportOptions] = useState<SessionExportOptions>({
    format: "html",
    includeReasoning: true,
    includeToolCalls: true,
    includeToolResults: true,
    sanitize: true,
  });
  const sessionScrollAreaRef = useRef<HTMLDivElement>(null);
  const initialScrollPendingRef = useRef(false);
  const loadingOlderMessagesRef = useRef(false);
  const loadingOlderMessagesTimerRef = useRef<number | null>(null);
  const resizingToolGroupTimerRef = useRef<number | null>(null);
  const pendingMessageJumpRef = useRef<string | null>(null);
  const conversationOnlyRef = useRef(false);
  conversationOnlyRef.current = conversationOnly;

  // Surface store errors as toasts.
  useEffect(() => {
    if (!sessionError) return;
    toast.error(sessionError);
    clearError();
  }, [sessionError, clearError]);

  // Reset message pagination whenever the selected session changes.
  useLayoutEffect(() => {
    if (!selected) return;
    loadingOlderMessagesRef.current = false;
    if (loadingOlderMessagesTimerRef.current !== null) {
      window.clearTimeout(loadingOlderMessagesTimerRef.current);
      loadingOlderMessagesTimerRef.current = null;
    }
    setIsLoadingOlderMessages(false);
    const count = conversationOnlyRef.current
      ? selected.messages.filter((item) => !isToolMessage(item)).length
      : selected.messages.length;
    setVisibleMessageStart(Math.max(0, count - MESSAGE_PAGE_SIZE));
  }, [selected?.id]);

  useEffect(
    () => () => {
      if (loadingOlderMessagesTimerRef.current !== null) {
        window.clearTimeout(loadingOlderMessagesTimerRef.current);
      }
      if (resizingToolGroupTimerRef.current !== null) {
        window.clearTimeout(resizingToolGroupTimerRef.current);
      }
    },
    [],
  );

  const runAction = async (name: string, action: () => Promise<unknown>) => {
    setBusyAction(name);
    try {
      await action();
    } catch (error) {
      toast.error(errorMessage(error, t("common.error")));
      throw error;
    } finally {
      setBusyAction(null);
    }
  };

  const resumeSession = async () => {
    if (!selected) return;
    await runAction("resume", async () => {
      await platformAPI.agents.sessions.resume(
        selected.agentId,
        selected.nativeId,
      );
      toast.success(t("sessions.resumeSuccess"));
    });
  };

  const duplicateSession = async (untilMessage?: number) => {
    if (!selected) return;
    await runAction("duplicate", async () => {
      const result = await platformAPI.agents.sessions.duplicate(
        selected.agentId,
        selected.nativeId,
        untilMessage,
      );
      toast.success(
        untilMessage
          ? t("sessions.branchSuccess")
          : t("sessions.duplicateSuccess"),
      );
      if (result.warnings?.length) {
        toast.info(
          result.warnings
            .map((warning) => warningMessage(t, warning))
            .join("\n"),
        );
      }
      await refreshSessions();
    });
  };

  const deleteSession = async () => {
    if (!selected) return;
    await runAction("delete", async () => {
      const result = await platformAPI.agents.sessions.delete(
        selected.agentId,
        selected.nativeId,
      );
      setDeleteOpen(false);
      clearSelected();
      toast.success(
        result.backupPath
          ? t("sessions.deleteSuccessWithBackup", {
              path: result.backupPath,
            })
          : t("sessions.deleteSuccess"),
      );
      await refreshSessions();
    });
  };

  const convertSession = async () => {
    if (!selected || !targetValue) return;
    const target = importTargets.find((item) => item.value === targetValue);
    if (!target) return;
    await runAction("convert", async () => {
      const result = await platformAPI.agents.sessions.importToAgent(
        selected.agentId,
        selected.nativeId,
        {
          targetAgentId: target.agentId,
          targetInstanceId: target.instanceId,
          title: selected.title,
          cwd: selected.cwd ?? undefined,
          openAfterImport: true,
        },
      );
      setConvertOpen(false);
      toast.success(
        t("sessions.convertSuccess", {
          agent: target.label,
          id: result.nativeId,
        }),
      );
      if (result.warnings?.length) {
        toast.info(
          result.warnings
            .map((warning) => warningMessage(t, warning))
            .join("\n"),
        );
      }
      await refreshSessions();
    });
  };

  const exportSession = async () => {
    if (!selected) return;
    await runAction("export", async () => {
      if (isDesktopRuntime) {
        const result = await platformAPI.agents.sessions.exportToFile(
          selected.agentId,
          selected.nativeId,
          exportOptions,
        );
        if (!result.saved) return;
        setExportOpen(false);
        toast.success(
          t("sessions.exportSuccessPath", {
            path: result.path ?? result.fileName,
          }),
        );
        return;
      }
      const result = await platformAPI.agents.sessions.export(
        selected.agentId,
        selected.nativeId,
        exportOptions,
      );
      downloadResult(
        result.fileName,
        result.mimeType,
        result.content,
        result.encoding,
      );
      setExportOpen(false);
      toast.success(t("sessions.exportSuccess", { file: result.fileName }));
    });
  };

  const importTargets = useMemo<AgentImportTarget[]>(
    () =>
      plugins.flatMap<AgentImportTarget>((plugin): AgentImportTarget[] => {
        if (
          !plugin.enabled ||
          !plugin.capabilities.includes("sessions.import")
        ) {
          return [];
        }
        return plugin.instances
          .filter(
            (instance) =>
              instance.enabled &&
              !(
                selected?.agentId === plugin.id &&
                selected.sourceInstanceId === instance.id
              ),
          )
          .map((instance) => ({
            value: `${plugin.id}::${instance.id}`,
            agentId: plugin.id,
            instanceId: instance.id,
            label: combinedSourceName(plugin.name, instance.label),
          }));
      }),
    [plugins, selected],
  );
  const selectedPlugin = useMemo(
    () => plugins.find((plugin) => plugin.id === selected?.agentId) ?? null,
    [plugins, selected?.agentId],
  );
  const canResume =
    selectedPlugin?.capabilities.includes("sessions.resume") === true;
  const canDuplicate =
    selectedPlugin?.capabilities.includes("sessions.duplicate") === true;
  const canBranch =
    selectedPlugin?.capabilities.includes("sessions.branch") === true;
  const canDelete =
    selectedPlugin?.capabilities.includes("sessions.delete") === true;
  const canExportNative =
    selectedPlugin?.capabilities.includes("sessions.export-native") === true;
  const canLoadStats =
    selectedPlugin?.capabilities.includes("sessions.stats") === true;
  const sessionStats =
    selected && statsState.sessionId === selected.id ? statsState.value : null;
  const isLoadingStats = Boolean(
    selected &&
    canLoadStats &&
    (statsState.sessionId !== selected.id || statsState.loading),
  );

  useEffect(() => {
    if (!selected || !canLoadStats) {
      setStatsState({
        sessionId: selected?.id ?? "",
        value: null,
        loading: false,
      });
      return;
    }
    const sessionId = selected.id;
    let cancelled = false;
    setStatsState({ sessionId, value: null, loading: true });
    void platformAPI.agents.sessions
      .getStats(selected.agentId, selected.nativeId)
      .then((value) => {
        if (!cancelled) setStatsState({ sessionId, value, loading: false });
      })
      .catch(() => {
        if (!cancelled) {
          setStatsState({ sessionId, value: null, loading: false });
        }
      });
    return () => {
      cancelled = true;
    };
  }, [
    canLoadStats,
    platformAPI,
    selected?.agentId,
    selected?.id,
    selected?.nativeId,
  ]);
  const displayMessages = useMemo(
    () =>
      selected?.messages
        .map((item, originalIndex) => ({ item, originalIndex }))
        .filter(
          ({ item }) =>
            !(item.text && isInternalContextText(item.text)) &&
            (!conversationOnly || !isToolMessage(item)),
        ) ?? [],
    [conversationOnly, selected],
  );
  const loadedUserMessageNavItems = useMemo<UserMessageNavItem[]>(
    () =>
      selected?.messages.flatMap((item, originalIndex) => {
        const normalizedText = item.text?.replace(/\s+/g, " ").trim();
        if (
          item.role !== "user" ||
          item.kind !== "text" ||
          !normalizedText ||
          isInternalContextText(normalizedText)
        ) {
          return [];
        }
        const text =
          normalizedText.length > 240
            ? `${normalizedText.slice(0, 240)}...`
            : normalizedText;
        return [{ messageId: item.id, originalIndex, text }];
      }) ?? [],
    [selected],
  );
  const userMessageNavItems =
    selected && messageIndex.sessionId === selected.id
      ? messageIndex.items
      : loadedUserMessageNavItems;

  useEffect(() => {
    if (!selected) return;
    const sessionId = selected.id;
    const agentId = selected.agentId;
    const nativeId = selected.nativeId;
    let cancelled = false;
    void platformAPI.agents.sessions
      .listUserMessages(agentId, nativeId)
      .then((items) => {
        if (!cancelled) setMessageIndex({ sessionId, items });
      })
      .catch((error) => {
        if (!cancelled) {
          toast.error(errorMessage(error, t("common.error")));
        }
      });
    return () => {
      cancelled = true;
    };
  }, [platformAPI, selected?.agentId, selected?.id, selected?.nativeId, t]);

  const userMessageIdSet = useMemo(
    () => new Set(userMessageNavItems.map((item) => item.messageId)),
    [userMessageNavItems],
  );
  const messageGroups = useMemo(
    () => groupVisibleMessages(displayMessages),
    [displayMessages],
  );
  const visibleMessageGroups = useMemo(
    () => sliceMessageGroups(messageGroups, visibleMessageStart),
    [messageGroups, visibleMessageStart],
  );
  const getMessageScrollElement = useCallback(
    () =>
      sessionScrollAreaRef.current?.querySelector<HTMLElement>(
        "[data-radix-scroll-area-viewport]",
      ) ?? null,
    [],
  );
  const messageVirtualizer = useVirtualizer({
    count: visibleMessageGroups.length,
    getScrollElement: getMessageScrollElement,
    estimateSize: () => 220,
    getItemKey: (index) => {
      return visibleMessageGroups[index]?.key ?? index;
    },
    overscan: 4,
  });
  const handleToolGroupResize = useCallback(
    (groupKey: string) => {
      if (resizingToolGroupTimerRef.current !== null) {
        window.clearTimeout(resizingToolGroupTimerRef.current);
      }
      messageVirtualizer.shouldAdjustScrollPositionOnItemSizeChange = (
        item,
        _delta,
        instance,
      ) =>
        item.key === groupKey
          ? false
          : item.start < (instance.scrollOffset ?? 0);
      resizingToolGroupTimerRef.current = window.setTimeout(() => {
        messageVirtualizer.shouldAdjustScrollPositionOnItemSizeChange =
          undefined;
        resizingToolGroupTimerRef.current = null;
      }, 250);
    },
    [messageVirtualizer],
  );
  const branchActionRef = useRef(duplicateSession);
  branchActionRef.current = duplicateSession;
  const branchFromMessage = useCallback((originalIndex: number) => {
    void branchActionRef.current(originalIndex + 1);
  }, []);

  useLayoutEffect(() => {
    const messageId = pendingMessageJumpRef.current;
    if (messageId == null) return;
    const groupIndex = visibleMessageGroups.findIndex((group) =>
      group.messages.some((message) => message.item.id === messageId),
    );
    if (groupIndex < 0) return;

    pendingMessageJumpRef.current = null;
    messageVirtualizer.scrollToIndex(groupIndex, { align: "start" });
    const frame = window.requestAnimationFrame(() => {
      document
        .getElementById(`session-message-${messageId}`)
        ?.scrollIntoView({ block: "start" });
    });
    return () => window.cancelAnimationFrame(frame);
  }, [messageVirtualizer, visibleMessageGroups]);

  const handleConversationOnlyChange = (checked: boolean) => {
    setConversationOnly(checked);
    if (!selected) return;
    const count = checked
      ? selected.messages.filter((item) => !isToolMessage(item)).length
      : selected.messages.length;
    setVisibleMessageStart(Math.max(0, count - MESSAGE_PAGE_SIZE));
  };

  const jumpToUserMessage = useCallback(
    async (messageId: string) => {
      const displayIndex = displayMessages.findIndex(
        (message) => message.item.id === messageId,
      );
      if (displayIndex < 0) {
        if (loadingOlderMessagesRef.current) return;
        pendingMessageJumpRef.current = messageId;
        loadingOlderMessagesRef.current = true;
        setIsLoadingOlderMessages(true);
        const found = await loadMessagesThrough(messageId);
        if (!found) pendingMessageJumpRef.current = null;
        else setVisibleMessageStart(0);
        loadingOlderMessagesRef.current = false;
        setIsLoadingOlderMessages(false);
        return;
      }
      if (displayIndex >= visibleMessageStart) {
        const groupIndex = visibleMessageGroups.findIndex((group) =>
          group.messages.some((message) => message.item.id === messageId),
        );
        if (groupIndex >= 0) {
          messageVirtualizer.scrollToIndex(groupIndex, { align: "start" });
          window.requestAnimationFrame(() => {
            document
              .getElementById(`session-message-${messageId}`)
              ?.scrollIntoView({ block: "start" });
          });
        }
        return;
      }
      pendingMessageJumpRef.current = messageId;
      setVisibleMessageStart(displayIndex);
    },
    [
      displayMessages,
      loadMessagesThrough,
      messageVirtualizer,
      visibleMessageGroups,
      visibleMessageStart,
    ],
  );

  useLayoutEffect(() => {
    if (!selected || isLoadingSession) return;
    const viewport = sessionScrollAreaRef.current?.querySelector<HTMLElement>(
      "[data-radix-scroll-area-viewport]",
    );
    if (!viewport) return;
    initialScrollPendingRef.current = true;
    let frame = 0;
    let attempts = 0;
    let stableFrames = 0;
    let previousHeight = -1;
    const settleAtLatest = () => {
      viewport.scrollTop = viewport.scrollHeight;
      const height = viewport.scrollHeight;
      stableFrames = height === previousHeight ? stableFrames + 1 : 0;
      previousHeight = height;
      attempts += 1;
      if ((attempts >= 12 && stableFrames >= 4) || attempts >= 60) {
        initialScrollPendingRef.current = false;
        return;
      }
      frame = window.requestAnimationFrame(settleAtLatest);
    };
    frame = window.requestAnimationFrame(settleAtLatest);
    return () => {
      window.cancelAnimationFrame(frame);
    };
  }, [conversationOnly, isLoadingSession, selected?.id]);

  useEffect(() => {
    if (!selected || isLoadingSession) return;
    const viewport = sessionScrollAreaRef.current?.querySelector<HTMLElement>(
      "[data-radix-scroll-area-viewport]",
    );
    if (!viewport) return;

    const loadOlderMessages = async () => {
      if (
        initialScrollPendingRef.current ||
        viewport.scrollTop > 80 ||
        (visibleMessageStart === 0 && !selected.hasMoreMessages) ||
        loadingOlderMessagesRef.current
      ) {
        return;
      }
      loadingOlderMessagesRef.current = true;
      setIsLoadingOlderMessages(true);
      const previousHeight = viewport.scrollHeight;
      const previousTop = viewport.scrollTop;
      if (visibleMessageStart > 0) {
        setVisibleMessageStart((current) =>
          Math.max(0, current - MESSAGE_PAGE_SIZE),
        );
      } else {
        await loadOlderSessionMessages();
      }
      window.requestAnimationFrame(() => {
        window.requestAnimationFrame(() => {
          viewport.scrollTop =
            previousTop + (viewport.scrollHeight - previousHeight);
          loadingOlderMessagesTimerRef.current = window.setTimeout(() => {
            loadingOlderMessagesRef.current = false;
            loadingOlderMessagesTimerRef.current = null;
            setIsLoadingOlderMessages(false);
          }, 300);
        });
      });
    };

    const handleScroll = () => void loadOlderMessages();
    viewport.addEventListener("scroll", handleScroll, { passive: true });
    return () => viewport.removeEventListener("scroll", handleScroll);
  }, [
    isLoadingSession,
    loadOlderSessionMessages,
    selected?.hasMoreMessages,
    selected?.id,
    visibleMessageStart,
  ]);

  useEffect(() => {
    if (!canExportNative && exportOptions.format === "native") {
      setExportOptions((current) => ({ ...current, format: "html" }));
    }
  }, [canExportNative, exportOptions.format]);

  const openConvert = () => {
    const first = importTargets[0]?.value ?? "";
    setTargetValue(first);
    setConvertOpen(true);
  };

  return (
    <PageLayout contentClassName="min-h-0 overflow-hidden p-0">
      <div className="flex h-full min-h-0 flex-col overflow-hidden">
        {!selected ? (
          <div className="flex flex-1 flex-col items-center justify-center p-8 text-center text-muted-foreground">
            <IconMessages className="mb-3 h-12 w-12" />
            <p>{t("sessions.selectSession")}</p>
          </div>
        ) : (
          <>
            <div className="shrink-0 border-b px-5 py-4">
              <div className="flex flex-col gap-4">
                <div className="min-w-0">
                  <div className="flex min-w-0 items-start">
                    <h2 className="line-clamp-2 break-all text-xl font-semibold leading-7">
                      {selected.title}
                    </h2>
                  </div>
                  <div className="mt-2 flex min-w-0 flex-wrap items-center gap-2 text-xs text-muted-foreground">
                    <Badge variant="secondary" className="font-normal">
                      {sessionSourceName(plugins, selected)}
                    </Badge>
                    <Tooltip>
                      <TooltipTrigger asChild>
                        <span className="flex min-w-0 max-w-full items-center gap-1.5">
                          <IconFolder className="h-3.5 w-3.5 shrink-0" />
                          <span className="truncate">
                            {selected.cwd || t("sessions.noWorkspace")}
                          </span>
                        </span>
                      </TooltipTrigger>
                      {selected.cwd && (
                        <TooltipContent>{selected.cwd}</TooltipContent>
                      )}
                    </Tooltip>
                    <Badge
                      variant="outline"
                      className="max-w-full truncate font-mono text-[11px] font-normal"
                    >
                      ID:{" "}
                      {typeof selected.rawMetadata?.sessionId === "string"
                        ? selected.rawMetadata.sessionId
                        : (selected.nativeSessionId ?? selected.nativeId)}
                    </Badge>
                    <label className="flex h-8 items-center gap-2 text-xs text-foreground/80">
                      <span>{t("sessions.conversationOnly")}</span>
                      <Switch
                        checked={conversationOnly}
                        onCheckedChange={handleConversationOnlyChange}
                      />
                    </label>
                    <Button
                      size="sm"
                      disabled={Boolean(busyAction) || !canResume}
                      onClick={() => void resumeSession()}
                    >
                      <IconPlayerPlay className="h-4 w-4" />
                      {t("sessions.resume")}
                    </Button>
                    <DropdownMenu>
                      <Tooltip>
                        <TooltipTrigger asChild>
                          <DropdownMenuTrigger asChild>
                            <Button
                              size="icon"
                              variant="outline"
                              className="h-8 w-8 shrink-0"
                              disabled={Boolean(busyAction)}
                              aria-label={t("sessions.moreActions")}
                            >
                              <IconDots className="h-4 w-4" />
                            </Button>
                          </DropdownMenuTrigger>
                        </TooltipTrigger>
                        <TooltipContent>
                          {t("sessions.moreActions")}
                        </TooltipContent>
                      </Tooltip>
                      <DropdownMenuContent align="end" className="w-52">
                        <DropdownMenuItem
                          disabled={!canDuplicate}
                          onClick={() => void duplicateSession()}
                        >
                          <IconCopy className="h-4 w-4" />
                          {t("sessions.duplicate")}
                        </DropdownMenuItem>
                        <DropdownMenuItem
                          disabled={importTargets.length === 0}
                          onClick={openConvert}
                        >
                          <IconArrowsExchange className="h-4 w-4" />
                          {t("sessions.useInOtherAgent")}
                        </DropdownMenuItem>
                        <DropdownMenuItem onClick={() => setExportOpen(true)}>
                          <IconDownload className="h-4 w-4" />
                          {t("sessions.export")}
                        </DropdownMenuItem>
                        <DropdownMenuSeparator />
                        <DropdownMenuItem
                          disabled={!canDelete}
                          className="text-destructive focus:text-destructive"
                          onClick={() => setDeleteOpen(true)}
                        >
                          <IconTrash className="h-4 w-4" />
                          {t("common.delete")}
                        </DropdownMenuItem>
                      </DropdownMenuContent>
                    </DropdownMenu>
                  </div>
                  {isLoadingStats ? (
                    <SessionStatsSkeleton />
                  ) : (
                    sessionStats && <SessionStatsSummary stats={sessionStats} />
                  )}
                </div>
              </div>
            </div>
            <div className="relative flex min-h-0 flex-1 overflow-hidden">
              {isLoadingOlderMessages && (
                <div className="pointer-events-none absolute left-1/2 top-3 z-20 flex -translate-x-1/2 items-center gap-2 rounded-full border bg-background/95 px-3 py-1.5 text-xs text-muted-foreground shadow-sm backdrop-blur">
                  <IconLoader2 className="h-3.5 w-3.5 animate-spin" />
                  <span>{t("sessions.loadingOlderMessages")}</span>
                </div>
              )}
              <ScrollArea
                ref={sessionScrollAreaRef}
                horizontal={false}
                className="h-full min-h-0 min-w-0 flex-1 overflow-hidden [&>div]:h-full [&>div]:max-w-full [&>div]:overflow-x-hidden"
              >
                {isLoadingSession ? (
                  <div className="p-6 text-sm text-muted-foreground">
                    {t("common.loading")}
                  </div>
                ) : (
                  <div className="mx-auto max-w-4xl px-6 py-5">
                    {visibleMessageStart === 0 &&
                      !selected.hasMoreMessages &&
                      displayMessages.length > 0 && (
                        <div className="flex justify-center py-2">
                          <span className="rounded-full border bg-muted/50 px-3 py-1 text-xs text-foreground/70">
                            {t("sessions.firstMessageReached")}
                          </span>
                        </div>
                      )}
                    <div
                      className="relative w-full"
                      style={{
                        height: `${messageVirtualizer.getTotalSize()}px`,
                      }}
                    >
                      {messageVirtualizer
                        .getVirtualItems()
                        .map((virtualItem) => {
                          const group = visibleMessageGroups[virtualItem.index];
                          if (!group) return null;
                          return (
                            <div
                              key={virtualItem.key}
                              ref={messageVirtualizer.measureElement}
                              data-index={virtualItem.index}
                              className={`absolute left-0 top-0 w-full ${
                                group.continuation ? "pb-5" : "pb-8"
                              }`}
                              style={{
                                transform: `translateY(${virtualItem.start}px)`,
                              }}
                            >
                              <MessageGroup
                                group={group}
                                plugin={selectedPlugin}
                                sessionAgentId={selected.agentId}
                                sessionNativeId={selected.nativeId}
                                userMessageIdSet={userMessageIdSet}
                                branchDisabled={
                                  Boolean(busyAction) || !canBranch
                                }
                                onBranch={branchFromMessage}
                                onToolGroupResize={handleToolGroupResize}
                              />
                            </div>
                          );
                        })}
                    </div>
                  </div>
                )}
              </ScrollArea>
              {userMessageNavItems.length > 0 && (
                <UserMessageNavigator
                  items={userMessageNavItems}
                  onSelect={jumpToUserMessage}
                  scrollAreaRef={sessionScrollAreaRef}
                  renderStart={visibleMessageStart}
                />
              )}
            </div>
          </>
        )}
      </div>

      <AlertDialog open={deleteOpen} onOpenChange={setDeleteOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("sessions.deleteTitle")}</AlertDialogTitle>
            <AlertDialogDescription>
              {selected?.agentId === "opencode"
                ? t("sessions.deleteDatabaseDescription")
                : t("sessions.deleteFileDescription")}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("common.cancel")}</AlertDialogCancel>
            <AlertDialogAction onClick={() => void deleteSession()}>
              {t("common.delete")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      <Dialog open={convertOpen} onOpenChange={setConvertOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t("sessions.convertTitle")}</DialogTitle>
            <DialogDescription>
              {t("sessions.convertDescription")}
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-2">
            <Label>{t("sessions.targetInstance")}</Label>
            <Select value={targetValue} onValueChange={setTargetValue}>
              <SelectTrigger>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {importTargets.map((target) => (
                  <SelectItem key={target.value} value={target.value}>
                    {target.label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <p className="text-xs text-muted-foreground">
              {t("sessions.convertNotice")}
            </p>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setConvertOpen(false)}>
              {t("common.cancel")}
            </Button>
            <Button
              disabled={!targetValue || busyAction === "convert"}
              onClick={() => void convertSession()}
            >
              {t("sessions.convertAndOpen")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog
        open={exportOpen}
        onOpenChange={(open) => {
          if (busyAction !== "export") setExportOpen(open);
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t("sessions.exportTitle")}</DialogTitle>
            <DialogDescription>
              {t("sessions.exportDescription")}
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-4">
            <div className="space-y-2">
              <Label>{t("sessions.exportFormat")}</Label>
              <Select
                value={exportOptions.format}
                disabled={busyAction === "export"}
                onValueChange={(format) =>
                  setExportOptions((current) => ({
                    ...current,
                    format: format as SessionExportFormat,
                  }))
                }
              >
                <SelectTrigger>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="html">HTML</SelectItem>
                  <SelectItem value="markdown">Markdown</SelectItem>
                  <SelectItem value="json">JSON</SelectItem>
                  {canExportNative && (
                    <SelectItem value="native">
                      {t("sessions.nativeFormat")}
                    </SelectItem>
                  )}
                </SelectContent>
              </Select>
            </div>
            {exportOptions.format !== "native" && (
              <div
                className={`space-y-3 rounded-md border p-3 ${
                  busyAction === "export"
                    ? "pointer-events-none opacity-60"
                    : ""
                }`}
              >
                <ExportSwitch
                  label={t("sessions.includeReasoning")}
                  checked={exportOptions.includeReasoning !== false}
                  onChange={(checked) =>
                    setExportOptions((current) => ({
                      ...current,
                      includeReasoning: checked,
                    }))
                  }
                />
                <ExportSwitch
                  label={t("sessions.includeToolCalls")}
                  checked={exportOptions.includeToolCalls !== false}
                  onChange={(checked) =>
                    setExportOptions((current) => ({
                      ...current,
                      includeToolCalls: checked,
                    }))
                  }
                />
                <ExportSwitch
                  label={t("sessions.includeToolResults")}
                  checked={exportOptions.includeToolResults !== false}
                  onChange={(checked) =>
                    setExportOptions((current) => ({
                      ...current,
                      includeToolResults: checked,
                    }))
                  }
                />
                <ExportSwitch
                  label={t("sessions.sanitizeSecrets")}
                  checked={exportOptions.sanitize !== false}
                  onChange={(checked) =>
                    setExportOptions((current) => ({
                      ...current,
                      sanitize: checked,
                    }))
                  }
                />
              </div>
            )}
            {busyAction === "export" && (
              <div
                role="status"
                className="flex items-center justify-center gap-2 py-2 text-sm text-muted-foreground"
              >
                <IconLoader2 className="h-4 w-4 animate-spin" />
                <span>{t("sessions.exportProcessing")}</span>
              </div>
            )}
          </div>
          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => setExportOpen(false)}
              disabled={busyAction === "export"}
            >
              {t("common.cancel")}
            </Button>
            <Button
              disabled={busyAction === "export"}
              onClick={() => void exportSession()}
            >
              {busyAction === "export" && (
                <IconLoader2 className="h-4 w-4 animate-spin" />
              )}
              {busyAction === "export"
                ? t("sessions.exportProcessingButton")
                : isDesktopRuntime
                  ? t("sessions.chooseLocationAndExport")
                  : t("sessions.download")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </PageLayout>
  );
};

const ExportSwitch: React.FC<{
  label: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
}> = ({ label, checked, onChange }) => (
  <div className="flex items-center justify-between gap-3">
    <Label>{label}</Label>
    <Switch checked={checked} onCheckedChange={onChange} />
  </div>
);

export default SessionsPage;
