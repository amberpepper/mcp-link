export interface HeatmapCell {
  date: string; // YYYY-MM-DD
  hour: number; // 0-23
  count: number;
}
export interface HeatmapData {
  cells: HeatmapCell[];
  maxCount: number;
}
export interface WordCloudItem {
  text: string;
  value: number;
}
export type ActivityType =
  | "ToolDiscovery"
  | "ToolExecute"
  | "CallTool"
  | "GetPrompt"
  | "ReadResource";
export interface ActivityLogEntry {
  id: string;
  timestamp: number;
  clientId: string;
  clientName: string;
  type: ActivityType;
  query?: string[];
  context?: string;
  discoveredTools?: {
    toolKey: string;
    toolName: string;
    serverName: string;
    relevance: number;
  }[];
  toolKey?: string;
  toolName?: string;
  serverName?: string;
  arguments?: Record<string, unknown>;
  status: "success" | "error";
  duration: number;
  errorMessage?: string;
  responseData?: unknown;

  promptName?: string;

  resourceUri?: string;
}
export interface DailyActivitySummary {
  date: string; // YYYY-MM-DD
  totalCount: number;
  discoveryCount: number;
  executeCount: number;
  successCount: number;
  errorCount: number;
  topQueries: WordCloudItem[];
}
export interface ActivitySession {
  id: string;
  timestamp: number;
  clientId: string;
  clientName: string;
  discovery: ActivityLogEntry;
  executions: ActivityLogEntry[];
}
export type ActivityItem =
  | { type: "session"; session: ActivitySession }
  | { type: "standalone"; entry: ActivityLogEntry };
