/**
 * Log management domain API
 */

import type {
  CursorPaginationOptions,
  CursorPaginationResult,
} from "../../pagination";
import type { RequestLogEntry } from "../../log-types";

// Alias for API compatibility
export type LogEntry = RequestLogEntry;
interface LogFilters {
  clientId?: string;
  serverId?: string;
  requestType?: string;
  startDate?: Date;
  endDate?: Date;
  responseStatus?: "success" | "error";
}
export interface LogQueryOptions extends LogFilters, CursorPaginationOptions {}
export interface LogQueryResult extends CursorPaginationResult<LogEntry> {
  logs: LogEntry[];
}

export interface LogAPI {
  query(options?: LogQueryOptions): Promise<LogQueryResult>;
}
