import { create, StoreApi, UseBoundStore } from "zustand";
import type { PlatformAPI } from "@mcp_link/shared";
import type { AgentSessionStoreState } from "@mcp_link/shared";

export const createSessionStore = (
  getPlatformAPI: () => PlatformAPI,
): UseBoundStore<StoreApi<AgentSessionStoreState>> => {
  let sessionRequestCounter = 0;
  let loadTimer: ReturnType<typeof setTimeout> | undefined;
  const messagePageSize = 50;

  return create<AgentSessionStoreState>((set, get) => {
    const scheduleLoad = () => {
      if (loadTimer) clearTimeout(loadTimer);
      loadTimer = setTimeout(() => {
        void get().loadSessions();
      }, 250);
    };

    return {
      plugins: [],
      query: "",
      sessions: [],
      loadedAgentIds: [],
      loadingAgentIds: [],
      selectedKey: null,
      selected: null,
      isLoading: false,
      isLoadingSession: false,
      error: null,

      loadPlugins: async () => {
        try {
          const result = await getPlatformAPI().agents.list();
          set({ plugins: result });
        } catch (error) {
          set({
            error:
              error instanceof Error ? error.message : "Failed to load agents",
          });
        }
      },

      loadSessions: async () => {
        const agentIds = get().loadedAgentIds;
        await Promise.all(
          agentIds.map((agentId) => get().loadAgentSessions(agentId, false)),
        );
      },

      loadAgentSessions: async (agentId, refresh = true) => {
        set((state) => ({
          isLoading: true,
          loadingAgentIds: state.loadingAgentIds.includes(agentId)
            ? state.loadingAgentIds
            : [...state.loadingAgentIds, agentId],
        }));
        try {
          const { query } = get();
          const result = await getPlatformAPI().agents.sessions.list({
            agentId,
            query: query.trim() || undefined,
            limit: 1000,
            refresh,
          });
          set((state) => ({
            sessions: [
              ...state.sessions.filter((item) => item.agentId !== agentId),
              ...result,
            ],
            loadedAgentIds: state.loadedAgentIds.includes(agentId)
              ? state.loadedAgentIds
              : [...state.loadedAgentIds, agentId],
          }));
        } catch (error) {
          set({
            error:
              error instanceof Error
                ? error.message
                : "Failed to load sessions",
          });
        } finally {
          set((state) => {
            const loadingAgentIds = state.loadingAgentIds.filter(
              (item) => item !== agentId,
            );
            return {
              loadingAgentIds,
              isLoading: loadingAgentIds.length > 0,
            };
          });
        }
      },

      refreshSessions: () => {
        const agentIds = get().loadedAgentIds;
        void Promise.all(
          agentIds.map((agentId) => get().loadAgentSessions(agentId, true)),
        );
      },

      selectSession: async (summary) => {
        const requestId = (sessionRequestCounter += 1);
        set({ selectedKey: summary.id, isLoadingSession: true });
        try {
          const session = await getPlatformAPI().agents.sessions.get(
            summary.agentId,
            summary.nativeId,
            { limit: messagePageSize },
          );
          if (requestId !== sessionRequestCounter) return;
          set({ selected: session, isLoadingSession: false });
        } catch (error) {
          if (requestId !== sessionRequestCounter) return;
          set({
            selected: null,
            isLoadingSession: false,
            error:
              error instanceof Error ? error.message : "Failed to load session",
          });
        }
      },

      loadOlderMessages: async () => {
        const current = get().selected;
        if (!current?.hasMoreMessages || current.messageCursor == null) {
          return false;
        }
        const selectedId = current.id;
        try {
          const page = await getPlatformAPI().agents.sessions.get(
            current.agentId,
            current.nativeId,
            { before: current.messageCursor, limit: messagePageSize },
          );
          const latest = get().selected;
          if (!latest || latest.id !== selectedId) return false;
          const existing = new Set(latest.messages.map((item) => item.id));
          const older = page.messages.filter((item) => !existing.has(item.id));
          set({
            selected: {
              ...latest,
              messages: [...older, ...latest.messages],
              messageCursor: page.messageCursor,
              hasMoreMessages: page.hasMoreMessages,
            },
          });
          return older.length > 0;
        } catch (error) {
          set({
            error:
              error instanceof Error
                ? error.message
                : "Failed to load older messages",
          });
          return false;
        }
      },

      loadMessagesThrough: async (messageId) => {
        const initial = get().selected;
        if (!initial) return false;
        if (initial.messages.some((item) => item.id === messageId)) return true;
        const selectedId = initial.id;
        let pageCount = 0;
        while (pageCount < 10_000) {
          const current = get().selected;
          if (
            !current ||
            current.id !== selectedId ||
            !current.hasMoreMessages ||
            current.messageCursor == null
          ) {
            return false;
          }
          const previousCursor = current.messageCursor;
          try {
            const page = await getPlatformAPI().agents.sessions.get(
              current.agentId,
              current.nativeId,
              { before: previousCursor, limit: 200 },
            );
            const latest = get().selected;
            if (!latest || latest.id !== selectedId) return false;
            const existing = new Set(latest.messages.map((item) => item.id));
            const older = page.messages.filter(
              (item) => !existing.has(item.id),
            );
            const merged = [...older, ...latest.messages];
            set({
              selected: {
                ...latest,
                messages: merged,
                messageCursor: page.messageCursor,
                hasMoreMessages: page.hasMoreMessages,
              },
            });
            if (merged.some((item) => item.id === messageId)) return true;
            if (
              page.messageCursor == null ||
              page.messageCursor === previousCursor
            ) {
              return false;
            }
          } catch (error) {
            set({
              error:
                error instanceof Error
                  ? error.message
                  : "Failed to load message",
            });
            return false;
          }
          pageCount += 1;
        }
        return false;
      },

      deleteSession: async (summary) => {
        const result = await getPlatformAPI().agents.sessions.delete(
          summary.agentId,
          summary.nativeId,
        );
        if (get().selectedKey === summary.id) {
          get().clearSelected();
        }
        set((state) => ({
          sessions: state.sessions.filter((item) => item.id !== summary.id),
        }));
        void get().loadSessions();
        return result;
      },

      renameSession: async (summary, title) => {
        const trimmed = title.trim();
        await getPlatformAPI().agents.sessions.rename(
          summary.agentId,
          summary.nativeId,
          trimmed,
        );
        set((state) => ({
          sessions: state.sessions.map((item) =>
            item.id === summary.id ? { ...item, title: trimmed } : item,
          ),
          selected:
            state.selected && state.selected.id === summary.id
              ? { ...state.selected, title: trimmed }
              : state.selected,
        }));
        void get().loadSessions();
      },

      setQuery: (value) => {
        set({ query: value });
        scheduleLoad();
      },

      setSelected: (session) => set({ selected: session }),

      clearSelected: () => {
        sessionRequestCounter += 1;
        set({
          selectedKey: null,
          selected: null,
          isLoadingSession: false,
        });
      },

      clearError: () => set({ error: null }),

      clearStore: () => {
        if (loadTimer) clearTimeout(loadTimer);
        sessionRequestCounter += 1;
        set({
          plugins: [],
          query: "",
          sessions: [],
          loadedAgentIds: [],
          loadingAgentIds: [],
          selectedKey: null,
          selected: null,
          isLoading: false,
          isLoadingSession: false,
          error: null,
        });
      },
    };
  });
};
