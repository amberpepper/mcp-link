import { CursorPaginationOptions, CursorPaginationResult } from "./pagination";
export interface RequestLogEntry {
  id: string;
  timestamp: number;
  clientId: string;
  clientName: string;
  serverId: string;
  serverName: string;
  requestType: string;
  requestParams: any;
  responseStatus: "success" | "error";
  responseData?: any;
  duration: number;
  errorMessage?: string;
}
export type RequestLogEntryInput = Omit<RequestLogEntry, "id" | "timestamp">;
export interface RequestLogFilters {
  clientId?: string;
  serverId?: string;
  requestType?: string;
  startDate?: Date;
  endDate?: Date;
  responseStatus?: "success" | "error";
}
export interface RequestLogQueryOptions
  extends RequestLogFilters, CursorPaginationOptions {}
export interface RequestLogQueryResult extends CursorPaginationResult<RequestLogEntry> {
  logs: RequestLogEntry[];
}
export interface McpManagerRequestLogEntry {
  timestamp: string;
  requestType: string;
  params: any;
  result: "success" | "error";
  errorMessage?: string;
  response?: any;
  duration: number;
  clientId: string;
}
export const AGGREGATOR_SERVER_ID = "mcp-link-aggregator";
export const AGGREGATOR_SERVER_NAME = "MCP Link Aggregator";
