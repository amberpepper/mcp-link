import React from "react";
import { Alert, AlertDescription, Button } from "@mcp_link/ui";
import { ArrowLeft, Trash2 } from "lucide-react";
import { useTranslation } from "react-i18next";

import PageLayout from "@/renderer/components/layout/PageLayout";
import type { HookRule } from "./hook-rule-adapter";

interface LegacyHookViewProps {
  rule: HookRule;
  onBack: () => void;
  onDelete: () => void | Promise<void>;
}

const LegacyHookView: React.FC<LegacyHookViewProps> = ({
  rule,
  onBack,
  onDelete,
}) => {
  const { t } = useTranslation();

  return (
    <PageLayout
      title={
        <span className="flex min-w-0 items-center gap-3">
          <Button
            type="button"
            variant="ghost"
            size="icon"
            className="h-8 w-8"
            onClick={onBack}
          >
            <ArrowLeft className="h-4 w-4" />
          </Button>
          <span className="truncate">{rule.name}</span>
        </span>
      }
      toolbar={
        <Button
          type="button"
          variant="destructive"
          size="sm"
          onClick={onDelete}
        >
          <Trash2 className="h-4 w-4" />
          {t("common.delete")}
        </Button>
      }
      contentClassName="max-w-3xl space-y-4"
    >
      <Alert>
        <AlertDescription>{t("hooks.legacyDescription")}</AlertDescription>
      </Alert>
      <div className="rounded-md border p-4 text-sm text-muted-foreground">
        {t("hooks.legacySubtitle", { count: rule.nodeCount ?? 0 })}
      </div>
    </PageLayout>
  );
};

export default LegacyHookView;
