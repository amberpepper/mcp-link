import React from "react";
import type { Icon } from "@tabler/icons-react";
import {
  IconCheck,
  IconChevronLeft,
  IconChevronRight,
  IconRefresh,
  IconSearch,
} from "@tabler/icons-react";
import {
  Button,
  Input,
  Pagination,
  PaginationContent,
  PaginationEllipsis,
  PaginationItem,
  PaginationLink,
  Skeleton,
  Tabs,
  TabsList,
  TabsTrigger,
} from "@mcp_link/ui";

import EmptyState from "@/renderer/components/common/EmptyState";
import { cn } from "@/renderer/utils/tailwind-utils";

type PageToken = number | "...";

interface MarketSourceOption<T extends string> {
  value: T;
  label: string;
}

export interface MarketMetaItem {
  label: React.ReactNode;
  icon?: Icon;
  emphasis?: "muted" | "success";
}

export interface MarketLink {
  label: string;
  icon?: Icon;
  url: string;
}

interface MarketPrimaryAction {
  label: string;
  icon?: Icon;
  onClick: () => void;
  loading?: boolean;
  disabled?: boolean;
  installed?: boolean;
  installedLabel?: string;
}

export interface MarketCardItem {
  id: string;
  title: React.ReactNode;
  subtitle?: React.ReactNode;
  metadata?: MarketMetaItem[];
  description?: React.ReactNode;
  tags?: string[];
  links?: MarketLink[];
  primaryAction: MarketPrimaryAction;
}

interface MarketViewProps<T extends string> {
  sources: MarketSourceOption<T>[];
  source: T;
  onSourceChange: (value: T) => void;
  search: string;
  onSearchChange: (value: string) => void;
  onRefresh: () => void;
  searchPlaceholder?: string;
  isLoading?: boolean;
  summary?: React.ReactNode;
  partialLabel?: React.ReactNode;
  error?: React.ReactNode;
  emptyTitle: React.ReactNode;
  emptyDescription?: React.ReactNode;
  items: MarketCardItem[];
  currentPage: number;
  totalPages: number;
  onPageChange: (page: number) => void;
  previousLabel: string;
  nextLabel: string;
}

export function MarketView<T extends string>({
  sources,
  source,
  onSourceChange,
  search,
  onSearchChange,
  onRefresh,
  searchPlaceholder,
  isLoading,
  summary,
  partialLabel,
  error,
  emptyTitle,
  emptyDescription,
  items,
  currentPage,
  totalPages,
  onPageChange,
  previousLabel,
  nextLabel,
}: MarketViewProps<T>) {
  return (
    <div className="flex min-w-0 flex-col gap-4 overflow-x-hidden">
      <MarketToolbar
        sources={sources}
        source={source}
        onSourceChange={onSourceChange}
        search={search}
        onSearchChange={onSearchChange}
        onRefresh={onRefresh}
        searchPlaceholder={searchPlaceholder}
        isLoading={isLoading}
      />

      {!isLoading && (summary || partialLabel) && (
        <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
          {summary && <span>{summary}</span>}
          {partialLabel && (
            <span className="text-amber-600">· {partialLabel}</span>
          )}
        </div>
      )}

      {error && (
        <div className="rounded-md border border-destructive/30 bg-destructive/5 px-3 py-2 text-sm text-destructive">
          {error}
        </div>
      )}

      {isLoading ? (
        <MarketLoadingGrid />
      ) : items.length === 0 ? (
        <>
          <EmptyState
            icon={IconSearch}
            title={emptyTitle}
            description={emptyDescription}
          />
          <MarketPagination
            current={currentPage}
            total={totalPages}
            onChange={onPageChange}
            previousLabel={previousLabel}
            nextLabel={nextLabel}
          />
        </>
      ) : (
        <>
          <div className="grid min-w-0 grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-3 2xl:grid-cols-4">
            {items.map((item) => (
              <MarketCard key={item.id} item={item} />
            ))}
          </div>
          <MarketPagination
            current={currentPage}
            total={totalPages}
            onChange={onPageChange}
            previousLabel={previousLabel}
            nextLabel={nextLabel}
          />
        </>
      )}
    </div>
  );
}

