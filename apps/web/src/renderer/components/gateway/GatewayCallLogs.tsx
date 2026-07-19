import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
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
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  Input,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  Skeleton,
} from "@mcp_link/ui";
import type {
  GatewayCallLog,
  GatewayCallStatus,
  GatewayProvider,
} from "@mcp_link/shared";
import { Copy, RefreshCw, Trash2 } from "lucide-react";
import { toast } from "sonner";

import { usePlatformAPI } from "@/renderer/platform-api";

const PAGE_SIZE = 100;

export function GatewayCallLogs({
  providers,
}: {
  providers: GatewayProvider[];
}) {
  const { t } = useTranslation();
  const platformAPI = usePlatformAPI();
  const requestRef = useRef(0);
  const logsRef = useRef<GatewayCallLog[]>([]);
  const [logs, setLogs] = useState<GatewayCallLog[]>([]);
  const [loading, setLoading] = useState(true);
  const [loadingMore, setLoadingMore] = useState(false);
  const [hasMore, setHasMore] = useState(false);
  const [status, setStatus] = useState<GatewayCallStatus | "all">("all");
  const [providerId, setProviderId] = useState("all");
  const [search, setSearch] = useState("");
  const [debouncedSearch, setDebouncedSearch] = useState("");
  const [selected, setSelected] = useState<GatewayCallLog | null>(null);
  const [clearOpen, setClearOpen] = useState(false);
  const [clearing, setClearing] = useState(false);

  useEffect(() => {
    const timer = window.setTimeout(
      () => setDebouncedSearch(search.trim()),
      250,
    );
    return () => window.clearTimeout(timer);
  }, [search]);

  const load = useCallback(
    async (append = false, silent = false) => {
      const request = requestRef.current + 1;
      requestRef.current = request;
      if (!silent) {
        if (append) setLoadingMore(true);
        else setLoading(true);
      }
      try {
        const next = await platformAPI.gateway.listCallLogs({
          limit: PAGE_SIZE,
          before: append ? logsRef.current.at(-1)?.startedAt : undefined,
          status,
          providerId: providerId === "all" ? undefined : providerId,
          search: debouncedSearch || undefined,
        });
        if (requestRef.current !== request) return;
        setLogs((current) => {
          const result = append ? [...current, ...next] : next;
          logsRef.current = result;
          return result;
        });
        setHasMore(next.length === PAGE_SIZE);
      } catch (error) {
        if (!silent) {
          toast.error(
            error instanceof Error
              ? error.message
              : t("gateway.logs.loadFailed"),
          );
        }
      } finally {
        if (requestRef.current === request) {
          setLoading(false);
          setLoadingMore(false);
        }
      }
    },
    [debouncedSearch, platformAPI, providerId, status, t],
  );

  useEffect(() => {
    void load(false);
    const timer = window.setInterval(() => void load(false, true), 5_000);
    return () => window.clearInterval(timer);
  }, [load]);

  const totals = useMemo(
    () =>
      logs.reduce(
        (result, log) => ({
          calls: result.calls + 1,
          tokens: result.tokens + log.totalTokens,
          failed: result.failed + (log.status === "failed" ? 1 : 0),
        }),
        { calls: 0, tokens: 0, failed: 0 },
      ),
    [logs],
  );

  const clearLogs = async () => {
    setClearing(true);
    try {
      await platformAPI.gateway.clearCallLogs();
      setClearOpen(false);
      logsRef.current = [];
      setLogs([]);
      setHasMore(false);
      toast.success(t("gateway.logs.cleared"));
    } catch (error) {
      toast.error(
        error instanceof Error ? error.message : t("gateway.logs.clearFailed"),
      );
    } finally {
      setClearing(false);
    }
  };

  return (
    <>
      <Card>
        <CardHeader className="space-y-4">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div>
              <CardTitle className="text-xl">
                {t("gateway.logs.title")}
              </CardTitle>
              <p className="mt-1 text-xs text-muted-foreground">
                {t("gateway.logs.safeDescription")}
              </p>
            </div>
            <div className="flex items-center gap-2">
              <Button
                variant="outline"
                size="icon"
                aria-label={t("common.refresh")}
                onClick={() => void load(false)}
              >
                <RefreshCw className="h-4 w-4" />
              </Button>
              <Button
                variant="outline"
                size="icon"
                aria-label={t("gateway.logs.clear")}
                disabled={logs.length === 0}
                onClick={() => setClearOpen(true)}
              >
                <Trash2 className="h-4 w-4" />
              </Button>
            </div>
          </div>
          <div className="grid gap-2 md:grid-cols-[minmax(180px,1fr)_180px_200px]">
            <Input
              value={search}
              onChange={(event) => setSearch(event.target.value)}
              placeholder={t("gateway.logs.search")}
            />
            <Select
              value={status}
              onValueChange={(value) =>
                setStatus(value as GatewayCallStatus | "all")
              }
            >
              <SelectTrigger>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="all">
                  {t("gateway.logs.allStatus")}
                </SelectItem>
                {(["running", "succeeded", "failed", "cancelled"] as const).map(
                  (value) => (
                    <SelectItem key={value} value={value}>
                      {t(`gateway.logs.status.${value}`)}
                    </SelectItem>
                  ),
                )}
              </SelectContent>
            </Select>
            <Select value={providerId} onValueChange={setProviderId}>
              <SelectTrigger>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="all">
                  {t("gateway.logs.allProviders")}
                </SelectItem>
                {providers.map((provider) => (
                  <SelectItem key={provider.id} value={provider.id}>
                    {provider.name}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
          <div className="flex gap-4 text-xs text-muted-foreground">
            <span>{t("gateway.logs.callCount", { count: totals.calls })}</span>
            <span>
              {t("gateway.logs.tokenCount", { count: totals.tokens })}
            </span>
            <span>
              {t("gateway.logs.failureCount", { count: totals.failed })}
            </span>
          </div>
        </CardHeader>
        <CardContent>
          {loading ? (
            <div className="space-y-2">
              {Array.from({ length: 6 }).map((_, index) => (
                <Skeleton key={index} className="h-10 w-full" />
              ))}
            </div>
          ) : logs.length === 0 ? (
            <p className="py-8 text-center text-sm text-muted-foreground">
              {t("gateway.logs.empty")}
            </p>
          ) : (
            <div className="overflow-x-auto">
              <table className="w-full min-w-[980px] text-left text-sm">
                <thead className="border-b text-xs text-muted-foreground">
                  <tr>
                    <th className="px-2 py-2 font-medium">
                      {t("gateway.logs.time")}
                    </th>
                    <th className="px-2 py-2 font-medium">
                      {t("gateway.logs.statusLabel")}
                    </th>
                    <th className="px-2 py-2 font-medium">
                      {t("gateway.logs.provider")}
                    </th>
                    <th className="px-2 py-2 font-medium">
                      {t("gateway.logs.model")}
                    </th>
                    <th className="px-2 py-2 font-medium">
                      {t("gateway.logs.protocol")}
                    </th>
                    <th className="px-2 py-2 text-right font-medium">Token</th>
                    <th className="px-2 py-2 text-right font-medium">TTFT</th>
                    <th className="px-2 py-2 text-right font-medium">
                      {t("gateway.logs.duration")}
                    </th>
                  </tr>
                </thead>
                <tbody className="divide-y">
                  {logs.map((log) => (
                    <tr
                      key={log.id}
                      className="cursor-pointer hover:bg-muted/40"
                      onClick={() => setSelected(log)}
                    >
                      <td className="whitespace-nowrap px-2 py-3 text-xs text-muted-foreground">
                        {formatTime(log.startedAt)}
                      </td>
                      <td className="px-2 py-3">
                        <StatusBadge status={log.status} />
                      </td>
                      <td className="max-w-40 truncate px-2 py-3">
                        {log.providerName}
                      </td>
                      <td className="max-w-56 px-2 py-3 font-mono text-xs">
                        <div className="truncate">{log.requestedModel}</div>
                        {log.requestedModel !== log.upstreamModel && (
                          <div className="truncate text-muted-foreground">
                            → {log.upstreamModel}
                          </div>
                        )}
                      </td>
                      <td className="whitespace-nowrap px-2 py-3 text-xs text-muted-foreground">
                        {shortProtocol(log.clientProtocol)} →{" "}
                        {shortProtocol(log.upstreamProtocol)}
                      </td>
                      <td className="px-2 py-3 text-right tabular-nums">
                        {formatNumber(log.totalTokens)}
                      </td>
                      <td className="px-2 py-3 text-right tabular-nums">
                        {formatMs(log.firstTokenMs)}
                      </td>
                      <td className="px-2 py-3 text-right tabular-nums">
                        {formatMs(log.durationMs)}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
              {hasMore && (
                <div className="flex justify-center pt-4">
                  <Button
                    variant="outline"
                    disabled={loadingMore}
                    onClick={() => void load(true)}
                  >
                    {t("gateway.logs.loadMore")}
                  </Button>
                </div>
              )}
            </div>
          )}
        </CardContent>
      </Card>

      <LogDetail log={selected} onClose={() => setSelected(null)} />
      <AlertDialog open={clearOpen} onOpenChange={setClearOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("gateway.logs.clearTitle")}</AlertDialogTitle>
            <AlertDialogDescription>
              {t("gateway.logs.clearDescription")}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("common.cancel")}</AlertDialogCancel>
            <AlertDialogAction
              disabled={clearing}
              onClick={() => void clearLogs()}
            >
              {t("common.delete")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
}

function LogDetail({
  log,
  onClose,
}: {
  log: GatewayCallLog | null;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  if (!log) return null;
  const copyRequestId = async () => {
    await navigator.clipboard.writeText(log.requestId);
    toast.success(t("gateway.copied"));
  };
  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="max-w-xl">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            {t("gateway.logs.detail")}
            <StatusBadge status={log.status} />
          </DialogTitle>
        </DialogHeader>
        <dl className="grid grid-cols-[140px_minmax(0,1fr)] gap-x-4 gap-y-3 text-sm">
          <Detail label="Request ID">
            <div className="flex min-w-0 items-center gap-2">
              <code className="truncate text-xs">{log.requestId}</code>
              <Button
                variant="ghost"
                size="icon"
                className="h-7 w-7"
                onClick={() => void copyRequestId()}
              >
                <Copy className="h-3.5 w-3.5" />
              </Button>
            </div>
          </Detail>
          <Detail label={t("gateway.logs.time")}>
            {formatDateTime(log.startedAt)}
          </Detail>
          <Detail label={t("gateway.logs.provider")}>{log.providerName}</Detail>
          <Detail label={t("gateway.logs.model")}>
            <code className="text-xs">
              {log.requestedModel} → {log.upstreamModel}
            </code>
          </Detail>
          <Detail label={t("gateway.logs.protocol")}>
            {log.clientProtocol} → {log.upstreamProtocol}
          </Detail>
          <Detail label="HTTP">{log.httpStatus ?? "—"}</Detail>
          <Detail label={t("gateway.logs.streaming")}>
            {log.streaming ? t("gateway.logs.yes") : t("gateway.logs.no")}
          </Detail>
          <Detail label={t("gateway.logs.tokens")}>
            {t("gateway.logs.tokenDetail", {
              input: formatNumber(log.inputTokens),
              output: formatNumber(log.outputTokens),
              cacheRead: formatNumber(log.cacheReadTokens),
              cacheWrite: formatNumber(log.cacheWriteTokens),
              total: formatNumber(log.totalTokens),
            })}
          </Detail>
          <Detail label="TTFT">{formatMs(log.firstTokenMs)}</Detail>
          <Detail label={t("gateway.logs.duration")}>
            {formatMs(log.durationMs)}
          </Detail>
          {log.error && (
            <Detail label={t("gateway.logs.error")}>
              <span className="text-destructive">{log.error}</span>
            </Detail>
          )}
        </dl>
      </DialogContent>
    </Dialog>
  );
}

function Detail({ label, children }: { label: string; children: ReactNode }) {
  return (
    <>
      <dt className="text-muted-foreground">{label}</dt>
      <dd className="min-w-0">{children}</dd>
    </>
  );
}

function StatusBadge({ status }: { status: GatewayCallStatus }) {
  const { t } = useTranslation();
  return (
    <Badge
      variant={
        status === "failed"
          ? "destructive"
          : status === "succeeded"
            ? "secondary"
            : "outline"
      }
    >
      {t(`gateway.logs.status.${status}`)}
    </Badge>
  );
}

function formatTime(value: number) {
  return new Intl.DateTimeFormat(undefined, {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  }).format(value);
}

function formatDateTime(value: number) {
  return new Intl.DateTimeFormat(undefined, {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  }).format(value);
}

function formatNumber(value: number) {
  return new Intl.NumberFormat().format(value);
}

function formatMs(value: number | null | undefined) {
  return value == null ? "—" : `${formatNumber(value)} ms`;
}

function shortProtocol(value: string) {
  if (value === "openai-compatible") return "Chat";
  if (value === "openai-responses") return "Responses";
  return "Anthropic";
}
