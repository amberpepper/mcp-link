export interface CursorPaginationOptions {
  cursor?: string;
  limit?: number;
}
export interface CursorPaginationResult<T> {
  items: T[];
  total: number;
  nextCursor?: string;
  hasMore: boolean;
}
export interface CursorData {
  timestamp: number;
  id: string;
}