export function formatCount(value: number): string {
  if (value < 1000) return value.toString();
  if (value < 10_000) return `${(value / 1000).toFixed(1)}k`;
  if (value < 1_000_000) return `${Math.round(value / 1000)}k`;
  return `${(value / 1_000_000).toFixed(1)}M`;
}

function MarketToolbar<T extends string>({
  sources,
  source,
  onSourceChange,
  search,
  onSearchChange,
  onRefresh,
  searchPlaceholder,
  isLoading,
}: Pick<
  MarketViewProps<T>,
  | "sources"
  | "source"
  | "onSourceChange"
  | "search"
  | "onSearchChange"
  | "onRefresh"
  | "searchPlaceholder"
  | "isLoading"
>) {
  return (
    <div className="mt-2 flex min-w-0 flex-col gap-2 sm:flex-row sm:items-center">
      <Tabs
        value={source}
        onValueChange={(value) => onSourceChange(value as T)}
      >
        <TabsList>
          {sources.map((item) => (
            <TabsTrigger key={item.value} value={item.value}>
              {item.label}
            </TabsTrigger>
          ))}
        </TabsList>
      </Tabs>

      <div className="relative min-w-0 flex-1">
        <IconSearch className="absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
        <Input
          value={search}
          onChange={(event) => onSearchChange(event.target.value)}
          className="pl-9"
          placeholder={searchPlaceholder}
        />
      </div>

      <Button
        variant="outline"
        size="icon"
        onClick={onRefresh}
        disabled={isLoading}
      >
        <IconRefresh className={cn("h-4 w-4", isLoading && "animate-spin")} />
      </Button>
    </div>
  );
}

function MarketCard({ item }: { item: MarketCardItem }) {
  return (
    <div className="group flex min-w-0 flex-col rounded-lg border bg-card p-4 transition-all hover:border-primary/40 hover:shadow-sm">
      <div className="mb-1 flex min-w-0 items-start justify-between gap-2">
        <h3 className="min-w-0 truncate text-base font-semibold">
          {item.title}
        </h3>
        <PrimaryButton {...item.primaryAction} />
      </div>

      {item.metadata && item.metadata.length > 0 && (
        <div className="mb-1.5 flex flex-wrap items-center gap-1.5 text-xs text-muted-foreground">
          {item.metadata.map((meta, index) => {
            const Icon = meta.icon;
            return (
              <React.Fragment key={index}>
                {index > 0 && <span className="opacity-40">·</span>}
                <span
                  className={cn(
                    "inline-flex min-w-0 items-center gap-1",
                    meta.emphasis === "success" && "text-emerald-600",
                  )}
                >
                  {Icon && <Icon className="h-3 w-3" />}
                  <span className="truncate">{meta.label}</span>
                </span>
              </React.Fragment>
            );
          })}
        </div>
      )}

      {item.subtitle && (
        <div className="mb-3 truncate font-mono text-xs text-muted-foreground">
          {item.subtitle}
        </div>
      )}

      {item.description && (
        <p className="mb-3 line-clamp-3 min-h-[3.75rem] text-sm leading-5 text-muted-foreground">
          {item.description}
        </p>
      )}

      {item.tags && item.tags.length > 0 && (
        <div className="mb-2 flex flex-wrap gap-x-2 gap-y-1 text-xs text-muted-foreground">
          {item.tags.slice(0, 3).map((tag) => (
            <span key={tag}>#{tag}</span>
          ))}
          {item.tags.length > 3 && (
            <span className="opacity-70">+{item.tags.length - 3}</span>
          )}
        </div>
      )}

      {item.links && item.links.length > 0 && (
        <div className="-ml-2 mt-auto flex flex-wrap items-center gap-1">
          {item.links.map((link) => {
            const Icon = link.icon;
            return (
              <Button
                key={`${link.label}:${link.url}`}
                variant="ghost"
                size="sm"
                className="h-7 max-w-full px-2 text-xs text-muted-foreground"
                onClick={(event) => {
                  event.stopPropagation();
                  window.open(link.url, "_blank", "noopener,noreferrer");
                }}
              >
                {Icon && <Icon className="h-3.5 w-3.5" />}
                <span className="truncate">{link.label}</span>
              </Button>
            );
          })}
        </div>
      )}
    </div>
  );
}

