import React, {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";

import { MarketView } from "@/renderer/components/common/MarketView";
import { usePlatformAPI } from "@/renderer/platform-api";
import {
  fetchRegistryServers,
  fetchMcpmServers,
  fetchMcpsLiveServers,
  fetchSmitheryServers,
  registryServerToConfig,
  type RegistryServerEntry,
  type RegistrySource,
} from "@/renderer/services/registry-service";
import { getEnabledMcpMarketSources } from "@/renderer/services/market-source-service";
import { registryMarketItem } from "@/renderer/utils/market-card-items";

const OFFICIAL_META_KEY = "io.modelcontextprotocol.registry/official";
const PAGE_SIZE = 24;
const REGISTRY_PAGE_LIMIT = 100;
const REGISTRY_SEARCH_BATCH_PAGES = 5;

interface RegistryMetadata {
  nextCursor?: string;
  count?: number;
  fetchedCount?: number;
  hasMore?: boolean;
  currentPage?: number;
  pages?: number;
  totalPages?: number;
}

const RegistryMarket: React.FC = () => {
  const { t } = useTranslation();
  const platformAPI = usePlatformAPI();
  const [source, setSource] = useState<RegistrySource>("official");
  const [sources, setSources] = useState(() =>
    getEnabledMcpMarketSources(null),
  );
  const [servers, setServers] = useState<RegistryServerEntry[]>([]);
  const [metadata, setMetadata] = useState<RegistryMetadata>({});
  const [search, setSearch] = useState("");
  const [currentPage, setCurrentPage] = useState(1);
  const [isLoading, setIsLoading] = useState(true);
  const [isLoadingMore, setIsLoadingMore] = useState(false);
  const [addingName, setAddingName] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const loadSeq = useRef(0);
  const metadataRef = useRef<RegistryMetadata>({});

  const updateMetadata = useCallback((value: RegistryMetadata) => {
    metadataRef.current = value;
    setMetadata(value);
  }, []);

  const clearResults = useCallback(
    (invalidate = false) => {
      if (invalidate) loadSeq.current += 1;
      setServers([]);
      updateMetadata({});
      setError(null);
      setIsLoadingMore(false);
    },
    [updateMetadata],
  );

  const loadServers = useCallback(
    async (append = false) => {
      const seq = loadSeq.current + 1;
      loadSeq.current = seq;

      if (!sources.some((item) => item.id === source)) {
        clearResults();
        setIsLoading(false);
        setIsLoadingMore(false);
        return;
      }

      const previousMetadata = metadataRef.current;
      const cursor = append ? previousMetadata.nextCursor : "";
      if (append) {
        if (source === "official" && !cursor) {
          return;
        }
        if (source === "mcpm" || !previousMetadata.hasMore) {
          return;
        }
      }

      if (append) {
        setIsLoadingMore(true);
      } else {
        setIsLoading(true);
      }
      setError(null);
      try {
        let result;
        if (source === "smithery") {
          result = await fetchSmitheryServers({
            search,
            page: append ? (previousMetadata.currentPage ?? 1) + 1 : 1,
            pageSize: 100,
          });
        } else if (source === "mcps-live") {
          result = await fetchMcpsLiveServers({
            search,
            page: append ? (previousMetadata.currentPage ?? 1) + 1 : 1,
            maxPages: 1,
          });
        } else if (source === "mcpm") {
          result = await fetchMcpmServers({ search });
        } else {
          const searchBatchPages = search.trim()
            ? REGISTRY_SEARCH_BATCH_PAGES
            : 1;
          result = await fetchRegistryServers({
            search,
            cursor,
            limit: REGISTRY_PAGE_LIMIT,
            fetchAll: searchBatchPages > 1,
            maxPages: searchBatchPages,
          });
        }

        if (loadSeq.current !== seq) return;
        if (append && source !== "mcpm") {
          setServers((current) => [...current, ...(result.servers ?? [])]);
          const previous = metadataRef.current;
          updateMetadata({
            ...(result.metadata ?? {}),
            fetchedCount:
              (previous.fetchedCount ?? 0) +
              (result.metadata?.fetchedCount ?? result.servers?.length ?? 0),
            pages:
              source === "official"
                ? (previous.pages ?? 0) + (result.metadata?.pages ?? 0)
                : result.metadata?.pages,
            totalPages: result.metadata?.totalPages ?? previous.totalPages,
          });
        } else {
          setServers(result.servers ?? []);
          updateMetadata(result.metadata ?? {});
        }
      } catch (error) {
        if (loadSeq.current !== seq) return;
        setError(error instanceof Error ? error.message : String(error));
      } finally {
        if (loadSeq.current === seq) {
          if (append) {
            setIsLoadingMore(false);
          } else {
            setIsLoading(false);
          }
        }
      }
    },
    [clearResults, search, source, sources, updateMetadata],
  );

  useEffect(() => {
    let cancelled = false;
    platformAPI.settings
      .get()
      .then((settings) => {
        if (cancelled) return;
        const enabledSources = getEnabledMcpMarketSources(settings);
        setSources(enabledSources);
        if (
          enabledSources.length > 0 &&
          !enabledSources.some((item) => item.id === source)
        ) {
          setSource(enabledSources[0].id);
          setCurrentPage(1);
        }
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [platformAPI, source]);

  useEffect(() => {
    const timer = setTimeout(() => {
      void loadServers();
    }, 250);
    return () => clearTimeout(timer);
  }, [loadServers]);

  const visibleServers = useMemo(() => {
    const latestByName = new Map<string, RegistryServerEntry>();
    for (const entry of servers) {
      const name = entry.server?.name;
      if (!name) continue;
      const official = entry._meta?.[OFFICIAL_META_KEY];
      if (official?.isLatest === false && latestByName.has(name)) continue;
      latestByName.set(name, entry);
    }
    return [...latestByName.values()];
  }, [servers]);

  const canLoadMore =
    source !== "mcpm" &&
    (source === "official" ||
      source === "smithery" ||
      source === "mcps-live") &&
    Boolean(metadata.hasMore);
  const loadedPages = Math.ceil(visibleServers.length / PAGE_SIZE);
  const totalPages = Math.max(
    1,
    loadedPages + (canLoadMore ? 1 : 0),
    canLoadMore && visibleServers.length === 0 ? currentPage + 1 : 1,
  );
  const safeCurrentPage = Math.min(currentPage, totalPages);
  const pagedServers = visibleServers.slice(
    (safeCurrentPage - 1) * PAGE_SIZE,
    safeCurrentPage * PAGE_SIZE,
  );

  useEffect(() => {
    if (source === "mcpm" || isLoading || isLoadingMore || !canLoadMore) {
      return;
    }

    const shouldFetchNextPage =
      safeCurrentPage > loadedPages ||
      (visibleServers.length > 0 &&
        safeCurrentPage >= Math.max(2, loadedPages - 1));

    if (shouldFetchNextPage) {
      void loadServers(true);
    }
  }, [
    isLoading,
    isLoadingMore,
    canLoadMore,
    loadedPages,
    loadServers,
    safeCurrentPage,
    source,
    visibleServers.length,
  ]);

  const handleAdd = async (entry: RegistryServerEntry) => {
    const config = registryServerToConfig(entry.server);
    if (!config) {
      toast.error(t("registry.unsupported"));
      return;
    }

    setAddingName(entry.server.name);
    try {
      await platformAPI.servers.create({ type: "config", config });
      toast.success(t("registry.added"));
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    } finally {
      setAddingName(null);
    }
  };

  const items = pagedServers.map((entry) =>
    registryMarketItem(
      entry,
      source,
      addingName === entry.server.name,
      handleAdd,
      t,
    ),
  );

  return (
    <MarketView
      sources={[
        ...sources.map((item) => ({
          value: item.id,
          label:
            item.id === "official" ? t("registry.sourceOfficial") : item.label,
        })),
      ]}
      source={source}
      onSourceChange={(value) => {
        setSource(value);
        setCurrentPage(1);
        clearResults(true);
      }}
      search={search}
      onSearchChange={(value) => {
        setSearch(value);
        setCurrentPage(1);
        clearResults(true);
      }}
      onRefresh={() => void loadServers()}
      searchPlaceholder={t("registry.searchPlaceholder")}
      isLoading={isLoading || (isLoadingMore && pagedServers.length === 0)}
      summary={t("registry.countSummary", {
        latest: visibleServers.length,
        total: metadata.fetchedCount ?? servers.length,
      })}
      partialLabel={metadata.hasMore ? t("registry.partial") : undefined}
      error={error}
      emptyTitle={t("registry.empty")}
      items={items}
      currentPage={safeCurrentPage}
      totalPages={totalPages}
      onPageChange={setCurrentPage}
      previousLabel={t("registry.previous")}
      nextLabel={t("registry.next")}
    />
  );
};

export default RegistryMarket;
