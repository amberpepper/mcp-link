import { useState, useEffect, useCallback, useMemo } from "react";
import {
  RequestLogEntry,
  WordCloudItem,
  ActivityLogEntry,
  ActivityType,
  ActivitySession,
  ActivityItem,
} from "@mcp_link/shared";
import { usePlatformAPI } from "@/renderer/platform-api";

interface ActivityDataParams {
  heatmapDays?: number;
  selectedDate?: string;
  refreshTrigger?: number;
}

const SESSION_TIME_WINDOW_MS = 30 * 60 * 1000;

const ACTIVITY_TYPES: ActivityType[] = [
  "ToolDiscovery",
  "ToolExecute",
  "CallTool",
  "GetPrompt",
  "ReadResource",
];

interface ActivityDataResult {
  wordCloudData: WordCloudItem[];
  activityItems: ActivityItem[];
  loading: boolean;
  error: string | null;
  refetch: () => Promise<void>;
}
const toActivityLogEntry = (log: RequestLogEntry): ActivityLogEntry | null => {
  const type = log.requestType as ActivityType;
  if (!ACTIVITY_TYPES.includes(type)) {
    return null;
  }

  const base: ActivityLogEntry = {
    id: log.id,
    timestamp: log.timestamp,
    clientId: log.clientId,
    clientName: log.clientName,
    type,
    status: log.responseStatus,
    duration: log.duration,
    errorMessage: log.errorMessage,
  };

  if (type === "ToolDiscovery") {
    const params = log.requestParams || {};
    const response = log.responseData;

    let discoveredTools: ActivityLogEntry["discoveredTools"] = [];
    if (Array.isArray(response?.tools)) {
      discoveredTools = response.tools.map((item: any) => ({
        toolKey: item.toolKey || item.toolName || "",
        toolName: item.toolName || "",
        serverName: item.serverName || log.serverName || "",
        relevance: item.relevance || 1,
      }));
    } else if (response?.content?.[0]?.text) {
      try {
        const parsed = JSON.parse(response.content[0].text);
        if (Array.isArray(parsed)) {
          discoveredTools = parsed.map((item: any) => ({
            toolKey: item.toolKey || "",
            toolName: item.toolName || "",
            serverName: item.serverName || "",
            relevance: item.relevance || 0,
          }));
        }
      } catch {
        // JSON parse error - ignore
      }
    }

    return {
      ...base,
      query: params.query || [],
      context: params.context,
      discoveredTools,
    };
  }

  if (type === "ToolExecute") {
    const params = log.requestParams || {};
    const toolKey = params.toolKey || "";
    const toolName = params.toolName || "";

    return {
      ...base,
      toolKey,
      toolName,
      serverName: log.serverName,
      arguments: params.arguments,
      responseData: log.responseData,
    };
  }

  if (type === "CallTool") {
    const params = log.requestParams || {};
    const toolName = params.name || "";

    return {
      ...base,
      toolName,
      serverName: log.serverName,
      arguments: params.arguments,
      responseData: log.responseData,
    };
  }

  if (type === "GetPrompt") {
    const params = log.requestParams || {};
    const promptName = params.name || "";

    return {
      ...base,
      promptName,
      serverName: log.serverName,
      arguments: params.arguments,
      responseData: log.responseData,
    };
  }

  if (type === "ReadResource") {
    const params = log.requestParams || {};
    const resourceUri = params.uri || "";

    return {
      ...base,
      resourceUri,
      serverName: log.serverName,
      responseData: log.responseData,
    };
  }

  return null;
};
const getDateString = (timestamp: number): string => {
  const date = new Date(timestamp);
  return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`;
};
export const useActivityData = (
  params: ActivityDataParams,
): ActivityDataResult => {
  const platformAPI = usePlatformAPI();
  const { heatmapDays = 30, selectedDate, refreshTrigger } = params;

  const [rawLogs, setRawLogs] = useState<RequestLogEntry[]>([]);
  const [loading, setLoading] = useState<boolean>(true);
  const [error, setError] = useState<string | null>(null);

  const fetchData = useCallback(async () => {
    setLoading(true);
    setError(null);

    try {
      const endDate = new Date();
      const startDate = new Date();
      startDate.setDate(startDate.getDate() - heatmapDays);

      const result = await platformAPI.logs.query({
        startDate,
        endDate,
        limit: 1000,
      });

      const allLogs = (result.logs || [])
        .filter((log) =>
          ACTIVITY_TYPES.includes(log.requestType as ActivityType),
        )
        .sort((a, b) => b.timestamp - a.timestamp);

      setRawLogs(allLogs);
    } catch (err) {
      console.error("Failed to fetch activity data:", err);
      setError("Failed to fetch activity data");
      setRawLogs([]);
    } finally {
      setLoading(false);
    }
  }, [platformAPI, heatmapDays, refreshTrigger]);

  useEffect(() => {
    fetchData();
  }, [fetchData]);

  const wordCloudData = useMemo((): WordCloudItem[] => {
    if (!selectedDate) return [];

    const wordCount = new Map<string, number>();

    for (const log of rawLogs) {
      if (getDateString(log.timestamp) !== selectedDate) continue;
      if (log.requestType !== "CallTool" && log.requestType !== "ToolExecute")
        continue;
      const toolName = log.requestParams?.name || log.requestParams?.toolName;
      if (typeof toolName === "string" && toolName.trim()) {
        const normalized = toolName.trim();
        wordCount.set(normalized, (wordCount.get(normalized) || 0) + 1);
      }
    }

    return Array.from(wordCount.entries())
      .map(([text, value]) => ({ text, value }))
      .sort((a, b) => b.value - a.value);
  }, [rawLogs, selectedDate]);

  const activityItems = useMemo((): ActivityItem[] => {
    if (!selectedDate) return [];

    const entries = rawLogs
      .filter((log) => getDateString(log.timestamp) === selectedDate)
      .map(toActivityLogEntry)
      .filter((entry): entry is ActivityLogEntry => entry !== null)
      .sort((a, b) => a.timestamp - b.timestamp);

    const items: ActivityItem[] = [];
    const usedExecuteIds = new Set<string>();

    const discoveries = entries.filter((e) => e.type === "ToolDiscovery");
    const executes = entries.filter(
      (e) => e.type === "ToolExecute" || e.type === "CallTool",
    );
    const standaloneEntries = entries.filter(
      (e) =>
        e.type === "CallTool" ||
        e.type === "GetPrompt" ||
        e.type === "ReadResource",
    );

    for (const discovery of discoveries) {
      const relatedExecutes: ActivityLogEntry[] = [];
      const discoveredToolKeys = new Set(
        discovery.discoveredTools?.map((t) => t.toolKey) || [],
      );

      for (const exec of executes) {
        if (usedExecuteIds.has(exec.id)) continue;

        if (exec.clientId !== discovery.clientId) continue;

        const timeDiff = exec.timestamp - discovery.timestamp;
        if (timeDiff < 0 || timeDiff > SESSION_TIME_WINDOW_MS) continue;

        const executedToolKey = exec.toolKey || exec.toolName;
        if (executedToolKey && discoveredToolKeys.has(executedToolKey)) {
          relatedExecutes.push(exec);
          usedExecuteIds.add(exec.id);
        }
      }

      const session: ActivitySession = {
        id: discovery.id,
        timestamp: discovery.timestamp,
        clientId: discovery.clientId,
        clientName: discovery.clientName,
        discovery,
        executions: relatedExecutes.sort((a, b) => a.timestamp - b.timestamp),
      };

      items.push({ type: "session", session });
    }

    for (const exec of executes) {
      if (!usedExecuteIds.has(exec.id)) {
        items.push({ type: "standalone", entry: exec });
      }
    }

    for (const entry of standaloneEntries) {
      items.push({ type: "standalone", entry });
    }

    return items.sort((a, b) => {
      const tsA =
        a.type === "session" ? a.session.timestamp : a.entry.timestamp;
      const tsB =
        b.type === "session" ? b.session.timestamp : b.entry.timestamp;
      return tsB - tsA;
    });
  }, [rawLogs, selectedDate]);

  return {
    wordCloudData,
    activityItems,
    loading,
    error,
    refetch: fetchData,
  };
};
