import React, {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { useTranslation } from "react-i18next";
import { useVirtualizer } from "@tanstack/react-virtual";
import { IconChevronLeft, IconChevronRight } from "@tabler/icons-react";
import {
  ScrollArea,
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@mcp_link/ui";
import { sampleNavItems } from "./session-utils";
import type { UserMessageNavItem } from "@mcp_link/shared";

interface UserMessageNavigatorProps {
  items: UserMessageNavItem[];
  onSelect: (messageId: string) => void | Promise<void>;
  scrollAreaRef: React.RefObject<HTMLDivElement | null>;
  renderStart: number;
}

const UserMessageNavigator: React.FC<UserMessageNavigatorProps> = React.memo(
  ({ items, onSelect, scrollAreaRef, renderStart }) => {
    const { t } = useTranslation();
    const [expanded, setExpanded] = useState(false);
    const [activeMessageId, setActiveMessageId] = useState<string | null>(
      () => items.at(-1)?.messageId ?? null,
    );
    const scrollAreaRootRef = useRef<HTMLDivElement>(null);
    const virtualizer = useVirtualizer({
      count: items.length,
      getScrollElement: () =>
        scrollAreaRootRef.current?.querySelector<HTMLElement>(
          "[data-radix-scroll-area-viewport]",
        ) ?? null,
      estimateSize: () => 40,
      overscan: 8,
      getItemKey: (index) => items[index].messageId,
    });

    useEffect(() => {
      const index = items.findIndex(
        (item) => item.messageId === activeMessageId,
      );
      if (index >= 0) virtualizer.scrollToIndex(index, { align: "auto" });
    }, [activeMessageId, items, virtualizer]);

    useEffect(() => {
      setActiveMessageId((current) =>
        current && items.some((item) => item.messageId === current)
          ? current
          : (items.at(-1)?.messageId ?? null),
      );
    }, [items]);

    useEffect(() => {
      const viewport = scrollAreaRef.current?.querySelector<HTMLElement>(
        "[data-radix-scroll-area-viewport]",
      );
      if (!viewport || items.length === 0) return;
      let frame = 0;
      const updateActiveMessage = () => {
        window.cancelAnimationFrame(frame);
        frame = window.requestAnimationFrame(() => {
          const viewportRect = viewport.getBoundingClientRect();
          const threshold =
            viewportRect.top + Math.min(180, viewportRect.height * 0.35);
          const elements = viewport.querySelectorAll<HTMLElement>(
            "[data-user-message-id]",
          );
          let nextMessageId: string | null = null;
          for (const element of elements) {
            const messageId = element.dataset.userMessageId;
            if (!messageId) continue;
            if (element.getBoundingClientRect().top <= threshold)
              nextMessageId = messageId;
            else break;
          }
          if (nextMessageId != null) {
            setActiveMessageId((current) =>
              current === nextMessageId ? current : nextMessageId,
            );
          }
        });
      };
      updateActiveMessage();
      viewport.addEventListener("scroll", updateActiveMessage, {
        passive: true,
      });
      return () => {
        window.cancelAnimationFrame(frame);
        viewport.removeEventListener("scroll", updateActiveMessage);
      };
    }, [items, renderStart, scrollAreaRef]);

    const selectMessage = useCallback(
      (messageId: string) => {
        setActiveMessageId(messageId);
        void onSelect(messageId);
      },
      [onSelect],
    );
    const railItems = useMemo(
      () => sampleNavItems(items, activeMessageId, 8),
      [activeMessageId, items],
    );
    const panelHeight = Math.min(items.length * 40 + 24, 320);

    return (
      <div className="absolute right-3 top-1/2 z-20 hidden -translate-y-1/2 lg:block">
        {!expanded && (
          <div className="flex min-w-9 flex-col items-center gap-2 rounded-md px-1.5 py-2">
            <Tooltip>
              <TooltipTrigger asChild>
                <button
                  type="button"
                  onClick={() => setExpanded(true)}
                  aria-label={t("sessions.expandList")}
                  className="flex h-6 w-6 items-center justify-center rounded-md text-muted-foreground outline-none hover:bg-muted hover:text-foreground focus-visible:ring-1 focus-visible:ring-ring"
                >
                  <IconChevronLeft className="h-3.5 w-3.5" />
                </button>
              </TooltipTrigger>
              <TooltipContent side="left">
                {t("sessions.expandList")}
              </TooltipContent>
            </Tooltip>
            {railItems.map((item) => {
              const active = item.messageId === activeMessageId;
              return (
                <Tooltip key={item.messageId}>
                  <TooltipTrigger asChild>
                    <button
                      type="button"
                      onClick={() => selectMessage(item.messageId)}
                      className="flex h-2.5 w-6 items-center justify-end outline-none"
                    >
                      <span
                        className={`h-[3px] rounded-full transition-all ${active ? "w-5 bg-primary" : "w-3 bg-muted-foreground/30"}`}
                      />
                    </button>
                  </TooltipTrigger>
                  <TooltipContent side="left" align="center">
                    {item.text}
                  </TooltipContent>
                </Tooltip>
              );
            })}
          </div>
        )}
        {expanded && (
          <div className="relative w-[360px] max-w-[calc(100vw-2rem)] overflow-hidden rounded-2xl border bg-background shadow-md">
            <Tooltip>
              <TooltipTrigger asChild>
                <button
                  type="button"
                  onClick={() => setExpanded(false)}
                  aria-label={t("sessions.collapseList")}
                  className="absolute right-2 top-2 z-10 flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground outline-none hover:bg-muted hover:text-foreground focus-visible:ring-1 focus-visible:ring-ring"
                >
                  <IconChevronRight className="h-4 w-4" />
                </button>
              </TooltipTrigger>
              <TooltipContent side="left">
                {t("sessions.collapseList")}
              </TooltipContent>
            </Tooltip>
            <ScrollArea
              ref={scrollAreaRootRef}
              horizontal={false}
              className="overflow-hidden"
              style={{ height: `${panelHeight}px` }}
            >
              <div
                className="relative my-3 w-full"
                style={{ height: `${virtualizer.getTotalSize()}px` }}
              >
                {virtualizer.getVirtualItems().map((virtualItem) => {
                  const item = items[virtualItem.index];
                  const active = item.messageId === activeMessageId;
                  return (
                    <div
                      key={item.messageId}
                      className="absolute left-0 top-0 w-full px-4"
                      style={{
                        height: `${virtualItem.size}px`,
                        transform: `translateY(${virtualItem.start}px)`,
                      }}
                    >
                      <Tooltip>
                        <TooltipTrigger asChild>
                          <button
                            type="button"
                            onClick={() => selectMessage(item.messageId)}
                            className={`grid h-10 w-full grid-cols-[minmax(0,1fr)_auto] items-center gap-4 px-2 text-right text-sm outline-none transition-colors hover:text-foreground focus-visible:ring-1 focus-visible:ring-ring ${active ? "font-medium text-primary" : "text-muted-foreground"}`}
                          >
                            <span className="truncate">{item.text}</span>
                            <span
                              className={`h-[3px] rounded-full ${active ? "w-5 bg-primary" : "w-3 bg-muted-foreground/30"}`}
                            />
                          </button>
                        </TooltipTrigger>
                        <TooltipContent>{item.text}</TooltipContent>
                      </Tooltip>
                    </div>
                  );
                })}
              </div>
            </ScrollArea>
          </div>
        )}
      </div>
    );
  },
);
UserMessageNavigator.displayName = "UserMessageNavigator";

export default UserMessageNavigator;
