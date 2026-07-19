import React, { useCallback, useEffect, useMemo, useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import { Button } from "@mcp_link/ui";
import { IconGitFork, IconPlus, IconSearch } from "@tabler/icons-react";
import { toast } from "sonner";
import { useTranslation } from "react-i18next";
import { v4 as uuidv4 } from "uuid";

import EmptyState from "@/renderer/components/common/EmptyState";
import PageLayout from "@/renderer/components/layout/PageLayout";
import { usePlatformAPI } from "@/renderer/platform-api";
import HookRow from "./HookRow";
import {
  HookRule,
  toHookRule,
  toWorkflowCreateInput,
} from "./hook-rule-adapter";

const HooksPage: React.FC = () => {
  const { t } = useTranslation();
  const platformAPI = usePlatformAPI();
  const navigate = useNavigate();
  const [rules, setRules] = useState<HookRule[]>([]);
  const [query, setQuery] = useState("");
  const [isLoading, setIsLoading] = useState(true);

  const loadHooks = useCallback(async () => {
    setIsLoading(true);
    try {
      const workflows = await platformAPI.workflows.workflows.list();
      setRules(
        workflows
          .map(toHookRule)
          .filter((rule): rule is HookRule => rule !== null)
          .sort((a, b) => a.createdAt - b.createdAt),
      );
    } catch (error) {
      toast.error(
        error instanceof Error ? error.message : t("hooks.loadFailed"),
      );
    } finally {
      setIsLoading(false);
    }
  }, [platformAPI, t]);

  useEffect(() => {
    void loadHooks();
  }, [loadHooks]);

  const filteredRules = useMemo(() => {
    const normalized = query.trim().toLowerCase();
    if (!normalized) return rules;
    return rules.filter(
      (rule) =>
        rule.name.toLowerCase().includes(normalized) ||
        rule.method.toLowerCase().includes(normalized),
    );
  }, [query, rules]);

  const toggleEnabled = async (rule: HookRule, checked: boolean) => {
    try {
      await platformAPI.workflows.workflows.update(rule.id, {
        enabled: checked,
      });
      setRules((current) =>
        current.map((item) =>
          item.id === rule.id ? { ...item, enabled: checked } : item,
        ),
      );
    } catch (error) {
      toast.error(
        error instanceof Error ? error.message : t("hooks.updateFailed"),
      );
    }
  };

  const duplicateRule = async (rule: HookRule) => {
    try {
      const now = Date.now();
      await platformAPI.workflows.workflows.create(
        toWorkflowCreateInput({
          ...rule,
          id: uuidv4(),
          name: t("hooks.copyName", { name: rule.name }),
          enabled: false,
          createdAt: now,
          updatedAt: now,
        }),
      );
      await loadHooks();
      toast.success(t("hooks.duplicateSuccess"));
    } catch (error) {
      toast.error(
        error instanceof Error ? error.message : t("hooks.duplicateFailed"),
      );
    }
  };

  const deleteRule = async (rule: HookRule) => {
    try {
      await platformAPI.workflows.workflows.delete(rule.id);
      setRules((current) => current.filter((item) => item.id !== rule.id));
      toast.success(t("hooks.deleteSuccess"));
    } catch (error) {
      toast.error(
        error instanceof Error ? error.message : t("hooks.deleteFailed"),
      );
    }
  };

  return (
    <PageLayout
      title={t("hooks.title")}
      toolbar={
        <div className="flex min-w-0 flex-1 flex-wrap items-center justify-end gap-2">
          <div className="relative min-w-[220px] flex-1 max-w-md">
            <input
              type="text"
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder={t("hooks.searchPlaceholder")}
              className="w-full rounded-md border border-border bg-background py-1.5 pl-8 pr-3 text-sm focus:outline-none focus:ring-1 focus:ring-primary"
            />
            <IconSearch className="absolute left-2.5 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
          </div>
          <Button asChild size="sm" className="h-8">
            <Link to="/hooks/new">
              <IconPlus className="h-4 w-4" />
              {t("hooks.addHook")}
            </Link>
          </Button>
        </div>
      }
      contentClassName="flex flex-col overflow-hidden"
    >
      {isLoading ? (
        <div className="p-6 text-sm text-muted-foreground">
          {t("common.loading")}
        </div>
      ) : rules.length === 0 ? (
        <EmptyState
          icon={IconGitFork}
          title={t("hooks.empty")}
          description={t("hooks.emptyDescription")}
          action={
            <Button asChild>
              <Link to="/hooks/new">
                <IconPlus className="h-4 w-4" />
                {t("hooks.addHook")}
              </Link>
            </Button>
          }
        />
      ) : filteredRules.length === 0 ? (
        <EmptyState
          icon={IconSearch}
          title={t("hooks.noMatches")}
          description={t("hooks.noMatchesDescription", { query })}
        />
      ) : (
        <div className="min-h-0 flex-1 overflow-hidden rounded-md border">
          <div className="divide-y divide-border">
            {filteredRules.map((rule) => (
              <HookRow
                key={rule.id}
                rule={rule}
                onClick={() => navigate(`/hooks/${rule.id}`)}
                onToggle={(checked) => toggleEnabled(rule, checked)}
                onDuplicate={() => duplicateRule(rule)}
                onDelete={() => deleteRule(rule)}
              />
            ))}
          </div>
        </div>
      )}
    </PageLayout>
  );
};

export default HooksPage;
