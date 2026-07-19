import React, { useEffect, useMemo, useState } from "react";
import { Navigate, useNavigate, useParams } from "react-router-dom";
import CodeMirror from "@uiw/react-codemirror";
import { javascript } from "@codemirror/lang-javascript";
import { EditorView } from "@codemirror/view";
import { oneDark } from "@codemirror/theme-one-dark";
import {
  Button,
  Input,
  Label,
  RadioGroup,
  RadioGroupItem,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  Switch,
} from "@mcp_link/ui";
import { ArrowLeft, Check, RefreshCw, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { v4 as uuidv4 } from "uuid";

import PageLayout from "@/renderer/components/layout/PageLayout";
import { usePlatformAPI } from "@/renderer/platform-api";
import {
  HOOK_METHODS,
  HookMethod,
  HookRule,
  HookTiming,
  toHookRule,
  toWorkflowCreateInput,
  toWorkflowUpdateInput,
} from "./hook-rule-adapter";

const DEFAULT_SCRIPT = `// context 包含 method、params、previousResults
return {
  method: context.method,
  params: context.params,
};`;

type HookEditPageProps = {
  mode?: "new" | "edit";
};

const HookEditPage: React.FC<HookEditPageProps> = ({ mode = "edit" }) => {
  const { t } = useTranslation();
  const platformAPI = usePlatformAPI();
  const navigate = useNavigate();
  const { id } = useParams<{ id: string }>();
  const isNew = mode === "new";
  const workflowId = isNew ? undefined : id;
  const [rule, setRule] = useState<HookRule | null>(
    isNew ? createEmptyRule(t("hooks.newHookName")) : null,
  );
  const [isLoading, setIsLoading] = useState(!isNew);
  const [isSaving, setIsSaving] = useState(false);
  const [notFound, setNotFound] = useState(false);

  const editorExtensions = useMemo(
    () => [javascript({ typescript: true }), EditorView.lineWrapping],
    [],
  );

  useEffect(() => {
    if (!workflowId || isNew) return;

    let cancelled = false;
    const loadRule = async () => {
      setIsLoading(true);
      setNotFound(false);
      try {
        const workflow = await platformAPI.workflows.workflows.get(workflowId);
        if (cancelled) return;
        if (!workflow) {
          setNotFound(true);
          setRule(null);
          return;
        }
        const nextRule = toHookRule(workflow);
        if (!nextRule) {
          setNotFound(true);
          setRule(null);
        } else {
          setRule(nextRule);
        }
      } catch (error) {
        toast.error(
          error instanceof Error ? error.message : t("hooks.loadFailed"),
        );
      } finally {
        if (!cancelled) setIsLoading(false);
      }
    };

    void loadRule();
    return () => {
      cancelled = true;
    };
  }, [workflowId, isNew, platformAPI, t]);

  const updateRule = (updates: Partial<HookRule>) => {
    setRule((current) => (current ? { ...current, ...updates } : current));
  };

  const goBack = () => navigate("/hooks");

  const handleSave = async () => {
    if (!rule || (!isNew && !workflowId)) return;
    if (!rule.name.trim()) {
      toast.error(t("hooks.nameRequired"));
      return;
    }

    setIsSaving(true);
    try {
      if (isNew) {
        await platformAPI.workflows.workflows.create(
          toWorkflowCreateInput({
            ...rule,
            name: rule.name.trim(),
          }),
        );
      } else {
        if (!workflowId) return;
        await platformAPI.workflows.workflows.update(
          workflowId,
          toWorkflowUpdateInput({
            ...rule,
            name: rule.name.trim(),
          }),
        );
      }
      toast.success(t("hooks.saveSuccess"));
      navigate("/hooks");
    } catch (error) {
      toast.error(
        error instanceof Error ? error.message : t("hooks.saveFailed"),
      );
    } finally {
      setIsSaving(false);
    }
  };

  if (!isNew && !workflowId) return <Navigate to="/hooks" replace />;

  if (isLoading) {
    return (
      <PageLayout title={t("hooks.title")}>
        <div className="text-sm text-muted-foreground">
          {t("common.loading")}
        </div>
      </PageLayout>
    );
  }

  if (notFound || !rule) {
    return (
      <PageLayout title={t("hooks.title")}>
        <div className="text-sm text-muted-foreground">
          {t("hooks.notFound")}{" "}
          <button
            type="button"
            className="text-primary"
            onClick={() => navigate("/hooks")}
          >
            {t("hooks.backToHooks")}
          </button>
        </div>
      </PageLayout>
    );
  }

  return (
    <PageLayout
      title={
        <span className="flex min-w-0 items-center gap-3">
          <Button
            type="button"
            variant="ghost"
            size="icon"
            className="h-8 w-8"
            onClick={goBack}
          >
            <ArrowLeft className="h-4 w-4" />
          </Button>
          <span className="truncate">
            {isNew ? t("hooks.addHook") : rule.name}
          </span>
        </span>
      }
      toolbar={
        <>
          <Button type="button" variant="outline" onClick={goBack}>
            <X className="h-4 w-4" />
            {t("common.cancel")}
          </Button>
          <Button type="button" onClick={handleSave} disabled={isSaving}>
            {isSaving ? (
              <RefreshCw className="h-4 w-4 animate-spin" />
            ) : (
              <Check className="h-4 w-4" />
            )}
            {t("common.save")}
          </Button>
        </>
      }
      contentClassName="max-w-3xl space-y-6"
    >
      <div className="grid gap-2">
        <Label htmlFor="hook-name">{t("hooks.name")}</Label>
        <Input
          id="hook-name"
          value={rule.name}
          onChange={(event) => updateRule({ name: event.target.value })}
          placeholder={t("hooks.namePlaceholder")}
        />
      </div>

      <div className="grid gap-2">
        <Label>{t("hooks.method")}</Label>
        <Select
          value={rule.method}
          onValueChange={(value) => updateRule({ method: value as HookMethod })}
        >
          <SelectTrigger>
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {HOOK_METHODS.map((method) => (
              <SelectItem key={method} value={method}>
                {method}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>

      <div className="grid gap-2">
        <Label>{t("hooks.timingLabel")}</Label>
        <RadioGroup
          value={rule.timing}
          onValueChange={(value) => updateRule({ timing: value as HookTiming })}
          className="grid gap-2 sm:grid-cols-2"
        >
          <label className="flex cursor-pointer items-center gap-2 rounded-md border p-3">
            <RadioGroupItem value="before" />
            <span className="text-sm">{t("hooks.timing.beforeMcp")}</span>
          </label>
          <label className="flex cursor-pointer items-center gap-2 rounded-md border p-3">
            <RadioGroupItem value="after" />
            <span className="text-sm">{t("hooks.timing.afterMcp")}</span>
          </label>
        </RadioGroup>
      </div>

      <div className="grid gap-2">
        <Label>{t("hooks.script")}</Label>
        <div className="overflow-hidden rounded-md border">
          <CodeMirror
            value={rule.script}
            height="360px"
            theme={oneDark}
            extensions={editorExtensions}
            onChange={(value) => updateRule({ script: value })}
            basicSetup={{
              lineNumbers: true,
              foldGutter: true,
              dropCursor: true,
              indentOnInput: true,
              bracketMatching: true,
              closeBrackets: true,
              autocompletion: true,
              highlightSelectionMatches: true,
            }}
          />
        </div>
      </div>

      <div className="flex items-center justify-between rounded-md border p-3">
        <Label htmlFor="hook-enabled">{t("hooks.enabled")}</Label>
        <Switch
          id="hook-enabled"
          checked={rule.enabled}
          onCheckedChange={(checked) => updateRule({ enabled: checked })}
        />
      </div>

      <div className="rounded-md border bg-muted/20 p-4">
        <div className="text-sm font-medium">{t("hooks.apiReference")}</div>
        <ul className="mt-2 space-y-1 text-sm text-muted-foreground">
          <li>{t("hooks.api.method")}</li>
          <li>{t("hooks.api.params")}</li>
          <li>{t("hooks.api.previousResults")}</li>
          <li>{t("hooks.api.returnValue")}</li>
        </ul>
      </div>
    </PageLayout>
  );
};

function createEmptyRule(name: string): HookRule {
  const now = Date.now();
  return {
    id: uuidv4(),
    name,
    enabled: true,
    method: "tools/call",
    timing: "before",
    script: DEFAULT_SCRIPT,
    createdAt: now,
    updatedAt: now,
  };
}

export default HookEditPage;
