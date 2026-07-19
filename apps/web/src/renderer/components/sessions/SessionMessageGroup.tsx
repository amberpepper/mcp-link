import React, {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { useTranslation } from "react-i18next";
import {
  Badge,
  Button,
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@mcp_link/ui";
import {
  IconChevronDown,
  IconChevronRight,
  IconGitBranch,
  IconLoader2,
  IconPaperclip,
  IconUser,
} from "@tabler/icons-react";
import type {
  AgentPluginDescriptor,
  AgentSessionAttachment,
  AgentSessionMessage,
  AgentSessionMessageGroup as MessageGroupData,
} from "@mcp_link/shared";
import Lightbox from "yet-another-react-lightbox";
import Zoom from "yet-another-react-lightbox/plugins/zoom";
import "yet-another-react-lightbox/styles.css";

import AgentAvatar from "@/renderer/components/agents/AgentAvatar";
import { usePlatformAPI } from "@/renderer/platform-api";
import SessionMarkdown from "./SessionMarkdown";
import {
  chatMessageShellClass,
  formatMessageTimestamp,
  messageAlignmentClass,
  prettyJson,
  roleClass,
} from "./session-utils";

const attachmentDataCache = new Map<string, string>();
const ATTACHMENT_CACHE_LIMIT = 64;

const MessageGroup: React.FC<{
  group: MessageGroupData;
  plugin: AgentPluginDescriptor | null;
  sessionAgentId: string;
  sessionNativeId: string;
  userMessageIdSet: Set<string>;
  onBranch: (originalIndex: number) => void;
  onToolGroupResize: (groupKey: string) => void;
  branchDisabled: boolean;
}> = React.memo(
  ({
    group,
    plugin,
    sessionAgentId,
    sessionNativeId,
    userMessageIdSet,
    onBranch,
    onToolGroupResize,
    branchDisabled,
  }) => {
    const { t } = useTranslation();
    const isUser = group.role === "user";
    const isSystem = group.role === "system";
    const displayName = isUser
      ? t("sessions.me")
      : isSystem
        ? t("sessions.roles.system")
        : (plugin?.name ?? t("sessions.roles.assistant"));
    const groupModel =
      isUser || isSystem
        ? null
        : group.messages.find(({ item }) => item.model)?.item.model;
    const messageList = group.messages.map(({ item, originalIndex }) => {
      const timestamp = formatMessageTimestamp(item.timestamp);
      return (
        <div
          key={`${item.id}-${originalIndex}`}
          id={`session-message-${item.id}`}
          data-user-message-id={
            userMessageIdSet.has(item.id) ? item.id : undefined
          }
          className={`scroll-mt-4 flex min-w-0 max-w-full flex-col ${messageAlignmentClass(item.role)}`}
        >
          <MessageCard
            item={item}
            plugin={plugin}
            sessionAgentId={sessionAgentId}
            sessionNativeId={sessionNativeId}
            onBranch={() => onBranch(originalIndex)}
            branchDisabled={branchDisabled}
            showIdentity={false}
            onToolResize={() => onToolGroupResize(group.key)}
          />
          {timestamp && (
            <time
              dateTime={timestamp.dateTime}
              title={timestamp.title}
              className="mt-1 px-1 text-[10px] tabular-nums text-muted-foreground/70"
            >
              {timestamp.label}
            </time>
          )}
        </div>
      );
    });

    if (isSystem) {
      return (
        <div className="flex w-fit max-w-[88%] flex-col items-center gap-2">
          {!group.continuation && (
            <div className="flex min-w-0 max-w-full items-center gap-2">
              <span className="shrink-0 text-xs text-muted-foreground">
                {displayName}
              </span>
              {groupModel && (
                <Badge
                  variant="secondary"
                  className="h-5 max-w-56 truncate px-1.5 py-0 font-mono text-[10px] font-normal"
                >
                  {groupModel}
                </Badge>
              )}
            </div>
          )}
          {messageList}
        </div>
      );
    }

    return (
      <div
        className={`flex w-full min-w-0 max-w-full items-start gap-3 ${
          isUser ? "flex-row-reverse justify-end" : "justify-start"
        }`}
      >
        {group.continuation ? (
          <div className="h-9 w-9 shrink-0" aria-hidden="true" />
        ) : isUser ? (
          <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-primary text-primary-foreground">
            <IconUser className="h-4 w-4" />
          </div>
        ) : (
          <AgentAvatar plugin={plugin} size="md" />
        )}
        <div
          className={`flex min-w-0 flex-1 flex-col gap-2 ${
            isUser ? "items-end" : "items-start"
          }`}
        >
          {!group.continuation && (
            <div className="flex min-w-0 max-w-full items-center gap-2">
              <span className="text-sm font-medium">{displayName}</span>
              {groupModel && (
                <Badge
                  variant="secondary"
                  className="h-5 max-w-56 truncate px-1.5 py-0 font-mono text-[10px] font-normal"
                >
                  {groupModel}
                </Badge>
              )}
            </div>
          )}
          {messageList}
        </div>
      </div>
    );
  },
);
MessageGroup.displayName = "MessageGroup";

const MessageCard: React.FC<{
  item: AgentSessionMessage;
  plugin: AgentPluginDescriptor | null;
  sessionAgentId: string;
  sessionNativeId: string;
  onBranch: () => void;
  branchDisabled: boolean;
  showIdentity?: boolean;
  onToolResize: () => void;
}> = React.memo(
  ({
    item,
    plugin,
    sessionAgentId,
    sessionNativeId,
    onBranch,
    branchDisabled,
    showIdentity = true,
    onToolResize,
  }) => {
    const { t } = useTranslation();
    const [toolOpen, setToolOpen] = useState(false);
    const isToolContent =
      item.kind === "tool-call" || item.kind === "tool-result";
    const body = useMemo(() => {
      if (!isToolContent) return item.text ?? "";
      if (!toolOpen) return "";
      return item.kind === "tool-call"
        ? prettyJson(item.toolInput)
        : prettyJson(item.toolOutput);
    }, [
      isToolContent,
      item.kind,
      item.text,
      item.toolInput,
      item.toolOutput,
      toolOpen,
    ]);
    const isUser = item.role === "user";
    const isSystem = item.role === "system";
    const canBranchHere =
      item.kind === "text" &&
      (item.role === "user" || item.role === "assistant");
    const displayName = isUser
      ? t("sessions.me")
      : isSystem
        ? t("sessions.roles.system")
        : (plugin?.name ?? t("sessions.roles.assistant"));

    const handleToolOpenChange = useCallback(
      (open: boolean) => {
        onToolResize();
        setToolOpen(open);
      },
      [onToolResize],
    );

    const bubble = isToolContent ? (
      <Collapsible
        open={toolOpen}
        onOpenChange={handleToolOpenChange}
        className="w-full"
      >
        <article
          className={`w-full min-w-0 overflow-hidden rounded-xl border ${roleClass(item.role)}`}
        >
          <CollapsibleTrigger asChild>
            <button
              type="button"
              className="flex min-h-11 w-full min-w-0 items-center gap-2 px-3 py-2 text-left outline-none focus-visible:ring-1 focus-visible:ring-ring"
            >
              {toolOpen ? (
                <IconChevronDown className="h-4 w-4 shrink-0 text-muted-foreground" />
              ) : (
                <IconChevronRight className="h-4 w-4 shrink-0 text-muted-foreground" />
              )}
              <span className="shrink-0 text-xs text-muted-foreground">
                {t(`sessions.kinds.${item.kind}`)}
              </span>
              {item.toolName && (
                <span className="min-w-0 truncate font-mono text-xs">
                  {item.toolName}
                </span>
              )}
            </button>
          </CollapsibleTrigger>
          <CollapsibleContent>
            <div className="border-t p-3">
              <pre className="min-w-0 max-w-full overflow-x-auto whitespace-pre-wrap break-all rounded-md bg-muted/50 p-3 font-mono text-xs leading-5">
                {body}
              </pre>
            </div>
          </CollapsibleContent>
          {item.attachments && item.attachments.length > 0 && (
            <div className="border-t p-3">
              <SessionAttachments
                attachments={item.attachments}
                agentId={sessionAgentId}
                nativeId={sessionNativeId}
                messageId={item.id}
                onResize={onToolResize}
              />
            </div>
          )}
        </article>
      </Collapsible>
    ) : (
      <article
        className={`w-fit min-w-0 overflow-hidden rounded-xl border p-4 ${
          !showIdentity && canBranchHere
            ? "max-w-[calc(100%-1.5rem)]"
            : "max-w-full"
        } ${roleClass(item.role)}`}
      >
        {item.kind !== "text" && (
          <div className="mb-2 text-xs text-muted-foreground">
            {t(`sessions.kinds.${item.kind}`)}
          </div>
        )}
        {item.attachments && item.attachments.length > 0 && (
          <SessionAttachments
            attachments={item.attachments}
            agentId={sessionAgentId}
            nativeId={sessionNativeId}
            messageId={item.id}
            onResize={onToolResize}
          />
        )}
        {body && <SessionMarkdown content={body} />}
      </article>
    );

    if (!showIdentity) {
      if (isSystem) return bubble;
      const branchButton = canBranchHere ? (
        <Tooltip>
          <TooltipTrigger asChild>
            <span className="inline-flex shrink-0">
              <Button
                size="icon"
                variant="ghost"
                className="mt-2 h-5 w-5 shrink-0 opacity-0 transition-opacity group-hover/message:opacity-100 focus-visible:opacity-100"
                disabled={branchDisabled}
                aria-label={t("sessions.branchHere")}
                onClick={onBranch}
              >
                <IconGitBranch className="h-3 w-3" />
              </Button>
            </span>
          </TooltipTrigger>
          <TooltipContent>{t("sessions.branchHere")}</TooltipContent>
        </Tooltip>
      ) : null;
      return (
        <div
          className={`group/message flex w-full min-w-0 max-w-full items-start gap-1 overflow-hidden ${
            isUser ? "justify-end" : "justify-start"
          }`}
        >
          {!isUser && bubble}
          {branchButton}
          {isUser && bubble}
        </div>
      );
    }

    if (isSystem) {
      return (
        <div className="group flex w-fit max-w-[88%] flex-col items-center">
          <span className="mb-1 text-xs text-muted-foreground">
            {displayName}
          </span>
          {bubble}
        </div>
      );
    }

    return (
      <div
        className={`group flex min-w-0 items-start gap-3 ${chatMessageShellClass(item.role, isToolContent)} ${
          isUser ? "flex-row-reverse" : ""
        }`}
      >
        {isUser ? (
          <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-primary text-primary-foreground">
            <IconUser className="h-4 w-4" />
          </div>
        ) : (
          <AgentAvatar plugin={plugin} size="md" />
        )}
        <div
          className={`flex min-w-0 flex-1 flex-col ${isUser ? "items-end" : "items-start"}`}
        >
          <div
            className={`mb-1 flex h-5 items-center gap-1 ${isUser ? "flex-row-reverse" : ""}`}
          >
            <span className="text-xs text-muted-foreground">{displayName}</span>
            {canBranchHere && (
              <Tooltip>
                <TooltipTrigger asChild>
                  <span className="inline-flex">
                    <Button
                      size="icon"
                      variant="ghost"
                      className="h-5 w-5 opacity-0 transition-opacity group-hover:opacity-100 focus-visible:opacity-100"
                      disabled={branchDisabled}
                      aria-label={t("sessions.branchHere")}
                      onClick={onBranch}
                    >
                      <IconGitBranch className="h-3 w-3" />
                    </Button>
                  </span>
                </TooltipTrigger>
                <TooltipContent>{t("sessions.branchHere")}</TooltipContent>
              </Tooltip>
            )}
          </div>
          {bubble}
        </div>
      </div>
    );
  },
);
MessageCard.displayName = "MessageCard";

const SessionAttachments: React.FC<{
  attachments: AgentSessionAttachment[];
  agentId: string;
  nativeId: string;
  messageId: string;
  onResize: () => void;
}> = React.memo(({ attachments, agentId, nativeId, messageId, onResize }) => (
  <div className="mb-3 grid max-w-2xl gap-2 last:mb-0 sm:grid-cols-2">
    {attachments.map((attachment) => (
      <SessionAttachmentPreview
        key={`${agentId}:${nativeId}:${attachment.id}`}
        attachment={attachment}
        agentId={agentId}
        nativeId={nativeId}
        messageId={messageId}
        onResize={onResize}
      />
    ))}
  </div>
));
SessionAttachments.displayName = "SessionAttachments";

const SessionAttachmentPreview: React.FC<{
  attachment: AgentSessionAttachment;
  agentId: string;
  nativeId: string;
  messageId: string;
  onResize: () => void;
}> = React.memo(({ attachment, agentId, nativeId, messageId, onResize }) => {
  const { t } = useTranslation();
  const platformAPI = usePlatformAPI();
  const cacheKey = `${agentId}:${nativeId}:${attachment.id}`;
  const [source, setSource] = useState(
    () => attachmentDataCache.get(cacheKey) ?? "",
  );
  const [error, setError] = useState(false);
  const [previewOpen, setPreviewOpen] = useState(false);
  const onResizeRef = useRef(onResize);

  useEffect(() => {
    onResizeRef.current = onResize;
  }, [onResize]);

  useEffect(() => {
    const cached = attachmentDataCache.get(cacheKey);
    if (cached) {
      setSource(cached);
      setError(false);
      return;
    }

    let active = true;
    setSource("");
    setError(false);
    void platformAPI.agents.sessions
      .getAttachment(agentId, nativeId, messageId, attachment)
      .then((data) => {
        if (!active) return;
        attachmentDataCache.set(cacheKey, data.dataUrl);
        if (attachmentDataCache.size > ATTACHMENT_CACHE_LIMIT) {
          const oldest = attachmentDataCache.keys().next().value;
          if (oldest) attachmentDataCache.delete(oldest);
        }
        setSource(data.dataUrl);
        window.requestAnimationFrame(() => onResizeRef.current());
      })
      .catch(() => {
        if (active) setError(true);
      });
    return () => {
      active = false;
    };
  }, [agentId, attachment.id, cacheKey, messageId, nativeId, platformAPI]);

  if (error) {
    return (
      <div className="flex min-h-20 items-center gap-2 rounded-lg border bg-muted/30 px-3 py-2 text-xs text-muted-foreground">
        <IconPaperclip className="h-4 w-4 shrink-0" />
        <span className="truncate">
          {attachment.name ?? t("sessions.attachmentLoadFailed")}
        </span>
      </div>
    );
  }

  if (!source) {
    return (
      <div className="flex min-h-20 items-center justify-center rounded-lg border bg-muted/30 text-muted-foreground">
        <IconLoader2 className="h-4 w-4 animate-spin" />
      </div>
    );
  }

  if (
    attachment.kind === "image" ||
    attachment.mimeType?.startsWith("image/")
  ) {
    return (
      <>
        <button
          type="button"
          className="block w-full cursor-zoom-in overflow-hidden rounded-lg border bg-black/5"
          aria-label={attachment.name ?? t("sessions.imageAttachment")}
          onClick={() => setPreviewOpen(true)}
        >
          <img
            src={source}
            alt={attachment.name ?? t("sessions.imageAttachment")}
            className="max-h-[420px] w-full object-contain"
            loading="lazy"
            onLoad={onResize}
          />
        </button>
        <Lightbox
          open={previewOpen}
          close={() => setPreviewOpen(false)}
          slides={[
            {
              src: source,
              alt: attachment.name ?? t("sessions.imageAttachment"),
            },
          ]}
          plugins={[Zoom]}
          carousel={{ finite: true }}
          controller={{ closeOnBackdropClick: true }}
          zoom={{ maxZoomPixelRatio: 4, scrollToZoom: true }}
          render={{
            buttonPrev: () => null,
            buttonNext: () => null,
          }}
        />
      </>
    );
  }

  return (
    <a
      href={source}
      download={attachment.name ?? undefined}
      className="flex min-h-20 items-center gap-2 rounded-lg border bg-muted/30 px-3 py-2 text-xs hover:bg-muted/50"
    >
      <IconPaperclip className="h-4 w-4 shrink-0" />
      <span className="truncate">
        {attachment.name ?? t("sessions.fileAttachment")}
      </span>
    </a>
  );
});
SessionAttachmentPreview.displayName = "SessionAttachmentPreview";

export default MessageGroup;
