import React, { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Card, CardContent, CardHeader, CardTitle, Switch } from "@mcp_link/ui";
import type { AppSettings } from "@mcp_link/shared";

import PageLayout from "@/renderer/components/layout/PageLayout";
import { usePlatformAPI } from "@/renderer/platform-api";
import {
  MCP_MARKET_SOURCES,
  SKILL_MARKET_SOURCES,
  getMarketSourceEnabled,
  setMarketSourceEnabled,
  type MarketSourceDefinition,
} from "@/renderer/services/market-source-service";

const MarketSourcesPage: React.FC = () => {
  const { t } = useTranslation();
  const platformAPI = usePlatformAPI();
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [savingId, setSavingId] = useState<string | null>(null);

  const loadSettings = useCallback(async () => {
    const nextSettings = await platformAPI.settings.get();
    setSettings(nextSettings);
  }, [platformAPI]);

  useEffect(() => {
    void loadSettings();
  }, [loadSettings]);

  const toggleSource = async (
    source: MarketSourceDefinition,
    enabled: boolean,
  ) => {
    const current = settings ?? {};
    const nextSettings = setMarketSourceEnabled(current, source, enabled);
    setSettings(nextSettings);
    setSavingId(`${source.kind}:${source.id}`);
    try {
      await platformAPI.settings.save(nextSettings);
      toast.success(t("marketSources.saved"));
    } catch (error) {
      setSettings(current);
      toast.error(
        error instanceof Error ? error.message : t("marketSources.saveFailed"),
      );
    } finally {
      setSavingId(null);
    }
  };

  return (
    <PageLayout
      title={t("marketSources.title")}
      contentClassName="flex flex-col gap-6"
    >
      <p className="max-w-3xl text-sm text-muted-foreground">
        {t("marketSources.description")}
      </p>

      <SourceSection
        title={t("marketSources.mcpTitle")}
        sources={MCP_MARKET_SOURCES}
        settings={settings}
        savingId={savingId}
        onToggle={toggleSource}
      />

      <SourceSection
        title={t("marketSources.skillTitle")}
        sources={SKILL_MARKET_SOURCES}
        settings={settings}
        savingId={savingId}
        onToggle={toggleSource}
      />
    </PageLayout>
  );
};

interface SourceSectionProps {
  title: string;
  sources: readonly MarketSourceDefinition[];
  settings: AppSettings | null;
  savingId: string | null;
  onToggle: (source: MarketSourceDefinition, enabled: boolean) => void;
}

function SourceSection({
  title,
  sources,
  settings,
  savingId,
  onToggle,
}: SourceSectionProps) {
  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-xl">{title}</CardTitle>
      </CardHeader>
      <CardContent className="divide-y">
        {sources.map((source) => {
          const rowId = `${source.kind}:${source.id}`;
          const enabled = getMarketSourceEnabled(settings, source);
          return (
            <div
              key={rowId}
              className="flex items-center justify-between gap-4 py-4 first:pt-0 last:pb-0"
            >
              <div className="min-w-0 space-y-1">
                <div className="flex min-w-0 items-center gap-2">
                  <h3 className="truncate text-sm font-medium">
                    {source.label}
                  </h3>
                  <span className="truncate text-xs text-muted-foreground">
                    {source.url}
                  </span>
                </div>
                <p className="text-xs text-muted-foreground">
                  {source.description}
                </p>
              </div>
              <Switch
                checked={enabled}
                disabled={savingId === rowId}
                onCheckedChange={(checked) => onToggle(source, !!checked)}
              />
            </div>
          );
        })}
      </CardContent>
    </Card>
  );
}

export default MarketSourcesPage;
