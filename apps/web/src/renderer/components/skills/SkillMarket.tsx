import React, { useCallback, useEffect, useRef, useState } from "react";
import { Link } from "react-router-dom";
import { useTranslation } from "react-i18next";
import { Button } from "@mcp_link/ui";
import { ArrowLeft } from "lucide-react";
import { toast } from "sonner";

import { MarketView } from "@/renderer/components/common/MarketView";
import PageLayout from "@/renderer/components/layout/PageLayout";
import { usePlatformAPI } from "@/renderer/platform-api";
import {
  fetchAnthropicSkills,
  fetchCommunitySkills,
  fetchSkillContent,
  type SkillMarketEntry,
  type SkillMarketSource,
} from "@/renderer/services/skill-market-service";
import { getEnabledSkillMarketSources } from "@/renderer/services/market-source-service";
import {
  safeSkillName,
  skillMarketItem,
} from "@/renderer/utils/market-card-items";
import EmbeddedSkillPage from "./EmbeddedSkillPage";

const PAGE_SIZE = 24;

interface SkillMarketProps {
  embedded?: boolean;
}

const SkillMarket: React.FC<SkillMarketProps> = ({ embedded = false }) => {
  const { t } = useTranslation();
  const platformAPI = usePlatformAPI();
  const [source, setSource] = useState<SkillMarketSource>("community");
  const [sources, setSources] = useState(() =>
    getEnabledSkillMarketSources(null),
  );
  const [skills, setSkills] = useState<SkillMarketEntry[]>([]);
  const [total, setTotal] = useState(0);
  const [totalPages, setTotalPages] = useState(0);
  const [hasMore, setHasMore] = useState(false);
  const [search, setSearch] = useState("");
  const [currentPage, setCurrentPage] = useState(1);
  const [isLoading, setIsLoading] = useState(true);
  const [installingId, setInstallingId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [installedNames, setInstalledNames] = useState<Set<string>>(new Set());
  const loadSeq = useRef(0);

  const loadInstalledSkills = useCallback(async () => {
    try {
      const existing = await platformAPI.skills.list();
      setInstalledNames(new Set(existing.map((skill) => skill.name)));
    } catch {
      // Installed skills are only used to disable duplicate installs.
    }
  }, [platformAPI]);

  const clearResults = useCallback((invalidate = false) => {
    if (invalidate) loadSeq.current += 1;
    setSkills([]);
    setTotal(0);
    setTotalPages(0);
    setHasMore(false);
    setError(null);
  }, []);

  const loadSkills = useCallback(async () => {
    const seq = loadSeq.current + 1;
    loadSeq.current = seq;

    if (!sources.some((item) => item.id === source)) {
      clearResults();
      setIsLoading(false);
      return;
    }

    setIsLoading(true);
    setError(null);
    try {
      let result;
      if (source === "community") {
        result = await fetchCommunitySkills({
          search: search || undefined,
          page: currentPage,
          pageSize: PAGE_SIZE,
        });
      } else {
        result = await fetchAnthropicSkills({
          search: search || undefined,
          page: currentPage,
          pageSize: PAGE_SIZE,
        });
      }

      if (loadSeq.current !== seq) return;
      setSkills(result.skills);
      setTotal(result.total ?? result.skills.length);
      setTotalPages(result.pages ?? 0);
      setHasMore(result.hasMore ?? false);
    } catch (error) {
      if (loadSeq.current !== seq) return;
      setError(error instanceof Error ? error.message : String(error));
    } finally {
      if (loadSeq.current === seq) {
        setIsLoading(false);
      }
    }
  }, [clearResults, currentPage, search, source, sources]);

  useEffect(() => {
    let cancelled = false;
    platformAPI.settings
      .get()
      .then((settings) => {
        if (cancelled) return;
        const enabledSources = getEnabledSkillMarketSources(settings);
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
    void loadInstalledSkills();
  }, [loadInstalledSkills]);

  useEffect(() => {
    const timer = setTimeout(() => {
      void loadSkills();
    }, 300);
    return () => clearTimeout(timer);
  }, [loadSkills]);

  const handleInstall = async (entry: SkillMarketEntry) => {
    setInstallingId(entry.id);
    try {
      const content = await fetchSkillContent(entry);
      if (!content) {
        toast.error(t("skillMarket.noContent"));
        return;
      }

      const name = safeSkillName(entry.name);
      await platformAPI.skills.create({ name, content });
      toast.success(t("skillMarket.installSuccess", { name: entry.name }));
      setInstalledNames((previous) => new Set(previous).add(name));
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    } finally {
      setInstallingId(null);
    }
  };

  const computedTotalPages = Math.max(1, totalPages);
  const safeCurrentPage = Math.min(currentPage, computedTotalPages);
  const items = skills.map((entry) =>
    skillMarketItem(
      entry,
      installedNames.has(safeSkillName(entry.name)),
      installingId === entry.id,
      handleInstall,
      t,
    ),
  );
  const Shell = embedded ? EmbeddedSkillPage : PageLayout;

  return (
    <Shell
      title={
        embedded ? (
          t("skillMarket.title")
        ) : (
          <span className="flex min-w-0 items-center gap-3">
            <Button
              asChild
              type="button"
              variant="ghost"
              size="icon"
              className="h-8 w-8 shrink-0"
            >
              <Link to="/skills" aria-label={t("skillMarket.backToSkills")}>
                <ArrowLeft className="h-4 w-4" />
              </Link>
            </Button>
            <span className="truncate">{t("skillMarket.title")}</span>
          </span>
        )
      }
      contentClassName="flex min-w-0 flex-col gap-4 overflow-x-hidden"
    >
      <MarketView
        sources={[
          ...sources.map((item) => ({
            value: item.id,
            label:
              item.id === "community"
                ? t("skillMarket.sourceCommunity")
                : t("skillMarket.sourceAnthropic"),
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
        onRefresh={() => void loadSkills()}
        searchPlaceholder={t("skillMarket.searchPlaceholder")}
        isLoading={isLoading}
        summary={t("skillMarket.countSummary", {
          shown: skills.length,
          total,
        })}
        partialLabel={hasMore ? t("skillMarket.partial") : undefined}
        error={error}
        emptyTitle={t("skillMarket.empty")}
        items={items}
        currentPage={safeCurrentPage}
        totalPages={computedTotalPages}
        onPageChange={(page) => {
          setCurrentPage(page);
          clearResults(true);
        }}
        previousLabel={t("skillMarket.previous")}
        nextLabel={t("skillMarket.next")}
      />
    </Shell>
  );
};

export default SkillMarket;
