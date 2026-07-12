import React, { useState, useCallback, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { useActivityData } from "./hooks/useActivityData";
import QueryWordCloud from "./components/QueryWordCloud";
import ActivityLog from "./components/ActivityLog";
import PageLayout from "@/renderer/components/layout/PageLayout";
import { Button } from "@mcp_link/ui";
import { RefreshCw } from "lucide-react";

interface LogViewerProps {
  heatmapDays?: number;
}
const getTodayString = (): string => {
  const today = new Date();
  return `${today.getFullYear()}-${String(today.getMonth() + 1).padStart(2, "0")}-${String(today.getDate()).padStart(2, "0")}`;
};

const LogViewer: React.FC<LogViewerProps> = () => {
  const { t } = useTranslation();

  const [selectedDate, setSelectedDate] = useState<string>(getTodayString());
  const [refreshTrigger, setRefreshTrigger] = useState<number>(0);

  const { wordCloudData, activityItems, loading, refetch } = useActivityData({
    selectedDate,
    refreshTrigger,
  });

  const handleRefresh = useCallback(() => {
    setRefreshTrigger((prev) => prev + 1);
  }, []);

  useEffect(() => {
    handleRefresh();
  }, [handleRefresh]);

  return (
    <PageLayout
      title={t("logs.activity.title", "Activity")}
      toolbar={
        <Button
          onClick={handleRefresh}
          variant="outline"
          size="sm"
          className="h-8 gap-2"
          aria-label={t("logs.viewer.refresh", "Refresh")}
        >
          <RefreshCw className="h-4 w-4" />
          {t("logs.viewer.refresh", "Refresh")}
        </Button>
      }
      contentClassName="flex flex-col overflow-hidden"
    >
      <div className="flex-1 grid grid-cols-1 lg:grid-cols-3 gap-4 min-h-0">
        {/* Word Cloud (1/3) */}
        <div className="lg:col-span-1">
          <QueryWordCloud data={wordCloudData} loading={loading} />
        </div>

        {/* Activity Log (2/3) */}
        <div className="lg:col-span-2 min-h-0">
          <ActivityLog items={activityItems} loading={loading} />
        </div>
      </div>
    </PageLayout>
  );
};

export default LogViewer;