function PrimaryButton({
  label,
  icon: Icon,
  onClick,
  loading,
  disabled,
  installed,
  installedLabel,
}: MarketPrimaryAction) {
  return (
    <Button
      size="sm"
      variant={installed ? "secondary" : "default"}
      className="h-7 shrink-0 gap-1.5 px-2.5 text-xs"
      disabled={loading || installed || disabled}
      onClick={(event) => {
        event.stopPropagation();
        onClick();
      }}
    >
      {installed ? (
        <>
          <IconCheck className="h-3.5 w-3.5" />
          {installedLabel}
        </>
      ) : loading ? (
        <>
          <IconRefresh className="h-3.5 w-3.5 animate-spin" />
          {label}
        </>
      ) : (
        <>
          {Icon && <Icon className="h-3.5 w-3.5" />}
          {label}
        </>
      )}
    </Button>
  );
}

function MarketLoadingGrid({ count = 8 }: { count?: number }) {
  return (
    <div className="grid min-w-0 grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-3 2xl:grid-cols-4">
      {Array.from({ length: count }, (_, index) => (
        <div key={index} className="rounded-lg border bg-card p-4">
          <div className="mb-2 flex items-center justify-between gap-2">
            <Skeleton className="h-5 w-36" />
            <Skeleton className="h-7 w-16" />
          </div>
          <Skeleton className="mb-1.5 h-3 w-48" />
          <Skeleton className="mb-3 h-3 w-40" />
          <Skeleton className="mb-1 h-4 w-full" />
          <Skeleton className="mb-1 h-4 w-11/12" />
          <Skeleton className="h-4 w-3/4" />
        </div>
      ))}
    </div>
  );
}

function MarketPagination({
  current,
  total,
  onChange,
  previousLabel,
  nextLabel,
}: {
  current: number;
  total: number;
  onChange: (page: number) => void;
  previousLabel: string;
  nextLabel: string;
}) {
  if (total <= 1) return null;
  const pages = paginate(current, total);

  return (
    <Pagination className="pt-2">
      <PaginationContent>
        <PaginationItem>
          <PaginationLink
            href="#"
            size="default"
            className={cn(
              "gap-1 pl-2.5",
              current === 1 && "pointer-events-none opacity-50",
            )}
            onClick={(event) => {
              event.preventDefault();
              onChange(Math.max(1, current - 1));
            }}
          >
            <IconChevronLeft className="h-4 w-4" />
            <span>{previousLabel}</span>
          </PaginationLink>
        </PaginationItem>

        {pages.map((page, index) =>
          page === "..." ? (
            <PaginationItem key={`gap-${index}`}>
              <PaginationEllipsis />
            </PaginationItem>
          ) : (
            <PaginationItem key={page}>
              <PaginationLink
                href="#"
                isActive={page === current}
                onClick={(event) => {
                  event.preventDefault();
                  onChange(page);
                }}
              >
                {page}
              </PaginationLink>
            </PaginationItem>
          ),
        )}

        <PaginationItem>
          <PaginationLink
            href="#"
            size="default"
            className={cn(
              "gap-1 pr-2.5",
              current === total && "pointer-events-none opacity-50",
            )}
            onClick={(event) => {
              event.preventDefault();
              onChange(Math.min(total, current + 1));
            }}
          >
            <span>{nextLabel}</span>
            <IconChevronRight className="h-4 w-4" />
          </PaginationLink>
        </PaginationItem>
      </PaginationContent>
    </Pagination>
  );
}

function paginate(current: number, total: number): PageToken[] {
  if (total <= 7) return Array.from({ length: total }, (_, index) => index + 1);
  if (current <= 4) return [1, 2, 3, 4, 5, "...", total];
  if (current >= total - 3) {
    return [1, "...", total - 4, total - 3, total - 2, total - 1, total];
  }
  return [1, "...", current - 1, current, current + 1, "...", total];
}
